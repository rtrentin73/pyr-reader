// Content classification and summarization module
// Supports multiple LLM providers: Ollama (local), Anthropic (Claude), OpenAI.

use crate::connectors::Post;
use anyhow::{Context, Result};
use log;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enrichment {
    pub synthesis: String,
    pub search_queries: Vec<String>,
    pub sources: Vec<EnrichmentSource>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentSource {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub score: f64,
}

/// Supported LLM providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Ollama,
    Anthropic,
    #[serde(rename = "openai")]
    OpenAI,
}

impl fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LlmProvider::Ollama => write!(f, "ollama"),
            LlmProvider::Anthropic => write!(f, "anthropic"),
            LlmProvider::OpenAI => write!(f, "openai"),
        }
    }
}

impl FromStr for LlmProvider {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(LlmProvider::Ollama),
            "anthropic" => Ok(LlmProvider::Anthropic),
            "openai" => Ok(LlmProvider::OpenAI),
            other => anyhow::bail!("Unknown LLM provider: {}", other),
        }
    }
}

/// Safe snapshot of classifier configuration for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifierConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub ollama_url: String,
    pub has_anthropic_key: bool,
    pub has_openai_key: bool,
    pub has_tavily_key: bool,
}

// ---------------------------------------------------------------------------
// Ollama request / response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponseMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OllamaModelEntry {
    name: String,
}

// ---------------------------------------------------------------------------
// Anthropic request / response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    text: Option<String>,
}

// ---------------------------------------------------------------------------
// OpenAI request / response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<OpenAiResponseFormat>,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAiResponseFormat {
    r#type: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
}

// ---------------------------------------------------------------------------
// Tavily request / response types (private)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TavilySearchRequest {
    query: String,
    search_depth: String,
    max_results: u32,
    include_answer: bool,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResponse {
    results: Vec<TavilySearchResult>,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResult {
    title: String,
    url: String,
    content: String,
    score: f64,
}

// ---------------------------------------------------------------------------
// Classifier
// ---------------------------------------------------------------------------

pub struct Classifier {
    provider: LlmProvider,
    ollama_url: String,
    model: String,
    anthropic_api_key: Option<String>,
    openai_api_key: Option<String>,
    tavily_api_key: Option<String>,
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
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build reqwest client");

        Self {
            provider: LlmProvider::Ollama,
            ollama_url: ollama_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            model: model.unwrap_or_else(|| "llama3.2".to_string()),
            anthropic_api_key: None,
            openai_api_key: None,
            tavily_api_key: None,
            http_client,
        }
    }

    // -----------------------------------------------------------------------
    // Setter methods
    // -----------------------------------------------------------------------

    pub fn set_provider(&mut self, provider: LlmProvider) {
        self.provider = provider;
    }

    pub fn set_model(&mut self, model: String) {
        self.model = model;
    }

    pub fn set_ollama_url(&mut self, url: String) {
        self.ollama_url = url;
    }

    pub fn set_api_key(&mut self, provider: &LlmProvider, key: String) {
        match provider {
            LlmProvider::Anthropic => self.anthropic_api_key = Some(key),
            LlmProvider::OpenAI => self.openai_api_key = Some(key),
            LlmProvider::Ollama => {} // Ollama doesn't use API keys
        }
    }

    pub fn set_tavily_api_key(&mut self, key: String) {
        self.tavily_api_key = Some(key);
    }

    pub fn has_tavily_key(&self) -> bool {
        self.tavily_api_key.is_some()
    }

    pub fn get_config(&self) -> ClassifierConfig {
        ClassifierConfig {
            provider: self.provider.clone(),
            model: self.model.clone(),
            ollama_url: self.ollama_url.clone(),
            has_anthropic_key: self.anthropic_api_key.is_some(),
            has_openai_key: self.openai_api_key.is_some(),
            has_tavily_key: self.tavily_api_key.is_some(),
        }
    }

    // -----------------------------------------------------------------------
    // Availability / discovery
    // -----------------------------------------------------------------------

    /// Check whether the current provider is available.
    pub async fn is_available(&self) -> Result<bool> {
        match self.provider {
            LlmProvider::Ollama => {
                let url = format!("{}/api/tags", self.ollama_url);
                match self.http_client.get(&url).send().await {
                    Ok(resp) => Ok(resp.status().is_success()),
                    Err(_) => Ok(false),
                }
            }
            LlmProvider::Anthropic => Ok(self.anthropic_api_key.is_some()),
            LlmProvider::OpenAI => Ok(self.openai_api_key.is_some()),
        }
    }

    /// Return the list of model names available for the current provider.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        match self.provider {
            LlmProvider::Ollama => self.list_models_ollama().await,
            LlmProvider::Anthropic => Ok(Self::curated_anthropic_models()),
            LlmProvider::OpenAI => self.list_models_openai().await,
        }
    }

    async fn list_models_ollama(&self) -> Result<Vec<String>> {
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

        let tags: OllamaTagsResponse = resp
            .json()
            .await
            .context("failed to parse /api/tags response")?;
        let names = tags.models.into_iter().map(|m| m.name).collect();
        Ok(names)
    }

    fn curated_anthropic_models() -> Vec<String> {
        vec![
            "claude-sonnet-4-5-20250929".to_string(),
            "claude-haiku-4-5-20251001".to_string(),
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
        ]
    }

    async fn list_models_openai(&self) -> Result<Vec<String>> {
        let api_key = self
            .openai_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("OpenAI API key not set"))?;

        let resp = self
            .http_client
            .get("https://api.openai.com/v1/models")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .context("Failed to reach OpenAI API")?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI /v1/models returned error: {}", text);
        }

        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<ModelData>,
        }
        #[derive(Deserialize)]
        struct ModelData {
            id: String,
        }

        let body: ModelsResponse = resp.json().await.context("Failed to parse OpenAI models")?;
        let mut models: Vec<String> = body
            .data
            .into_iter()
            .map(|m| m.id)
            .filter(|id| id.starts_with("gpt-"))
            .collect();
        models.sort();
        models.reverse();
        Ok(models)
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

        let raw = self
            .send_chat(system_prompt, &user_message, true)
            .await?;

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

        self.send_chat(system_prompt, &post.content, false).await
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

        self.send_chat(system_prompt, &user_message, false).await
    }

    // -----------------------------------------------------------------------
    // Enrichment (Learn Mode)
    // -----------------------------------------------------------------------

    /// Generate 2-3 focused web search queries from a post's content.
    pub async fn generate_search_queries(&self, post: &Post) -> Result<Vec<String>> {
        let system_prompt = "You are a research assistant. Generate 2-3 focused web search queries \
            that would help someone deeply understand the topics discussed in the given post. \
            Return a JSON object with a single key \"queries\" containing an array of query strings. \
            Return ONLY valid JSON, no other text.";

        let user_message = format!(
            "Generate search queries for this post:\n\nAuthor: {}\nContent: {}",
            post.author,
            &post.content[..post.content.len().min(1500)]
        );

        let raw = self.send_chat(system_prompt, &user_message, true).await?;

        // Try to parse JSON response
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(queries) = value.get("queries") {
                if let Ok(qs) = serde_json::from_value::<Vec<String>>(queries.clone()) {
                    if !qs.is_empty() {
                        return Ok(qs);
                    }
                }
            }
        }

        // Fallback: use truncated post content as a single query
        let fallback = post.content.chars().take(200).collect::<String>();
        Ok(vec![fallback])
    }

    /// Search Tavily for a given query.
    async fn search_tavily(&self, query: &str) -> Result<Vec<TavilySearchResult>> {
        let api_key = self
            .tavily_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Tavily API key not set"))?;

        let body = TavilySearchRequest {
            query: query.to_string(),
            search_depth: "basic".to_string(),
            max_results: 5,
            include_answer: true,
        };

        let resp = self
            .http_client
            .post("https://api.tavily.com/search")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to reach Tavily API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Tavily returned HTTP {}: {}", status, text);
        }

        let search_resp: TavilySearchResponse = resp
            .json()
            .await
            .context("Failed to parse Tavily response")?;

        Ok(search_resp.results)
    }

    /// Full enrichment pipeline: generate queries, search, synthesize.
    pub async fn enrich_post(&self, post: &Post) -> Result<Enrichment> {
        // 1. Generate search queries via LLM
        let queries = self.generate_search_queries(post).await?;

        // 2. Run Tavily search for each query, collect results
        let mut all_results: Vec<TavilySearchResult> = Vec::new();
        for query in &queries {
            match self.search_tavily(query).await {
                Ok(results) => all_results.extend(results),
                Err(e) => {
                    log::warn!("Tavily search failed for query '{}': {}", query, e);
                }
            }
        }

        // Dedupe by URL, sort by score descending, take top 8
        let mut seen_urls = std::collections::HashSet::new();
        all_results.retain(|r| seen_urls.insert(r.url.clone()));
        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(8);

        // 3. Build sources text with numbered references
        let sources: Vec<EnrichmentSource> = all_results
            .iter()
            .map(|r| EnrichmentSource {
                title: r.title.clone(),
                url: r.url.clone(),
                snippet: r.content.chars().take(300).collect(),
                score: r.score,
            })
            .collect();

        let sources_text = sources
            .iter()
            .enumerate()
            .map(|(i, s)| format!("[{}] {} - {}\n{}", i + 1, s.title, s.url, s.snippet))
            .collect::<Vec<_>>()
            .join("\n\n");

        // 4. Synthesize via LLM
        let system_prompt = "You are a research synthesizer. Given an original post and web search results, \
            write a 3-5 paragraph informative synthesis that provides deeper context, highlights key facts, \
            notes contrasting viewpoints if any, and references sources by number [1], [2], etc. \
            Be informative and educational. Write in clear, accessible prose.";

        let user_message = format!(
            "Original post by {}:\n{}\n\n---\nSearch results:\n{}",
            post.author, post.content, sources_text
        );

        let synthesis = self.send_chat(system_prompt, &user_message, false).await?;

        Ok(Enrichment {
            synthesis,
            search_queries: queries,
            sources,
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    // -----------------------------------------------------------------------
    // Internal: provider-dispatched send_chat
    // -----------------------------------------------------------------------

    async fn send_chat(
        &self,
        system_prompt: &str,
        user_message: &str,
        json_mode: bool,
    ) -> Result<String> {
        match self.provider {
            LlmProvider::Ollama => {
                self.send_chat_ollama(system_prompt, user_message, json_mode)
                    .await
            }
            LlmProvider::Anthropic => {
                self.send_chat_anthropic(system_prompt, user_message)
                    .await
            }
            LlmProvider::OpenAI => {
                self.send_chat_openai(system_prompt, user_message, json_mode)
                    .await
            }
        }
    }

    async fn send_chat_ollama(
        &self,
        system_prompt: &str,
        user_message: &str,
        json_mode: bool,
    ) -> Result<String> {
        let url = format!("{}/api/chat", self.ollama_url);

        let body = OllamaChatRequest {
            model: self.model.clone(),
            messages: vec![
                OllamaChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                OllamaChatMessage {
                    role: "user".to_string(),
                    content: user_message.to_string(),
                },
            ],
            stream: false,
            format: if json_mode {
                Some("json".to_string())
            } else {
                None
            },
        };

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
            anyhow::bail!("Ollama returned HTTP {}: {}", status, text);
        }

        let chat_resp: OllamaChatResponse = resp
            .json()
            .await
            .context("failed to parse Ollama chat response")?;

        Ok(chat_resp.message.content)
    }

    async fn send_chat_anthropic(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String> {
        let api_key = self
            .anthropic_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Anthropic API key not set"))?;

        let body = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            system: system_prompt.to_string(),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: user_message.to_string(),
            }],
        };

        let resp = self
            .http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to reach Anthropic API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic returned HTTP {}: {}", status, text);
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .context("failed to parse Anthropic response")?;

        let text = api_resp
            .content
            .into_iter()
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("");

        Ok(text)
    }

    async fn send_chat_openai(
        &self,
        system_prompt: &str,
        user_message: &str,
        json_mode: bool,
    ) -> Result<String> {
        let api_key = self
            .openai_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("OpenAI API key not set"))?;

        let body = OpenAiRequest {
            model: self.model.clone(),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: user_message.to_string(),
                },
            ],
            response_format: if json_mode {
                Some(OpenAiResponseFormat {
                    r#type: "json_object".to_string(),
                })
            } else {
                None
            },
        };

        let resp = self
            .http_client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to reach OpenAI API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI returned HTTP {}: {}", status, text);
        }

        let api_resp: OpenAiResponse = resp
            .json()
            .await
            .context("failed to parse OpenAI response")?;

        let text = api_resp
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .unwrap_or_default();

        Ok(text)
    }
}
