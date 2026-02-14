// Content classification and summarization module
// Uses a local Ollama instance for LLM inference via the REST API.

use crate::connectors::Post;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub sentiment: Option<String>,
    pub confidence: f32,
}

impl Default for Classification {
    fn default() -> Self {
        Self {
            categories: vec!["Other".to_string()],
            tags: vec![],
            sentiment: None,
            confidence: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Ollama request / response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    name: String,
}

// ---------------------------------------------------------------------------
// Classifier
// ---------------------------------------------------------------------------

pub struct Classifier {
    ollama_url: String,
    model: String,
    http_client: reqwest::Client,
}

impl Classifier {
    /// Create a new `Classifier`.
    ///
    /// * `ollama_url` - Base URL for the Ollama REST API. Defaults to
    ///   `http://localhost:11434` when `None`.
    /// * `model` - Model name to use. Defaults to `llama3.2` when `None`.
    pub fn new(ollama_url: Option<String>, model: Option<String>) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build reqwest client");

        Self {
            ollama_url: ollama_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            model: model.unwrap_or_else(|| "llama3.2".to_string()),
            http_client,
        }
    }

    /// Builder-pattern setter for the model name.
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    // -----------------------------------------------------------------------
    // Availability / discovery
    // -----------------------------------------------------------------------

    /// Check whether the Ollama server is reachable.
    pub async fn is_available(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.ollama_url);
        match self.http_client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Return the list of model names available on the Ollama server.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.ollama_url);
        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Ollama is not running at {}. Please start Ollama first.",
                    self.ollama_url
                )
            })?;

        let tags: TagsResponse = resp.json().await.context("failed to parse /api/tags response")?;
        let names = tags.models.into_iter().map(|m| m.name).collect();
        Ok(names)
    }

    // -----------------------------------------------------------------------
    // Core functionality
    // -----------------------------------------------------------------------

    /// Classify a post into categories, tags, and sentiment using the LLM.
    pub async fn classify_post(&self, post: &Post) -> Result<Classification> {
        let system_prompt = "You are a content classifier. Analyze the given post and return \
            a JSON object with: categories (array of 1-3 category strings from: Technology, \
            Politics, Science, Business, Entertainment, Sports, Health, Education, Environment, \
            Culture, Other), tags (array of 2-5 relevant keyword tags), sentiment (one of: \
            positive, negative, neutral, mixed), confidence (float 0.0-1.0). Return ONLY valid \
            JSON, no other text.";

        let user_message = format!(
            "Classify this post:\n\nAuthor: {}\nContent: {}",
            post.author, post.content
        );

        let body = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_message,
                },
            ],
            stream: false,
            format: Some("json".to_string()),
        };

        let raw = self.send_chat(body).await?;

        // Attempt to parse the structured JSON returned by the model.
        match serde_json::from_str::<Classification>(&raw) {
            Ok(c) => Ok(c),
            Err(_) => {
                // The model may have returned valid JSON that wraps the fields
                // differently. Try extracting from a generic Value.
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let categories = value
                        .get("categories")
                        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                        .unwrap_or_default();
                    let tags = value
                        .get("tags")
                        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
                        .unwrap_or_default();
                    let sentiment = value
                        .get("sentiment")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let confidence = value
                        .get("confidence")
                        .and_then(|v| v.as_f64())
                        .map(|f| f as f32)
                        .unwrap_or(0.0);

                    Ok(Classification {
                        categories,
                        tags,
                        sentiment,
                        confidence,
                    })
                } else {
                    // Completely unparseable -- return a safe default.
                    Ok(Classification::default())
                }
            }
        }
    }

    /// Generate a concise 1-2 sentence summary of a post.
    pub async fn summarize_post(&self, post: &Post) -> Result<String> {
        let system_prompt = "You are a content summarizer. Provide a concise 1-2 sentence \
            summary of the given post. Be direct and informative.";

        let body = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: post.content.clone(),
                },
            ],
            stream: false,
            format: None,
        };

        self.send_chat(body).await
    }

    /// Generate a derivative post that adds perspective or insight, kept under
    /// 280 characters.
    pub async fn generate_derivative(&self, post: &Post) -> Result<String> {
        let system_prompt = "You are a social media content creator. Based on the given post, \
            create an original derivative post that adds your own perspective or insight. Keep \
            it under 280 characters. Be engaging but professional.";

        let user_message = format!(
            "Original post by {}:\n\n{}",
            post.author, post.content
        );

        let body = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_message,
                },
            ],
            stream: false,
            format: None,
        };

        self.send_chat(body).await
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Send a chat request to the Ollama `/api/chat` endpoint and return the
    /// assistant message content as a `String`.
    async fn send_chat(&self, body: ChatRequest) -> Result<String> {
        let url = format!("{}/api/chat", self.ollama_url);

        let resp = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| {
                format!(
                    "Ollama is not running at {}. Please start Ollama first.",
                    self.ollama_url
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "Ollama returned HTTP {}: {}",
                status,
                text
            );
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .context("failed to parse Ollama chat response")?;

        Ok(chat_resp.message.content)
    }
}
