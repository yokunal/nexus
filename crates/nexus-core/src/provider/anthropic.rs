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
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_str(&self.api_key).unwrap());
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // Format messages for Anthropic API
        let formatted_messages: Vec<serde_json::Value> = history
            .iter()
            .map(|m| {
                if let Some(tool_results) = &m.tool_results {
                    // Tool result turn
                    let content_blocks: Vec<serde_json::Value> = tool_results
                        .iter()
                        .map(|tr| {
                            json!({
                                "type": "tool_result",
                                "tool_use_id": tr.call_id,
                                "content": tr.output,
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

        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": formatted_messages,
            "stream": true,
        });

        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }

        let response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
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

        while let Some(event) = stream.next().await {
            let event = event.map_err(|e| ProviderError::Stream(e.to_string()))?;
            let data: serde_json::Value = match serde_json::from_str(&event.data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event_type = data["type"].as_str().unwrap_or_default();

            match event_type {
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
