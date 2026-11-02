use super::{LlmProvider, ProviderError};
use crate::events::NexusEvent;
use crate::session::Message;
use crate::tools::ToolCall;
use async_trait::async_trait;
use chrono::Utc;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde_json::json;
use tokio::sync::mpsc::Sender;

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Intelligent head+tail compaction for historical tool results to prevent context window bloat.
    fn compact_historical_output(output: &str) -> String {
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() <= 24 && output.len() <= 1200 {
            return output.to_string();
        }

        let head: Vec<&str> = lines.iter().take(6).copied().collect();
        let tail: Vec<&str> = lines.iter().rev().take(6).rev().copied().collect();
        let omitted = lines.len().saturating_sub(12);

        format!(
            "{}\n[... {} lines omitted in conversation history for token efficiency ...]\n{}",
            head.join("\n"),
            omitted,
            tail.join("\n")
        )
    }

    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: model.unwrap_or_else(|| "claude-3-7-sonnet-20250219".to_string()),
            base_url: "https://api.anthropic.com".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn stream_turn(
        &self,
        system_prompt: &str,
        history: &[Message],
        tools: &[serde_json::Value],
        event_tx: Sender<NexusEvent>,
    ) -> Result<Message, ProviderError> {
        let clean_key = self.api_key.trim();
        let mut headers = HeaderMap::new();
        if let Ok(key_val) = HeaderValue::from_str(clean_key) {
            headers.insert("x-api-key", key_val);
        }
        if let Ok(bearer_val) = HeaderValue::from_str(&format!("Bearer {}", clean_key)) {
            headers.insert("authorization", bearer_val);
        }
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("prompt-caching-2024-07-31"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // Format messages for Anthropic API with token efficiency compactor
        let history_len = history.len();
        let mut formatted_messages: Vec<serde_json::Value> = history
            .iter()
            .enumerate()
            .map(|(idx, m)| {
                let is_historical = idx < history_len.saturating_sub(2);
                if let Some(tool_results) = &m.tool_results {
                    // Tool result turn
                    let content_blocks: Vec<serde_json::Value> = tool_results
                        .iter()
                        .map(|tr| {
                            let content_to_send = if is_historical {
                                Self::compact_historical_output(&tr.output)
                            } else {
                                tr.output.clone()
                            };
                            json!({
                                "type": "tool_result",
                                "tool_use_id": tr.call_id,
                                "content": content_to_send,
                                "is_error": tr.is_error
                            })
                        })
                        .collect();
                    json!({
                        "role": "user",
                        "content": content_blocks
                    })
                } else if let Some(tool_calls) = &m.tool_calls {
                    // Assistant tool call turn
                    let mut content_blocks = vec![];
                    if !m.content.is_empty() {
                        content_blocks.push(json!({
                            "type": "text",
                            "text": m.content
                        }));
                    }
                    for tc in tool_calls {
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments
                        }));
                    }
                    json!({
                        "role": "assistant",
                        "content": content_blocks
                    })
                } else {
                    json!({
                        "role": m.role,
                        "content": m.content
                    })
                }
            })
            .collect();

        // Attach ephemeral prompt cache breakpoint to the last message if history is present
        if let Some(last_msg) = formatted_messages.last_mut() {
            if let Some(content_arr) = last_msg.get_mut("content").and_then(|c| c.as_array_mut()) {
                if let Some(last_block) = content_arr.last_mut() {
                    if let Some(obj) = last_block.as_object_mut() {
                        obj.insert("cache_control".to_string(), json!({ "type": "ephemeral" }));
                    }
                }
            } else if let Some(content_str) = last_msg.get("content").and_then(|c| c.as_str()) {
                last_msg["content"] = json!([
                    {
                        "type": "text",
                        "text": content_str,
                        "cache_control": { "type": "ephemeral" }
                    }
                ]);
            }
        }

        // Ephemeral caching on system prompt block
        let system_block = json!([
            {
                "type": "text",
                "text": system_prompt,
                "cache_control": { "type": "ephemeral" }
            }
        ]);

        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": system_block,
            "messages": formatted_messages,
            "stream": true,
        });

        // Ephemeral caching on the tools definitions (cache on the last tool)
        if !tools.is_empty() {
            let mut cached_tools = tools.to_vec();
            if let Some(last_tool) = cached_tools.last_mut() {
                if let Some(obj) = last_tool.as_object_mut() {
                    obj.insert("cache_control".to_string(), json!({ "type": "ephemeral" }));
                }
            }
            body["tools"] = json!(cached_tools);
        }

        let base = self.base_url.trim_end_matches('/');
        let endpoint_url = if base.ends_with("/v1") {
            format!("{}/messages", base)
        } else if base.ends_with("/v1/messages") {
            base.to_string()
        } else {
            format!("{}/v1/messages", base)
        };

        let response = self
            .client
            .post(endpoint_url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status,
                message: text,
            });
        }

        let mut stream = response.bytes_stream().eventsource();
        let mut full_text = String::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_json = String::new();
        let mut tool_calls = Vec::new();
        let mut usage = crate::events::TokenUsage::default();

        while let Some(event) = stream.next().await {
            let event = event.map_err(|e| ProviderError::Stream(e.to_string()))?;
            let data: serde_json::Value = match serde_json::from_str(&event.data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event_type = data["type"].as_str().unwrap_or_default();

            match event_type {
                "message_start" => {
                    if let Some(msg) = data.get("message") {
                        if let Some(u) = msg.get("usage") {
                            if let Some(inp) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                                usage.input_tokens = inp as u32;
                            }
                            if let Some(cr) = u.get("cache_read_input_tokens").and_then(|v| v.as_u64()) {
                                usage.cache_read_tokens = cr as u32;
                            }
                            if let Some(cc) = u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()) {
                                usage.cache_creation_tokens = cc as u32;
                            }
                        }
                    }
                }
                "message_delta" => {
                    if let Some(u) = data.get("usage") {
                        if let Some(out) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                            usage.output_tokens = out as u32;
                        }
                    }
                }
                "content_block_start" => {
                    let block = &data["content_block"];
                    if block["type"].as_str() == Some("tool_use") {
                        current_tool_id = block["id"].as_str().unwrap_or_default().to_string();
                        current_tool_name = block["name"].as_str().unwrap_or_default().to_string();
                        current_tool_json.clear();
                    }
                }
                "content_block_delta" => {
                    let delta = &data["delta"];
                    if let Some(text) = delta["text"].as_str() {
                        full_text.push_str(text);
                        let _ = event_tx
                            .send(NexusEvent::TokenChunk(text.to_string()))
                            .await;
                    } else if let Some(json_partial) = delta["partial_json"].as_str() {
                        current_tool_json.push_str(json_partial);
                    }
                }
                "content_block_stop" => {
                    if !current_tool_name.is_empty() {
                        let parsed_args =
                            serde_json::from_str(&current_tool_json).unwrap_or(json!({}));
                        let call = ToolCall {
                            id: current_tool_id.clone(),
                            name: current_tool_name.clone(),
                            arguments: parsed_args,
                        };
                        let _ = event_tx
                            .send(NexusEvent::ToolCallStarted(call.clone()))
                            .await;
                        tool_calls.push(call);

                        current_tool_id.clear();
                        current_tool_name.clear();
                        current_tool_json.clear();
                    }
                }
                "message_stop" => {
                    break;
                }
                _ => {}
            }
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
    fn test_compact_historical_output() {
        let short = "Line 1\nLine 2";
        assert_eq!(AnthropicProvider::compact_historical_output(short), short);

        let lines: Vec<String> = (1..=100).map(|i| format!("Historical compiler output line {}", i)).collect();
        let long_output = lines.join("\n");
        let compacted = AnthropicProvider::compact_historical_output(&long_output);

        assert!(compacted.contains("Historical compiler output line 1"));
        assert!(compacted.contains("Historical compiler output line 100"));
        assert!(compacted.contains("88 lines omitted in conversation history for token efficiency"));
    }
}
