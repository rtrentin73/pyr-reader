// Gmail connector — fetches emails via Gmail REST API (OAuth2).
// Only imports emails matching configured from-address or subject-keyword filters (OR logic).
use super::{DataSource, Post};
use anyhow::{anyhow, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;
use uuid::Uuid;

const GMAIL_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GMAIL_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GMAIL_API_BASE: &str = "https://www.googleapis.com/gmail/v1";
pub const OAUTH_PORT: u16 = 8765;

// ---------------------------------------------------------------------------
// Filter configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailFilter {
    pub from_addresses: Vec<String>,
    pub subject_keywords: Vec<String>,
}

impl EmailFilter {
    pub fn is_empty(&self) -> bool {
        self.from_addresses.is_empty() && self.subject_keywords.is_empty()
    }

    /// Build a Gmail search query string from the filters (OR logic).
    /// Returns None when both lists are empty (nothing should be imported).
    pub fn build_query(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }

        let mut parts: Vec<String> = Vec::new();
        for addr in &self.from_addresses {
            parts.push(format!("from:{}", addr));
        }
        for kw in &self.subject_keywords {
            // Wrap multi-word keywords in quotes so Gmail treats them as phrases.
            if kw.contains(' ') {
                parts.push(format!("subject:\"{}\"", kw));
            } else {
                parts.push(format!("subject:{}", kw));
            }
        }

        Some(parts.join(" OR "))
    }
}

// ---------------------------------------------------------------------------
// Connector
// ---------------------------------------------------------------------------

pub struct GmailConnector {
    pub client_id: String,
    pub client_secret: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    /// Unix timestamp after which the access token is expired.
    pub token_expiry: Option<i64>,
    pub filters: EmailFilter,
    pub http_client: reqwest::Client,
}

impl GmailConnector {
    pub fn new(client_id: String, client_secret: String, filters: EmailFilter) -> Self {
        let http_client = reqwest::Client::builder()
            .user_agent("PyrReader/0.1")
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client_id,
            client_secret,
            access_token: None,
            refresh_token: None,
            token_expiry: None,
            filters,
            http_client,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.refresh_token.is_some()
            && !self.client_id.is_empty()
            && !self.client_secret.is_empty()
    }

    /// Build the Google OAuth2 authorization URL to open in the browser.
    pub fn build_auth_url(&self) -> String {
        let mut url = Url::parse(GMAIL_AUTH_URL).expect("Static URL is valid");
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("client_id", &self.client_id);
            q.append_pair(
                "redirect_uri",
                &format!("http://localhost:{}", OAUTH_PORT),
            );
            q.append_pair("response_type", "code");
            q.append_pair(
                "scope",
                "https://www.googleapis.com/auth/gmail.readonly",
            );
            q.append_pair("access_type", "offline");
            q.append_pair("prompt", "consent");
        }
        url.to_string()
    }

    /// Return a valid access token, refreshing it if necessary.
    async fn ensure_fresh_token(&mut self) -> Result<String> {
        let now = Utc::now().timestamp();

        if let (Some(token), Some(expiry)) = (&self.access_token, self.token_expiry) {
            if now < expiry - 60 {
                return Ok(token.clone());
            }
        }

        let refresh_token = self
            .refresh_token
            .as_ref()
            .ok_or_else(|| anyhow!("No refresh token. Please reconnect Gmail."))?
            .clone();

        let params = [
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];

        let resp = self
            .http_client
            .post(GMAIL_TOKEN_URL)
            .form(&params)
            .send()
            .await?
            .error_for_status()?;

        let body: Value = resp.json().await?;
        let access_token = body["access_token"]
            .as_str()
            .ok_or_else(|| anyhow!("No access_token in refresh response"))?
            .to_string();

        let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
        self.access_token = Some(access_token.clone());
        self.token_expiry = Some(now + expires_in);

        Ok(access_token)
    }

    /// List message IDs matching the Gmail query (max 50).
    async fn fetch_message_ids(&self, token: &str, query: &str) -> Result<Vec<String>> {
        let url = format!("{}/users/me/messages", GMAIL_API_BASE);
        let resp = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .query(&[("q", query), ("maxResults", "50")])
            .send()
            .await?
            .error_for_status()?;

        let body: Value = resp.json().await?;
        let ids = body["messages"]
            .as_array()
            .map(|msgs| {
                msgs.iter()
                    .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(ids)
    }

    /// Fetch a single message by Gmail ID and convert it to a Post.
    async fn fetch_message(&self, token: &str, gmail_id: &str) -> Result<Post> {
        let url = format!("{}/users/me/messages/{}", GMAIL_API_BASE, gmail_id);
        let resp = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .query(&[("format", "full")])
            .send()
            .await?
            .error_for_status()?;

        let msg: Value = resp.json().await?;
        parse_message_to_post(&msg)
    }

    /// Fetch all posts matching the configured filters.
    /// Uses `&mut self` because token refresh may update internal state.
    pub async fn fetch_posts(&mut self) -> Result<Vec<Post>> {
        if !self.is_connected() {
            return Err(anyhow!(
                "Gmail not connected. Configure credentials and authorize first."
            ));
        }

        let query = match self.filters.build_query() {
            Some(q) => q,
            None => return Ok(vec![]), // No filters configured → import nothing.
        };

        let token = self.ensure_fresh_token().await?;
        let ids = self.fetch_message_ids(&token, &query).await?;

        let mut posts = Vec::new();
        for id in &ids {
            match self.fetch_message(&token, id).await {
                Ok(post) => posts.push(post),
                Err(e) => log::error!("Failed to fetch Gmail message {}: {}", id, e),
            }
        }

        Ok(posts)
    }
}

// ---------------------------------------------------------------------------
// Message parsing helpers
// ---------------------------------------------------------------------------

fn parse_message_to_post(msg: &Value) -> Result<Post> {
    let headers = msg["payload"]["headers"]
        .as_array()
        .ok_or_else(|| anyhow!("Missing headers in Gmail message"))?;

    let get_header = |name: &str| -> String {
        headers
            .iter()
            .find(|h| {
                h["name"]
                    .as_str()
                    .map(|n| n.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
            })
            .and_then(|h| h["value"].as_str())
            .unwrap_or("")
            .to_string()
    };

    let from = get_header("From");
    let subject = get_header("Subject");
    let date = get_header("Date");
    let raw_message_id = get_header("Message-ID");

    // Strip angle brackets from Message-ID (e.g. "<abc@mail.gmail.com>" → "abc@mail.gmail.com").
    let message_id = raw_message_id.trim_matches(|c| c == '<' || c == '>').to_string();
    let id = if message_id.is_empty() {
        msg["id"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    } else {
        message_id
    };

    let body_text = super::normalize_content(&extract_body_text(&msg["payload"]));
    // Fall back to subject line when the email body is empty (e.g. image-only emails).
    let content = if body_text.trim().is_empty() {
        subject.clone()
    } else {
        body_text
    };
    let timestamp = parse_email_date(&date).unwrap_or_else(|| Utc::now().timestamp());

    Ok(Post {
        id,
        source: DataSource::Email,
        author: from.clone(),
        content,
        url: None,
        timestamp,
        raw_data: json!({
            "from": from,
            "subject": subject,
            "date": date,
            "gmail_id": msg["id"].as_str().unwrap_or(""),
        }),
    })
}

/// Recursively extract body from a Gmail message payload.
/// Prefers text/plain; falls back to text/html, then any text/* type.
fn extract_body_text(payload: &Value) -> String {
    if let Some(parts) = payload["parts"].as_array() {
        // 1) Prefer text/plain.
        for part in parts {
            if part["mimeType"].as_str() == Some("text/plain") {
                if let Some(text) = decode_body_data(&part["body"]["data"]) {
                    if !text.trim().is_empty() {
                        return text;
                    }
                }
            }
        }
        // 2) Recurse into nested multipart/* parts (e.g. multipart/alternative).
        for part in parts {
            let mime = part["mimeType"].as_str().unwrap_or("");
            if mime.starts_with("multipart/") {
                let nested = extract_body_text(part);
                if !nested.is_empty() {
                    return nested;
                }
            }
        }
        // 3) Fall back to text/html (tags will be stripped by normalize_content).
        for part in parts {
            if part["mimeType"].as_str() == Some("text/html") {
                if let Some(html) = decode_body_data(&part["body"]["data"]) {
                    if !html.trim().is_empty() {
                        return html;
                    }
                }
            }
        }
        // 4) Last resort: any text/* part we haven't tried yet.
        for part in parts {
            let mime = part["mimeType"].as_str().unwrap_or("");
            if mime.starts_with("text/") && mime != "text/plain" && mime != "text/html" {
                if let Some(text) = decode_body_data(&part["body"]["data"]) {
                    if !text.trim().is_empty() {
                        return text;
                    }
                }
            }
        }
    }

    // Single-part message (plain or html — normalize_content handles both).
    decode_body_data(&payload["body"]["data"]).unwrap_or_default()
}

fn decode_body_data(data: &Value) -> Option<String> {
    let encoded = data.as_str()?;
    if encoded.is_empty() {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(encoded).ok()?;
    String::from_utf8(bytes).ok()
}

fn parse_email_date(date_str: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc2822(date_str.trim())
        .ok()
        .map(|dt| dt.timestamp())
}

// ---------------------------------------------------------------------------
// OAuth2 callback server (standalone — called from a spawned task)
// ---------------------------------------------------------------------------

/// Start a one-shot local HTTP server on OAUTH_PORT, wait for the Google
/// redirect with the authorization code, exchange it for tokens, and return
/// (access_token, refresh_token, expires_in_seconds).
pub async fn wait_for_oauth_callback(
    client_id: String,
    client_secret: String,
    http_client: reqwest::Client,
) -> Result<(String, String, i64)> {
    let redirect_uri = format!("http://localhost:{}", OAUTH_PORT);
    let listener = TcpListener::bind(format!("127.0.0.1:{}", OAUTH_PORT))
        .await
        .map_err(|e| anyhow!("Could not bind to port {}: {}. Is another app using it?", OAUTH_PORT, e))?;

    // Accept first incoming connection (the browser redirect).
    let (mut stream, _) = listener.accept().await?;

    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the authorization code from "GET /?code=xxx HTTP/1.1".
    let code = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|path| Url::parse(&format!("http://localhost{}", path)).ok())
        .and_then(|url| {
            url.query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.into_owned())
        });

    let (status_line, body_text) = if code.is_some() {
        (
            "HTTP/1.1 200 OK",
            "Authorization successful! You can close this tab and return to Pyr Reader.",
        )
    } else {
        (
            "HTTP/1.1 400 Bad Request",
            "Authorization failed. Please try again from Pyr Reader.",
        )
    };
    let response = format!(
        "{}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_line,
        body_text.len(),
        body_text
    );
    stream.write_all(response.as_bytes()).await.ok();
    drop(stream);
    drop(listener);

    let code = code.ok_or_else(|| anyhow!("No authorization code in OAuth callback"))?;

    // Exchange the code for tokens.
    let params = [
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code", code.as_str()),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
    ];

    let resp = http_client
        .post(GMAIL_TOKEN_URL)
        .form(&params)
        .send()
        .await?
        .error_for_status()?;

    let body: Value = resp.json().await?;

    let access_token = body["access_token"]
        .as_str()
        .ok_or_else(|| anyhow!("No access_token in token response"))?
        .to_string();

    let refresh_token = body["refresh_token"]
        .as_str()
        .ok_or_else(|| anyhow!(
            "No refresh_token in token response. Make sure access_type=offline and prompt=consent."
        ))?
        .to_string();

    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);

    Ok((access_token, refresh_token, expires_in))
}
