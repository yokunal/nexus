use super::{LlmProvider, ProviderError};
use crate::events::{NexusEvent, TokenUsage};
use crate::session::Message;
use crate::tools::ToolCall;
use async_trait::async_trait;
use chrono::Utc;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

/// Native Google Gemini provider speaking Google's OpenAI-compatible endpoint.
/// Endpoint: https://generativelanguage.googleapis.com/v1beta/openai/chat/completions
/// Provides access to Gemini 2.0 Flash / 1.5 Flash with official free quota.
pub struct GeminiProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        let mut default_model = model.unwrap_or_else(|| "gemini-2.5-flash".to_string());
        if default_model == "claude-3-7-sonnet-20250219" || default_model == "gemini-2.0-flash" {
            default_model = "gemini-2.5-flash".to_string();
        }
        Self {
            client: reqwest::Client::builder()
                .pool_idle_timeout(Some(std::time::Duration::from_secs(5)))
                .pool_max_idle_per_host(1)
                .tcp_nodelay(true)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_key,
            model: default_model,
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    pub fn for_model(&self, model: &str) -> Self {
        Self {
            client: self.client.clone(),
            api_key: self.api_key.clone(),
            model: model.to_string(),
            base_url: self.base_url.clone(),
        }
    }

    /// Converts Anthropic-style tool definitions (input_schema) into OpenAI/Gemini function tool format.
    pub fn convert_tools_to_openai(tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                let name = t.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                let desc = t.get("description").and_then(|v| v.as_str()).unwrap_or_default();
                let schema = t.get("input_schema").cloned().unwrap_or_else(|| json!({"type": "object"}));

                json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": desc,
                        "parameters": schema
                    }
                })
            })
            .collect()
    }

    /// Converts Nexus internal Message history into OpenAI/Gemini chat messages.
    pub fn format_messages_for_gemini(system_prompt: &str, history: &[Message]) -> Vec<Value> {
        let mut messages = Vec::new();

        if !system_prompt.trim().is_empty() {
            messages.push(json!({
                "role": "system",
                "content": system_prompt
            }));
        }

        for msg in history {
            if let Some(tool_results) = &msg.tool_results {
                for tr in tool_results {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tr.call_id,
                        "content": tr.output
                    }));
                }
            } else if let Some(tool_calls) = &msg.tool_calls {
                let calls_val: Vec<Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        let mut call_obj = json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": tc.arguments.to_string()
                            }
                        });
                        if let Some(extra) = &tc.extra_content {
                            call_obj["extra_content"] = extra.clone();
                        }
                        call_obj
                    })
                    .collect();

                messages.push(json!({
                    "role": "assistant",
                    "content": if msg.content.is_empty() { Value::Null } else { json!(msg.content) },
                    "tool_calls": calls_val
                }));
            } else {
                messages.push(json!({
                    "role": msg.role,
                    "content": msg.content
                }));
            }
        }

        messages
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn with_model(&self, model: &str) -> Option<Arc<dyn LlmProvider>> {
        Some(Arc::new(self.for_model(model)))
    }

    async fn stream_turn(
        &self,
        system_prompt: &str,
        history: &[Message],
        tools: &[Value],
        event_tx: Sender<NexusEvent>,
    ) -> Result<Message, ProviderError> {
        let clean_key = self.api_key.trim();
        let mut headers = HeaderMap::new();
        if let Ok(bearer_val) = HeaderValue::from_str(&format!("Bearer {}", clean_key)) {
            headers.insert("authorization", bearer_val);
        }
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let messages = Self::format_messages_for_gemini(system_prompt, history);
        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
            "stream_options": {
                "include_usage": true
            }
        });

        if !tools.is_empty() {
            let gemini_tools = Self::convert_tools_to_openai(tools);
            body["tools"] = json!(gemini_tools);
        }

        let base = self.base_url.trim_end_matches('/');
        let endpoint_url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{}/chat/completions", base)
        };

        let response = self
            .client
            .post(&endpoint_url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let err_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown API error".to_string());
            return Err(ProviderError::Api {
                status,
                message: err_body,
            });
        }

        let mut full_text = String::new();
        let mut tool_calls_map: BTreeMap<usize, (String, String, String, Option<Value>)> = BTreeMap::new();
        let mut usage = TokenUsage::default();

        let mut stream = response.bytes_stream().eventsource();

        while let Some(event) = stream.next().await {
            let event = event.map_err(|e| ProviderError::Stream(e.to_string()))?;
            let raw_data = event.data.trim();

            if raw_data == "[DONE]" {
                break;
            }

            let data: Value = match serde_json::from_str(raw_data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Parse usage if included
            if let Some(u) = data.get("usage") {
                if let Some(prompt) = u.get("prompt_tokens").and_then(|v| v.as_u64()) {
                    usage.input_tokens = prompt as u32;
                }
                if let Some(comp) = u.get("completion_tokens").and_then(|v| v.as_u64()) {
                    usage.output_tokens = comp as u32;
                }
            }

            // Parse delta chunks
            if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                if let Some(choice) = choices.first() {
                    if let Some(delta) = choice.get("delta") {
                        // Text delta
                        if let Some(text) = delta.get("content").and_then(|t| t.as_str()) {
                            full_text.push_str(text);
                            let _ = event_tx
                                .send(NexusEvent::TokenChunk(text.to_string()))
                                .await;
                        }

                        // Tool calls delta
                        if let Some(calls) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                            for c in calls {
                                let idx = c.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                let entry = tool_calls_map.entry(idx).or_insert_with(|| {
                                    (
                                        format!("call_{}", &uuid::Uuid::new_v4().to_string()[..8]),
                                        String::new(),
                                        String::new(),
                                        None,
                                    )
                                });

                                if let Some(id) = c.get("id").and_then(|i| i.as_str()) {
                                    if !id.is_empty() {
                                        entry.0 = id.to_string();
                                    }
                                }

                                if let Some(extra) = c.get("extra_content") {
                                    entry.3 = Some(extra.clone());
                                }

                                if let Some(func) = c.get("function") {
                                    if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                        entry.1.push_str(name);
                                    }
                                    if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                        entry.2.push_str(args);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Assemble tool calls
        let mut tool_calls = Vec::new();
        for (_, (id, name, args_str, extra_content)) in tool_calls_map {
            if !name.is_empty() {
                let parsed_args = serde_json::from_str::<Value>(&args_str).unwrap_or_else(|_| json!({}));
                let call = ToolCall {
                    id,
                    name,
                    arguments: parsed_args,
                    extra_content,
                };
                let _ = event_tx.send(NexusEvent::ToolCallStarted(call.clone())).await;
                tool_calls.push(call);
            }
        }

        // Token fallback estimation
        if usage.output_tokens == 0 {
            let total_out = full_text.len();
            if total_out > 0 {
                usage.output_tokens = total_out.div_ceil(4).max(1) as u32;
            }
        }
        if usage.input_tokens == 0 {
            let est_input_chars: usize = history.iter().map(|m| m.content.len()).sum();
            usage.input_tokens = (est_input_chars + system_prompt.len()).div_ceil(4).max(10) as u32;
        }

        let _ = event_tx.send(NexusEvent::TokenUsageRecorded(usage)).await;
        let _ = event_tx.send(NexusEvent::TurnComplete).await;

        Ok(Message {
            role: "assistant".to_string(),
            content: full_text,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_results: None,
            timestamp: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_tools_to_gemini_openai() {
        let anthropic_tools = vec![json!({
            "name": "read_file",
            "description": "Reads file content",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }
        })];

        let converted = GeminiProvider::convert_tools_to_openai(&anthropic_tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["function"]["name"], "read_file");
        assert_eq!(converted[0]["function"]["parameters"]["required"][0], "path");
    }

    #[test]
    fn test_format_messages_for_gemini() {
        let history = vec![
            Message {
                role: "user".to_string(),
                content: "Hello Gemini".to_string(),
                tool_calls: None,
                tool_results: None,
                timestamp: Utc::now(),
            },
            Message {
                role: "assistant".to_string(),
                content: "".to_string(),
                tool_calls: Some(vec![ToolCall::new(
                    "call_abc",
                    "read_file",
                    json!({"path": "Cargo.toml"}),
                )]),
                tool_results: None,
                timestamp: Utc::now(),
            },
        ];

        let formatted = GeminiProvider::format_messages_for_gemini("You are Nexus.", &history);
        assert_eq!(formatted.len(), 3); // system + user + assistant tool_calls
        assert_eq!(formatted[0]["role"], "system");
        assert_eq!(formatted[1]["role"], "user");
        assert_eq!(formatted[2]["role"], "assistant");
        assert_eq!(formatted[2]["tool_calls"][0]["function"]["name"], "read_file");
    }
}
