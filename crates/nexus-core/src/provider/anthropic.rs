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
    /// Compresses whitespace, removes trailing whitespace per line, and collapses 2+ consecutive blank lines into 1.
    pub fn compress_whitespace(text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();
        let mut last_was_blank = false;

        for line in trimmed.lines() {
            let t = line.trim_end();
            if t.is_empty() {
                if !last_was_blank {
                    lines.push("");
                    last_was_blank = true;
                }
            } else {
                lines.push(t);
                last_was_blank = false;
            }
        }

        lines.join("\n")
    }

    /// Tier 2: Mid-history compaction (3-5 turns ago). Keeps 4 head and 4 tail lines.
    pub fn compact_mid_historical_output(output: &str) -> String {
        let compressed = Self::compress_whitespace(output);
        let lines: Vec<&str> = compressed.lines().collect();
        if lines.len() <= 12 && compressed.len() <= 600 {
            return compressed;
        }

        let head: Vec<&str> = lines.iter().take(4).copied().collect();
        let tail: Vec<&str> = lines.iter().rev().take(4).rev().copied().collect();
        let omitted = lines.len().saturating_sub(8);

        format!(
            "{}\n[... {} lines omitted in conversation history for token efficiency ...]\n{}",
            head.join("\n"),
            omitted,
            tail.join("\n")
        )
    }

    /// Tier 3: Deep-history compaction (>5 turns ago).
    /// If success: collapses to a 1-line execution tombstone saving >95% tokens.
    /// If error: preserves the first 2 error lines.
    pub fn compact_deep_historical_output(output: &str, is_error: bool) -> String {
        let compressed = Self::compress_whitespace(output);
        let lines: Vec<&str> = compressed.lines().collect();
        let count = lines.len();

        if count <= 2 && compressed.len() <= 120 {
            return compressed;
        }

        if is_error {
            let error_head: Vec<&str> = lines.iter().take(2).copied().collect();
            format!(
                "{}\n[... {} lines of error output omitted ...]",
                error_head.join("\n"),
                count.saturating_sub(2)
            )
        } else {
            format!("[Command/Tool succeeded. {} lines of past output omitted]", count)
        }
    }

    /// Backwards compatible compaction helper
    pub fn compact_historical_output(output: &str) -> String {
        Self::compact_mid_historical_output(output)
    }

    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .pool_idle_timeout(Some(std::time::Duration::from_secs(5)))
                .pool_max_idle_per_host(1)
                .tcp_nodelay(true)
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            api_key,
            model: model.unwrap_or_else(|| "claude-3-7-sonnet-20250219".to_string()),
            base_url: "https://api.anthropic.com".to_string(),
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
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn with_model(&self, model: &str) -> Option<std::sync::Arc<dyn LlmProvider>> {
        Some(std::sync::Arc::new(self.for_model(model)))
    }

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

        // Format messages for Anthropic API with 3-tier progressive token compactor
        let history_len = history.len();
        let mut formatted_messages: Vec<serde_json::Value> = history
            .iter()
            .enumerate()
            .map(|(idx, m)| {
                let turns_from_end = history_len.saturating_sub(idx + 1);
                if let Some(tool_results) = &m.tool_results {
                    // Tool result turn with 3-tier progressive compactor
                    let content_blocks: Vec<serde_json::Value> = tool_results
                        .iter()
                        .map(|tr| {
                            let content_to_send = if turns_from_end <= 1 {
                                // Tier 1 (recent/immediate: 0-1 turns ago): fresh output with whitespace compression
                                Self::compress_whitespace(&tr.output)
                            } else if turns_from_end <= 3 {
                                // Tier 2 (mid history: 2-3 turns ago): 4 head + 4 tail lines
                                Self::compact_mid_historical_output(&tr.output)
                            } else {
                                // Tier 3 (deep history: 4+ turns ago): 1-line tombstone summary
                                Self::compact_deep_historical_output(&tr.output, tr.is_error)
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
                                usage.input_tokens = usage.input_tokens.max(inp as u32);
                            }
                            if let Some(cr) = u.get("cache_read_input_tokens").and_then(|v| v.as_u64()) {
                                usage.cache_read_tokens = usage.cache_read_tokens.max(cr as u32);
                            }
                            if let Some(cc) = u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()) {
                                usage.cache_creation_tokens = usage.cache_creation_tokens.max(cc as u32);
                            }
                        }
                    }
                }
                "message_delta" => {
                    if let Some(u) = data.get("usage") {
                        if let Some(out) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                            usage.output_tokens = usage.output_tokens.max(out as u32);
                        }
                        if let Some(inp) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                            usage.input_tokens = usage.input_tokens.max(inp as u32);
                        }
                        if let Some(cr) = u.get("cache_read_input_tokens").and_then(|v| v.as_u64()) {
                            usage.cache_read_tokens = usage.cache_read_tokens.max(cr as u32);
                        }
                        if let Some(cc) = u.get("cache_creation_input_tokens").and_then(|v| v.as_u64()) {
                            usage.cache_creation_tokens = usage.cache_creation_tokens.max(cc as u32);
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
                        let call = ToolCall::new(
                            current_tool_id.clone(),
                            current_tool_name.clone(),
                            parsed_args,
                        );
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
                    // Do not break early; proxy endpoints may stream final message_delta after stop
                }
                _ => {}
            }
        }

        // Fallback estimation if third-party proxy omitted usage tokens
        if usage.output_tokens == 0 {
            let tool_json_chars: usize = tool_calls.iter().map(|tc| tc.arguments.to_string().len()).sum();
            let total_out = full_text.len() + tool_json_chars;
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
    fn test_compact_historical_output() {
        let short = "Line 1\nLine 2";
        assert_eq!(AnthropicProvider::compact_historical_output(short), short);

        let lines: Vec<String> = (1..=100).map(|i| format!("Historical compiler output line {}", i)).collect();
        let long_output = lines.join("\n");
        let compacted = AnthropicProvider::compact_mid_historical_output(&long_output);

        assert!(compacted.contains("Historical compiler output line 1"));
        assert!(compacted.contains("Historical compiler output line 100"));
        assert!(compacted.contains("92 lines omitted in conversation history for token efficiency"));

        // Test deep compaction (Tier 3)
        let deep_success = AnthropicProvider::compact_deep_historical_output(&long_output, false);
        assert!(deep_success.contains("[Command/Tool succeeded. 100 lines of past output omitted]"));

        let deep_error = AnthropicProvider::compact_deep_historical_output(&long_output, true);
        assert!(deep_error.contains("Historical compiler output line 1"));
        assert!(deep_error.contains("98 lines of error output omitted"));

        // Test whitespace compression
        let messy = "line 1   \n\n\n\nline 2\n\n\n";
        let cleaned = AnthropicProvider::compress_whitespace(messy);
        assert_eq!(cleaned, "line 1\n\nline 2");
    }
}
