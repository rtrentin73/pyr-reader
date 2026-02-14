// X (Twitter) API v2 connector - OAuth 2.0 PKCE flow
// Uses official X API v2 endpoints only (https://api.x.com/2/)

use super::{Connector, DataSource, Post};
use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use url::Url;

const AUTH_ENDPOINT: &str = "https://x.com/i/oauth2/authorize";
const TOKEN_ENDPOINT: &str = "https://api.x.com/2/oauth2/token";
const API_BASE: &str = "https://api.x.com/2";
const REDIRECT_URI: &str = "http://localhost:8765/callback";
const SCOPES: &str = "tweet.read users.read offline.access";

/// Standard tweet fields requested in all API calls.
const TWEET_FIELDS: &str = "created_at,author_id,text";
/// Expansions requested to resolve author usernames.
const EXPANSIONS: &str = "author_id";
/// User fields returned when expanding author_id.
const USER_FIELDS: &str = "username,name";

pub struct XTwitterConnector {
    client_id: String,
    client_secret: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    http_client: reqwest::Client,
    authenticated: bool,
    user_id: Option<String>,
    pkce_verifier: Option<String>,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl XTwitterConnector {
    pub fn new(client_id: String, client_secret: Option<String>) -> Self {
        Self {
            client_id,
            client_secret,
            access_token: None,
            refresh_token: None,
            http_client: reqwest::Client::new(),
            authenticated: false,
            user_id: None,
            pkce_verifier: None,
        }
    }

    /// Restore previously saved tokens without going through the OAuth flow.
    /// Verifies the access token by fetching the user profile. If the access
    /// token is expired but a refresh token is available, attempts a refresh.
    pub async fn restore_tokens(
        &mut self,
        access_token: String,
        refresh_token: Option<String>,
    ) -> Result<()> {
        self.access_token = Some(access_token);
        self.refresh_token = refresh_token;

        // Try to verify the token.
        match self.verify_credentials().await {
            Ok(()) => {
                self.authenticated = true;
                Ok(())
            }
            Err(_) if self.refresh_token.is_some() => {
                // Access token may be expired; try refreshing.
                self.refresh_access_token().await?;
                self.verify_credentials().await?;
                self.authenticated = true;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Return the current access token (for persistence).
    pub fn access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    /// Return the current refresh token (for persistence).
    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    /// Return the client ID.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Return the client secret.
    pub fn client_secret(&self) -> Option<&str> {
        self.client_secret.as_deref()
    }
}

// ---------------------------------------------------------------------------
// OAuth 2.0 PKCE helpers
// ---------------------------------------------------------------------------

/// Generate a cryptographically random code verifier (128 random bytes,
/// base64url-encoded without padding, then truncated to 128 characters to
/// stay within the 43-128 character spec range).
fn generate_code_verifier() -> String {
    let random_bytes: [u8; 128] = {
        let mut buf = [0u8; 128];
        // Use getrandom via uuid's v4 implementation pattern - we just need
        // random bytes. We re-use sha2's digest machinery to avoid pulling
        // another crate: hash a fresh uuid repeatedly until we fill the buffer.
        // This is only used once per auth flow so performance is irrelevant.
        let mut pos = 0usize;
        while pos < 128 {
            let id = uuid::Uuid::new_v4();
            let bytes = id.as_bytes();
            let copy_len = (128 - pos).min(bytes.len());
            buf[pos..pos + copy_len].copy_from_slice(&bytes[..copy_len]);
            pos += copy_len;
        }
        buf
    };
    let encoded = URL_SAFE_NO_PAD.encode(random_bytes);
    // Spec allows 43-128 characters; truncate to 128.
    encoded[..128].to_string()
}

/// Derive the PKCE code challenge from a code verifier using S256
/// (SHA-256 + base64url without padding).
fn derive_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

// ---------------------------------------------------------------------------
// Public API methods
// ---------------------------------------------------------------------------

impl XTwitterConnector {
    /// Generate the full OAuth 2.0 authorization URL that the user should be
    /// directed to in a browser. Stores the PKCE code verifier internally so
    /// it can be used later in [`exchange_code`].
    pub fn get_auth_url(&mut self) -> Result<String> {
        let verifier = generate_code_verifier();
        let challenge = derive_code_challenge(&verifier);

        let state = uuid::Uuid::new_v4().to_string();

        let mut auth_url = Url::parse(AUTH_ENDPOINT)
            .context("Failed to parse authorization endpoint URL")?;

        auth_url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", REDIRECT_URI)
            .append_pair("scope", SCOPES)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");

        self.pkce_verifier = Some(verifier);

        Ok(auth_url.to_string())
    }

    /// Exchange an authorization code (received at the redirect URI) for an
    /// access token and refresh token. On success the connector is marked as
    /// authenticated and the user ID is fetched via [`verify_credentials`].
    pub async fn exchange_code(&mut self, code: &str) -> Result<()> {
        let verifier = self
            .pkce_verifier
            .take()
            .context("No PKCE verifier stored - call get_auth_url first")?;

        let mut params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", &verifier),
            ("client_id", &self.client_id),
        ];

        // Confidential clients must also send client_secret.
        let secret_owned;
        if let Some(ref s) = self.client_secret {
            secret_owned = s.clone();
            params.push(("client_secret", &secret_owned));
        }

        let resp = self
            .http_client
            .post(TOKEN_ENDPOINT)
            .form(&params)
            .send()
            .await
            .context("Failed to send token exchange request")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse token exchange response body")?;

        if !status.is_success() {
            bail!(
                "Token exchange failed (HTTP {}): {}",
                status,
                body
            );
        }

        self.access_token = body["access_token"]
            .as_str()
            .map(|s| s.to_string());
        self.refresh_token = body["refresh_token"]
            .as_str()
            .map(|s| s.to_string());

        if self.access_token.is_none() {
            bail!("Token exchange response did not contain an access_token: {}", body);
        }

        // Fetch the authenticated user's ID.
        self.verify_credentials().await?;
        self.authenticated = true;

        Ok(())
    }

    /// Refresh an expired access token using the stored refresh token.
    pub async fn refresh_access_token(&mut self) -> Result<()> {
        let refresh = self
            .refresh_token
            .as_deref()
            .context("No refresh token available")?
            .to_string();

        let mut params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", self.client_id.as_str()),
        ];

        let secret_owned;
        if let Some(ref s) = self.client_secret {
            secret_owned = s.clone();
            params.push(("client_secret", &secret_owned));
        }

        let resp = self
            .http_client
            .post(TOKEN_ENDPOINT)
            .form(&params)
            .send()
            .await
            .context("Failed to send token refresh request")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse token refresh response body")?;

        if !status.is_success() {
            bail!(
                "Token refresh failed (HTTP {}): {}",
                status,
                body
            );
        }

        self.access_token = body["access_token"]
            .as_str()
            .map(|s| s.to_string());

        // The API may or may not rotate the refresh token.
        if let Some(new_refresh) = body["refresh_token"].as_str() {
            self.refresh_token = Some(new_refresh.to_string());
        }

        if self.access_token.is_none() {
            bail!("Token refresh response did not contain an access_token: {}", body);
        }

        Ok(())
    }

    /// Fetch the authenticated user's home timeline (reverse chronological).
    ///
    /// `max_results` defaults to 20 and is clamped to 1..=100.
    pub async fn fetch_timeline(&self, max_results: Option<u32>) -> Result<Vec<Post>> {
        if !self.authenticated {
            bail!("Not authenticated with X API");
        }

        let user_id = self
            .user_id
            .as_deref()
            .context("User ID not available - authenticate first")?;

        let max = max_results.unwrap_or(20).clamp(1, 100);
        let url = format!(
            "{}/users/{}/timelines/reverse_chronological",
            API_BASE, user_id
        );

        let resp = self
            .authorized_get(&url, &[
                ("max_results", &max.to_string()),
                ("tweet.fields", &TWEET_FIELDS.to_string()),
                ("expansions", &EXPANSIONS.to_string()),
                ("user.fields", &USER_FIELDS.to_string()),
            ])
            .await?;

        self.parse_tweets_response(resp).await
    }

    /// Search recent tweets matching the given query.
    ///
    /// `max_results` defaults to 20 and is clamped to 10..=100 (API minimum
    /// for search is 10).
    pub async fn search_tweets(
        &self,
        query: &str,
        max_results: Option<u32>,
    ) -> Result<Vec<Post>> {
        if !self.authenticated {
            bail!("Not authenticated with X API");
        }

        let max = max_results.unwrap_or(20).clamp(10, 100);
        let url = format!("{}/tweets/search/recent", API_BASE);

        let resp = self
            .authorized_get(&url, &[
                ("query", &query.to_string()),
                ("max_results", &max.to_string()),
                ("tweet.fields", &TWEET_FIELDS.to_string()),
                ("expansions", &EXPANSIONS.to_string()),
                ("user.fields", &USER_FIELDS.to_string()),
            ])
            .await?;

        self.parse_tweets_response(resp).await
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl XTwitterConnector {
    /// Fetch the authenticated user's profile from `GET /2/users/me` and store
    /// the user ID.
    async fn verify_credentials(&mut self) -> Result<()> {
        let token = self
            .access_token
            .as_deref()
            .context("No access token available")?;

        let url = format!("{}/users/me", API_BASE);

        let resp = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to send verify credentials request")?;

        check_rate_limit(&resp)?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse /users/me response")?;

        if !status.is_success() {
            bail!(
                "Verify credentials failed (HTTP {}): {}",
                status,
                body
            );
        }

        let id = body["data"]["id"]
            .as_str()
            .context("Missing data.id in /users/me response")?
            .to_string();

        self.user_id = Some(id);

        Ok(())
    }

    /// Perform an authorized GET request with query parameters, returning the
    /// raw `reqwest::Response`. Checks rate limits before returning.
    async fn authorized_get(
        &self,
        url: &str,
        query: &[(&str, &String)],
    ) -> Result<reqwest::Response> {
        let token = self
            .access_token
            .as_deref()
            .context("No access token available")?;

        let resp = self
            .http_client
            .get(url)
            .bearer_auth(token)
            .query(query)
            .send()
            .await
            .with_context(|| format!("GET request to {} failed", url))?;

        check_rate_limit(&resp)?;

        let status = resp.status();
        if !status.is_success() {
            let body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or(serde_json::Value::Null);
            bail!("X API request failed (HTTP {}): {}", status, body);
        }

        Ok(resp)
    }

    /// Parse a standard X API v2 tweets response (with `data` array and
    /// `includes.users` expansion) into a `Vec<Post>`.
    async fn parse_tweets_response(
        &self,
        resp: reqwest::Response,
    ) -> Result<Vec<Post>> {
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse tweets response body")?;

        let empty_vec = vec![];
        let empty_array = serde_json::Value::Array(vec![]);

        let tweets = body["data"]
            .as_array()
            .unwrap_or(&empty_vec);

        let users = body
            .get("includes")
            .and_then(|inc| inc.get("users"))
            .unwrap_or(&empty_array);

        let posts: Vec<Post> = tweets
            .iter()
            .map(|tweet| tweet_to_post(tweet, users))
            .collect();

        Ok(posts)
    }
}

// ---------------------------------------------------------------------------
// Rate-limit checking
// ---------------------------------------------------------------------------

/// Inspect the `x-rate-limit-remaining` and `x-rate-limit-reset` response
/// headers. If the remaining count has reached zero, return an error telling
/// the caller when the limit resets.
fn check_rate_limit(resp: &reqwest::Response) -> Result<()> {
    let remaining = resp
        .headers()
        .get("x-rate-limit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let reset = resp
        .headers()
        .get("x-rate-limit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());

    if let Some(0) = remaining {
        let reset_dt = reset
            .and_then(|ts| {
                chrono::DateTime::from_timestamp(ts, 0)
            })
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "unknown".to_string());

        bail!(
            "X API rate limit exceeded. Limit resets at {} (unix: {})",
            reset_dt,
            reset.unwrap_or(0)
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tweet-to-Post conversion
// ---------------------------------------------------------------------------

/// Convert a single X API v2 tweet JSON object into a [`Post`].
///
/// `users` is the `includes.users` array from the response, used to resolve
/// `author_id` to a human-readable `@username (Display Name)` string.
fn tweet_to_post(tweet_data: &serde_json::Value, users: &serde_json::Value) -> Post {
    let tweet_id = tweet_data["id"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let text = tweet_data["text"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let author_id = tweet_data["author_id"]
        .as_str()
        .unwrap_or("unknown");

    // Resolve author_id to username via the includes.users expansion.
    let author = users
        .as_array()
        .and_then(|arr| arr.iter().find(|u| u["id"].as_str() == Some(author_id)))
        .map(|u| {
            let username = u["username"].as_str().unwrap_or("unknown");
            let name = u["name"].as_str().unwrap_or("");
            if name.is_empty() {
                format!("@{}", username)
            } else {
                format!("@{} ({})", username, name)
            }
        })
        .unwrap_or_else(|| format!("user:{}", author_id));

    // Parse created_at (ISO 8601) into a unix timestamp.
    let timestamp = tweet_data["created_at"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);

    let url = Some(format!(
        "https://x.com/i/status/{}",
        tweet_id
    ));

    Post {
        id: tweet_id,
        source: DataSource::XTwitter,
        author,
        content: text,
        url,
        timestamp,
        raw_data: tweet_data.clone(),
    }
}

// ---------------------------------------------------------------------------
// Connector trait implementation
// ---------------------------------------------------------------------------

impl Connector for XTwitterConnector {
    /// Fetch posts from the authenticated user's home timeline (up to 50).
    async fn fetch_posts(&self) -> Result<Vec<Post>> {
        self.fetch_timeline(Some(50)).await
    }

    /// The X connector uses a two-step OAuth 2.0 PKCE flow that requires
    /// user interaction in a browser. Call [`get_auth_url`] to obtain the
    /// authorization URL, direct the user there, and then call
    /// [`exchange_code`] with the code received at the redirect URI.
    async fn authenticate(&mut self) -> Result<()> {
        bail!(
            "X/Twitter OAuth 2.0 requires a browser-based flow. \
             Use get_auth_url() to obtain the authorization URL, then \
             call exchange_code() with the authorization code received \
             at the callback."
        );
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }
}
