// LinkedIn API connector - OAuth 2.0 Authorization Code flow (standard, no PKCE)
// Uses official LinkedIn API endpoints only

use super::{Connector, DataSource, Post};
use anyhow::{bail, Context, Result};
use log::error;
use url::Url;

const AUTH_ENDPOINT: &str = "https://www.linkedin.com/oauth/v2/authorization";
const TOKEN_ENDPOINT: &str = "https://www.linkedin.com/oauth/v2/accessToken";
const API_BASE: &str = "https://api.linkedin.com";
const REDIRECT_URI: &str = "http://localhost:8765/callback/linkedin";
const SCOPES: &str = "r_liteprofile r_organization_social w_member_social";
const LINKEDIN_VERSION: &str = "202401";

pub struct LinkedInConnector {
    client_id: String,
    client_secret: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    http_client: reqwest::Client,
    authenticated: bool,
    member_id: Option<String>,
    followed_profiles: Vec<String>,
    oauth_state: Option<String>,
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl LinkedInConnector {
    pub fn new(client_id: String, client_secret: String) -> Self {
        use reqwest::header::{HeaderMap, HeaderValue};

        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            "X-Restli-Protocol-Version",
            HeaderValue::from_static("2.0.0"),
        );
        default_headers.insert(
            "LinkedIn-Version",
            HeaderValue::from_static(LINKEDIN_VERSION),
        );

        let http_client = reqwest::Client::builder()
            .default_headers(default_headers)
            .build()
            .expect("Failed to build HTTP client for LinkedIn connector");

        Self {
            client_id,
            client_secret,
            access_token: None,
            refresh_token: None,
            http_client,
            authenticated: false,
            member_id: None,
            followed_profiles: Vec::new(),
            oauth_state: None,
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
    pub fn client_secret(&self) -> &str {
        &self.client_secret
    }
}

// ---------------------------------------------------------------------------
// Followed profiles management
// ---------------------------------------------------------------------------

impl LinkedInConnector {
    /// Add a LinkedIn profile URN to the followed list.
    pub fn add_followed_profile(&mut self, urn: String) {
        self.followed_profiles.push(urn);
    }

    /// Remove a LinkedIn profile URN from the followed list.
    pub fn remove_followed_profile(&mut self, urn: &str) {
        self.followed_profiles.retain(|u| u != urn);
    }

    /// Return a slice of all followed profile URNs.
    pub fn list_followed_profiles(&self) -> &[String] {
        &self.followed_profiles
    }
}

// ---------------------------------------------------------------------------
// OAuth 2.0 Authorization Code flow
// ---------------------------------------------------------------------------

impl LinkedInConnector {
    /// Generate the full OAuth 2.0 authorization URL that the user should be
    /// directed to in a browser. Stores a random state parameter internally
    /// for CSRF protection.
    pub fn get_auth_url(&mut self) -> Result<String> {
        let state = uuid::Uuid::new_v4().to_string();

        let mut auth_url = Url::parse(AUTH_ENDPOINT)
            .context("Failed to parse LinkedIn authorization endpoint URL")?;

        auth_url
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", REDIRECT_URI)
            .append_pair("scope", SCOPES)
            .append_pair("state", &state);

        self.oauth_state = Some(state);

        Ok(auth_url.to_string())
    }

    /// Exchange an authorization code (received at the redirect URI) for an
    /// access token and refresh token. On success the connector is marked as
    /// authenticated and the member ID is fetched via [`verify_credentials`].
    pub async fn exchange_code(&mut self, code: &str) -> Result<()> {
        let params = vec![
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
        ];

        let resp = self
            .http_client
            .post(TOKEN_ENDPOINT)
            .form(&params)
            .send()
            .await
            .context("Failed to send LinkedIn token exchange request")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse LinkedIn token exchange response body")?;

        if !status.is_success() {
            bail!(
                "LinkedIn token exchange failed (HTTP {}): {}",
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
            bail!(
                "LinkedIn token exchange response did not contain an access_token: {}",
                body
            );
        }

        // Fetch the authenticated member's ID.
        self.verify_credentials().await?;
        self.authenticated = true;

        Ok(())
    }

    /// Refresh an expired access token using the stored refresh token.
    pub async fn refresh_access_token(&mut self) -> Result<()> {
        let refresh = self
            .refresh_token
            .as_deref()
            .context("No LinkedIn refresh token available")?
            .to_string();

        let params = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", self.client_id.as_str()),
            ("client_secret", self.client_secret.as_str()),
        ];

        let resp = self
            .http_client
            .post(TOKEN_ENDPOINT)
            .form(&params)
            .send()
            .await
            .context("Failed to send LinkedIn token refresh request")?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse LinkedIn token refresh response body")?;

        if !status.is_success() {
            bail!(
                "LinkedIn token refresh failed (HTTP {}): {}",
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
            bail!(
                "LinkedIn token refresh response did not contain an access_token: {}",
                body
            );
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Public API methods
// ---------------------------------------------------------------------------

impl LinkedInConnector {
    /// Fetch posts for a specific LinkedIn entity (person or organization) by
    /// their URN.
    pub async fn fetch_posts_for_entity(&self, author_urn: &str) -> Result<Vec<Post>> {
        if !self.authenticated {
            bail!("Not authenticated with LinkedIn API");
        }

        let url = format!("{}/rest/posts", API_BASE);
        let urn_string = author_urn.to_string();
        let count_string = "20".to_string();
        let q_string = "author".to_string();

        let resp = self
            .authorized_get(
                &url,
                &[
                    ("author", &urn_string),
                    ("count", &count_string),
                    ("q", &q_string),
                ],
            )
            .await?;

        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse LinkedIn posts response body")?;

        let empty_vec = vec![];
        let elements = body["elements"].as_array().unwrap_or(&empty_vec);

        let posts: Vec<Post> = elements
            .iter()
            .map(|element| linkedin_post_to_post(element))
            .collect();

        Ok(posts)
    }

    /// Fetch posts from all followed profiles. Errors on individual profiles
    /// are logged but do not stop fetching from remaining profiles.
    pub async fn fetch_all_followed_posts(&self) -> Result<Vec<Post>> {
        let mut all_posts = Vec::new();

        for profile_urn in &self.followed_profiles {
            match self.fetch_posts_for_entity(profile_urn).await {
                Ok(posts) => {
                    all_posts.extend(posts);
                }
                Err(e) => {
                    error!(
                        "Failed to fetch LinkedIn posts for '{}': {}",
                        profile_urn, e
                    );
                }
            }
        }

        Ok(all_posts)
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

impl LinkedInConnector {
    /// Fetch the authenticated member's profile from `GET /v2/me` and store
    /// the member ID as a URN.
    async fn verify_credentials(&mut self) -> Result<()> {
        let token = self
            .access_token
            .as_deref()
            .context("No LinkedIn access token available")?;

        let url = format!("{}/v2/me", API_BASE);

        let resp = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to send LinkedIn verify credentials request")?;

        check_rate_limit(&resp)?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse LinkedIn /v2/me response")?;

        if !status.is_success() {
            bail!(
                "LinkedIn verify credentials failed (HTTP {}): {}",
                status,
                body
            );
        }

        let id = body["id"]
            .as_str()
            .context("Missing id in LinkedIn /v2/me response")?
            .to_string();

        self.member_id = Some(format!("urn:li:person:{}", id));

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
            .context("No LinkedIn access token available")?;

        let resp = self
            .http_client
            .get(url)
            .bearer_auth(token)
            .query(query)
            .send()
            .await
            .with_context(|| format!("LinkedIn GET request to {} failed", url))?;

        check_rate_limit(&resp)?;

        let status = resp.status();
        if !status.is_success() {
            let body: serde_json::Value = resp
                .json()
                .await
                .unwrap_or(serde_json::Value::Null);
            bail!("LinkedIn API request failed (HTTP {}): {}", status, body);
        }

        Ok(resp)
    }
}

// ---------------------------------------------------------------------------
// Rate-limit checking
// ---------------------------------------------------------------------------

/// Inspect the `X-RateLimit-Remaining` response header. If the remaining
/// count has reached zero, log a warning.
fn check_rate_limit(resp: &reqwest::Response) -> Result<()> {
    let remaining = resp
        .headers()
        .get("X-RateLimit-Remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    if let Some(0) = remaining {
        let reset = resp
            .headers()
            .get("X-RateLimit-Reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok());

        let reset_dt = reset
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "unknown".to_string());

        bail!(
            "LinkedIn API rate limit exceeded. Limit resets at {} (unix: {})",
            reset_dt,
            reset.unwrap_or(0)
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// LinkedIn post-to-Post conversion
// ---------------------------------------------------------------------------

/// Convert a single LinkedIn post JSON element into a [`Post`].
fn linkedin_post_to_post(element: &serde_json::Value) -> Post {
    let post_id = element["id"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let author = element["author"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    let content = element["commentary"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // LinkedIn createdAt is in milliseconds; convert to seconds.
    let timestamp = element["createdAt"]
        .as_i64()
        .map(|ms| ms / 1000)
        .unwrap_or(0);

    let url = Some(format!(
        "https://www.linkedin.com/feed/update/{}",
        post_id
    ));

    Post {
        id: post_id,
        source: DataSource::LinkedIn,
        author,
        content,
        url,
        timestamp,
        raw_data: element.clone(),
    }
}

// ---------------------------------------------------------------------------
// Connector trait implementation
// ---------------------------------------------------------------------------

impl Connector for LinkedInConnector {
    /// Fetch posts from all followed LinkedIn profiles.
    async fn fetch_posts(&self) -> Result<Vec<Post>> {
        self.fetch_all_followed_posts().await
    }

    /// The LinkedIn connector uses a browser-based OAuth 2.0 Authorization
    /// Code flow. Call [`get_auth_url`] to obtain the authorization URL,
    /// direct the user there, and then call [`exchange_code`] with the code
    /// received at the redirect URI.
    async fn authenticate(&mut self) -> Result<()> {
        bail!(
            "LinkedIn OAuth 2.0 requires a browser-based flow. \
             Use get_auth_url() to obtain the authorization URL, then \
             call exchange_code() with the authorization code received \
             at the callback."
        );
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated
    }
}
