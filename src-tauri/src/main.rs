// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod classifier;
mod connectors;
mod storage;

use classifier::{Classification, Classifier};
use connectors::rss::RssConnector;
use connectors::linkedin::LinkedInConnector;
use connectors::x_twitter::XTwitterConnector;
use connectors::{Connector, Post};
use storage::{Board, Card, SecretStore, StorageManager};

use std::fs;
use tauri::{Manager, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub storage: std::sync::Mutex<StorageManager>,
    pub rss: tokio::sync::Mutex<RssConnector>,
    pub x_twitter: tokio::sync::Mutex<Option<XTwitterConnector>>,
    pub linkedin: tokio::sync::Mutex<Option<LinkedInConnector>>,
    pub classifier: Classifier,
}

// ===========================================================================
// Board commands
// ===========================================================================

#[tauri::command]
fn create_board(
    name: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Board, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .create_board(&name, description.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_boards(state: State<'_, AppState>) -> Result<Vec<Board>, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.get_boards().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_board(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.delete_board(&id).map_err(|e| e.to_string())
}

// ===========================================================================
// Card commands
// ===========================================================================

#[tauri::command]
fn create_card(
    board_id: String,
    post_id: String,
    summary: Option<String>,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Card, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .create_card(&board_id, &post_id, summary.as_deref(), &tags)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_cards_by_board(
    board_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<Card>, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .get_cards_by_board(&board_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_card(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.delete_card(&id).map_err(|e| e.to_string())
}

// ===========================================================================
// Post commands
// ===========================================================================

#[tauri::command]
fn get_posts(
    limit: Option<i64>,
    offset: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<Post>, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);
    storage.get_posts(limit, offset).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_post_by_id(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<Post>, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.get_post_by_id(&id).map_err(|e| e.to_string())
}

// ===========================================================================
// RSS commands
// ===========================================================================

#[tauri::command]
async fn add_rss_feed(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut rss = state.rss.lock().await;
    rss.add_feed(url);
    // Persist the updated feed list.
    let feeds_json = serde_json::to_string(rss.list_feeds()).map_err(|e| e.to_string())?;
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.save_setting("rss_feeds", &feeds_json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn remove_rss_feed(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut rss = state.rss.lock().await;
    rss.remove_feed(&url);
    // Persist the updated feed list.
    let feeds_json = serde_json::to_string(rss.list_feeds()).map_err(|e| e.to_string())?;
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.save_setting("rss_feeds", &feeds_json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn list_rss_feeds(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let rss = state.rss.lock().await;
    Ok(rss.list_feeds().to_vec())
}

#[tauri::command]
async fn fetch_rss_posts(state: State<'_, AppState>) -> Result<Vec<Post>, String> {
    use connectors::Connector;

    // Fetch posts from all configured RSS feeds.
    let posts = {
        let rss = state.rss.lock().await;
        rss.fetch_posts().await.map_err(|e| e.to_string())?
    };

    // Save each post to storage.
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        for post in &posts {
            storage.save_post(post).map_err(|e| e.to_string())?;
        }
    }

    Ok(posts)
}

// ===========================================================================
// X/Twitter commands
// ===========================================================================

#[tauri::command]
async fn x_setup(
    client_id: String,
    client_secret: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Persist X credentials (client_id in SQLite, secret in Keychain).
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage.save_setting("x_client_id", &client_id).map_err(|e| e.to_string())?;
    }
    if let Some(ref secret) = client_secret {
        SecretStore::set("x_client_secret", secret).map_err(|e| e.to_string())?;
    }
    let connector = XTwitterConnector::new(client_id, client_secret);
    let mut x = state.x_twitter.lock().await;
    *x = Some(connector);
    Ok(())
}

#[tauri::command]
async fn x_get_auth_url(state: State<'_, AppState>) -> Result<String, String> {
    let mut x = state.x_twitter.lock().await;
    let connector = x
        .as_mut()
        .ok_or_else(|| "X/Twitter connector not initialized. Call x_setup first.".to_string())?;
    connector.get_auth_url().map_err(|e| e.to_string())
}

#[tauri::command]
async fn x_exchange_code(code: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut x = state.x_twitter.lock().await;
    let connector = x
        .as_mut()
        .ok_or_else(|| "X/Twitter connector not initialized. Call x_setup first.".to_string())?;
    connector
        .exchange_code(&code)
        .await
        .map_err(|e| e.to_string())?;

    // Persist tokens in Keychain after successful exchange.
    if let Some(token) = connector.access_token() {
        SecretStore::set("x_access_token", token).map_err(|e| e.to_string())?;
    }
    if let Some(token) = connector.refresh_token() {
        SecretStore::set("x_refresh_token", token).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
async fn x_is_authenticated(state: State<'_, AppState>) -> Result<bool, String> {
    let x = state.x_twitter.lock().await;
    match x.as_ref() {
        Some(connector) => Ok(connector.is_authenticated()),
        None => Ok(false),
    }
}

#[tauri::command]
async fn x_fetch_timeline(state: State<'_, AppState>) -> Result<Vec<Post>, String> {
    // Fetch timeline posts from the X connector.
    let posts = {
        let x = state.x_twitter.lock().await;
        let connector = x.as_ref().ok_or_else(|| {
            "X/Twitter connector not initialized. Call x_setup first.".to_string()
        })?;
        connector
            .fetch_timeline(None)
            .await
            .map_err(|e| e.to_string())?
    };

    // Save each post to storage.
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        for post in &posts {
            storage.save_post(post).map_err(|e| e.to_string())?;
        }
    }

    Ok(posts)
}

// ===========================================================================
// LinkedIn commands
// ===========================================================================

#[tauri::command]
async fn linkedin_setup(
    client_id: String,
    client_secret: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Persist LinkedIn credentials (client_id in SQLite, secret in Keychain).
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage.save_setting("linkedin_client_id", &client_id).map_err(|e| e.to_string())?;
    }
    SecretStore::set("linkedin_client_secret", &client_secret).map_err(|e| e.to_string())?;
    let connector = LinkedInConnector::new(client_id, client_secret);
    let mut li = state.linkedin.lock().await;
    *li = Some(connector);
    Ok(())
}

#[tauri::command]
async fn linkedin_get_auth_url(state: State<'_, AppState>) -> Result<String, String> {
    let mut li = state.linkedin.lock().await;
    let connector = li
        .as_mut()
        .ok_or_else(|| "LinkedIn connector not initialized. Call linkedin_setup first.".to_string())?;
    connector.get_auth_url().map_err(|e| e.to_string())
}

#[tauri::command]
async fn linkedin_exchange_code(code: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut li = state.linkedin.lock().await;
    let connector = li
        .as_mut()
        .ok_or_else(|| "LinkedIn connector not initialized. Call linkedin_setup first.".to_string())?;
    connector
        .exchange_code(&code)
        .await
        .map_err(|e| e.to_string())?;

    // Persist tokens in Keychain after successful exchange.
    if let Some(token) = connector.access_token() {
        SecretStore::set("linkedin_access_token", token).map_err(|e| e.to_string())?;
    }
    if let Some(token) = connector.refresh_token() {
        SecretStore::set("linkedin_refresh_token", token).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
async fn linkedin_is_authenticated(state: State<'_, AppState>) -> Result<bool, String> {
    let li = state.linkedin.lock().await;
    match li.as_ref() {
        Some(connector) => Ok(connector.is_authenticated()),
        None => Ok(false),
    }
}

#[tauri::command]
async fn linkedin_add_profile(urn: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut li = state.linkedin.lock().await;
    let connector = li
        .as_mut()
        .ok_or_else(|| "LinkedIn connector not initialized. Call linkedin_setup first.".to_string())?;
    connector.add_followed_profile(urn);

    // Persist the updated followed profiles list.
    let profiles_json = serde_json::to_string(connector.list_followed_profiles())
        .map_err(|e| e.to_string())?;
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.save_setting("linkedin_followed_profiles", &profiles_json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn linkedin_remove_profile(urn: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut li = state.linkedin.lock().await;
    let connector = li
        .as_mut()
        .ok_or_else(|| "LinkedIn connector not initialized. Call linkedin_setup first.".to_string())?;
    connector.remove_followed_profile(&urn);

    // Persist the updated followed profiles list.
    let profiles_json = serde_json::to_string(connector.list_followed_profiles())
        .map_err(|e| e.to_string())?;
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.save_setting("linkedin_followed_profiles", &profiles_json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn linkedin_list_profiles(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let li = state.linkedin.lock().await;
    let connector = li
        .as_ref()
        .ok_or_else(|| "LinkedIn connector not initialized. Call linkedin_setup first.".to_string())?;
    Ok(connector.list_followed_profiles().to_vec())
}

#[tauri::command]
async fn linkedin_fetch_posts(state: State<'_, AppState>) -> Result<Vec<Post>, String> {
    use connectors::Connector;

    // Fetch posts from all followed LinkedIn profiles.
    let posts = {
        let li = state.linkedin.lock().await;
        let connector = li.as_ref().ok_or_else(|| {
            "LinkedIn connector not initialized. Call linkedin_setup first.".to_string()
        })?;
        connector
            .fetch_posts()
            .await
            .map_err(|e| e.to_string())?
    };

    // Save each post to storage.
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        for post in &posts {
            storage.save_post(post).map_err(|e| e.to_string())?;
        }
    }

    Ok(posts)
}

// ===========================================================================
// OAuth callback helper
// ===========================================================================

/// Start a temporary HTTP server on localhost:8765, wait for the OAuth provider
/// to redirect back, parse the `code` query parameter, send a "success" page,
/// and return the authorization code. Times out after 2 minutes.
async fn wait_for_oauth_callback(
    listener: &tokio::net::TcpListener,
    expected_path: &str,
) -> Result<String, String> {
    let timeout = tokio::time::Duration::from_secs(120);

    let (mut stream, _addr) = tokio::time::timeout(timeout, listener.accept())
        .await
        .map_err(|_| "OAuth timed out — no callback received within 2 minutes".to_string())?
        .map_err(|e| format!("Failed to accept connection: {}", e))?;

    // Read the HTTP request (up to 8 KB is plenty for a redirect callback).
    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Failed to read request: {}", e))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the request line, e.g. "GET /callback/linkedin?code=abc&state=xyz HTTP/1.1"
    let request_line = request.lines().next().unwrap_or("");
    let path_and_query = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "Malformed HTTP request from OAuth callback".to_string())?;

    // Verify the path matches what we expect.
    if !path_and_query.starts_with(expected_path) {
        let body = "Unexpected callback path.";
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes()).await;
        return Err(format!(
            "Unexpected callback path: expected {} but got {}",
            expected_path, path_and_query
        ));
    }

    // Extract the `code` query parameter.
    let url = url::Url::parse(&format!("http://localhost{}", path_and_query))
        .map_err(|e| format!("Failed to parse callback URL: {}", e))?;
    let code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .ok_or_else(|| {
            // Check if there's an error parameter instead.
            let error = url
                .query_pairs()
                .find(|(k, _)| k == "error")
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            format!("OAuth authorization failed: {}", error)
        })?;

    // Send a friendly "you can close this tab" response.
    let html_body = r#"<!DOCTYPE html>
<html><head><title>Authorization Complete</title>
<style>body{font-family:-apple-system,system-ui,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#f5f5f7;color:#1d1d1f}
.box{text-align:center;padding:40px;background:#fff;border-radius:12px;box-shadow:0 2px 8px rgba(0,0,0,0.1)}
h1{font-size:24px;margin-bottom:8px}p{color:#86868b}</style></head>
<body><div class="box"><h1>Authorization Successful</h1><p>You can close this tab and return to pyr-reader.</p></div></body></html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html_body.len(),
        html_body
    );
    let _ = stream.write_all(response.as_bytes()).await;

    Ok(code)
}

// ===========================================================================
// One-click OAuth commands
// ===========================================================================

#[tauri::command]
async fn linkedin_start_oauth(state: State<'_, AppState>) -> Result<(), String> {
    // 1. Get auth URL.
    let auth_url = {
        let mut li = state.linkedin.lock().await;
        let connector = li
            .as_mut()
            .ok_or("LinkedIn connector not initialized. Save credentials first.")?;
        connector.get_auth_url().map_err(|e| e.to_string())?
    };

    // 2. Bind local callback server.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8765")
        .await
        .map_err(|e| format!("Failed to start callback server: {}", e))?;

    // 3. Open browser.
    open::that(&auth_url).map_err(|e| format!("Failed to open browser: {}", e))?;

    // 4. Wait for callback.
    let code = wait_for_oauth_callback(&listener, "/callback/linkedin").await?;

    // 5. Exchange code and persist tokens.
    {
        let mut li = state.linkedin.lock().await;
        let connector = li
            .as_mut()
            .ok_or("LinkedIn connector not initialized.")?;
        connector
            .exchange_code(&code)
            .await
            .map_err(|e| e.to_string())?;

        if let Some(token) = connector.access_token() {
            SecretStore::set("linkedin_access_token", token).map_err(|e| e.to_string())?;
        }
        if let Some(token) = connector.refresh_token() {
            SecretStore::set("linkedin_refresh_token", token).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[tauri::command]
async fn x_start_oauth(state: State<'_, AppState>) -> Result<(), String> {
    // 1. Get auth URL.
    let auth_url = {
        let mut x = state.x_twitter.lock().await;
        let connector = x
            .as_mut()
            .ok_or("X/Twitter connector not initialized. Call x_setup first.")?;
        connector.get_auth_url().map_err(|e| e.to_string())?
    };

    // 2. Bind local callback server.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8765")
        .await
        .map_err(|e| format!("Failed to start callback server: {}", e))?;

    // 3. Open browser.
    open::that(&auth_url).map_err(|e| format!("Failed to open browser: {}", e))?;

    // 4. Wait for callback.
    let code = wait_for_oauth_callback(&listener, "/callback").await?;

    // 5. Exchange code and persist tokens.
    {
        let mut x = state.x_twitter.lock().await;
        let connector = x
            .as_mut()
            .ok_or("X/Twitter connector not initialized.")?;
        connector
            .exchange_code(&code)
            .await
            .map_err(|e| e.to_string())?;

        if let Some(token) = connector.access_token() {
            SecretStore::set("x_access_token", token).map_err(|e| e.to_string())?;
        }
        if let Some(token) = connector.refresh_token() {
            SecretStore::set("x_refresh_token", token).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// ===========================================================================
// Classifier commands
// ===========================================================================

#[tauri::command]
async fn classifier_is_available(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .classifier
        .is_available()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn classifier_list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state
        .classifier
        .list_models()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn classify_post(
    post_id: String,
    state: State<'_, AppState>,
) -> Result<Classification, String> {
    // Retrieve the post from storage.
    let post = {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage
            .get_post_by_id(&post_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Post not found: {}", post_id))?
    };

    // Classify using the LLM.
    let classification = state
        .classifier
        .classify_post(&post)
        .await
        .map_err(|e| e.to_string())?;

    // Save the classification to storage.
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage
            .save_classification(&post_id, &classification)
            .map_err(|e| e.to_string())?;
    }

    Ok(classification)
}

#[tauri::command]
async fn summarize_post(
    post_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Retrieve the post from storage.
    let post = {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage
            .get_post_by_id(&post_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Post not found: {}", post_id))?
    };

    state
        .classifier
        .summarize_post(&post)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn generate_derivative(
    post_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Retrieve the post from storage.
    let post = {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage
            .get_post_by_id(&post_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Post not found: {}", post_id))?
    };

    state
        .classifier
        .generate_derivative(&post)
        .await
        .map_err(|e| e.to_string())
}

// ===========================================================================
// Settings commands
// ===========================================================================

#[tauri::command]
fn save_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if SecretStore::is_secret_key(&key) {
        return Err(format!("'{}' is a secret and cannot be stored via generic settings", key));
    }
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .save_setting(&key, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_setting(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    if SecretStore::is_secret_key(&key) {
        return Ok(None);
    }
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.get_setting(&key).map_err(|e| e.to_string())
}

// ===========================================================================
// Application entry point
// ===========================================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Resolve the app data directory for the SQLite database.
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data directory");

            // Ensure the directory exists.
            fs::create_dir_all(&app_data_dir)
                .expect("Failed to create app data directory");

            let db_path = app_data_dir.join("pyr_reader.db");
            let db_path_str = db_path
                .to_str()
                .expect("App data directory path is not valid UTF-8");

            // Initialize the storage manager.
            let storage = StorageManager::new(db_path_str)
                .expect("Failed to initialize StorageManager");

            // Migrate any secrets from SQLite to Keychain (idempotent).
            SecretStore::migrate_from_sqlite(&storage);

            // Restore RSS feeds from settings.
            let rss_feeds: Vec<String> = storage
                .get_setting("rss_feeds")
                .ok()
                .flatten()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();
            let rss = RssConnector::new(rss_feeds);

            // Restore X/Twitter connector from saved credentials.
            // client_id stays in SQLite; secrets come from Keychain.
            let saved_x_client_id = storage.get_setting("x_client_id").ok().flatten();
            let saved_x_client_secret = SecretStore::get("x_client_secret").ok().flatten();
            let saved_x_access_token = SecretStore::get("x_access_token").ok().flatten();
            let saved_x_refresh_token = SecretStore::get("x_refresh_token").ok().flatten();

            let x_twitter: Option<XTwitterConnector> = saved_x_client_id.map(|client_id| {
                XTwitterConnector::new(client_id, saved_x_client_secret)
            });

            // Restore LinkedIn connector from saved credentials.
            // client_id stays in SQLite; secrets come from Keychain.
            let saved_li_client_id = storage.get_setting("linkedin_client_id").ok().flatten();
            let saved_li_client_secret = SecretStore::get("linkedin_client_secret").ok().flatten();
            let saved_li_access_token = SecretStore::get("linkedin_access_token").ok().flatten();
            let saved_li_refresh_token = SecretStore::get("linkedin_refresh_token").ok().flatten();
            let saved_li_profiles: Vec<String> = storage
                .get_setting("linkedin_followed_profiles")
                .ok()
                .flatten()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();

            let linkedin: Option<LinkedInConnector> = match (saved_li_client_id, saved_li_client_secret) {
                (Some(client_id), Some(client_secret)) => {
                    let mut connector = LinkedInConnector::new(client_id, client_secret);
                    for urn in &saved_li_profiles {
                        connector.add_followed_profile(urn.clone());
                    }
                    Some(connector)
                }
                _ => None,
            };

            // Initialize the classifier with default Ollama URL and model.
            let classifier = Classifier::new(None, None);

            // Build and manage application state.
            let app_state = AppState {
                storage: std::sync::Mutex::new(storage),
                rss: tokio::sync::Mutex::new(rss),
                x_twitter: tokio::sync::Mutex::new(x_twitter),
                linkedin: tokio::sync::Mutex::new(linkedin),
                classifier,
            };

            app.manage(app_state);

            // Restore X/Twitter tokens asynchronously (best-effort).
            if let Some(access_token) = saved_x_access_token {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let mut x = state.x_twitter.lock().await;
                    if let Some(connector) = x.as_mut() {
                        if let Err(e) = connector
                            .restore_tokens(access_token, saved_x_refresh_token)
                            .await
                        {
                            log::warn!("Failed to restore X/Twitter session: {}", e);
                        } else {
                            // Persist any rotated tokens back to Keychain.
                            if let Some(token) = connector.access_token() {
                                let _ = SecretStore::set("x_access_token", token);
                            }
                            if let Some(token) = connector.refresh_token() {
                                let _ = SecretStore::set("x_refresh_token", token);
                            }
                        }
                    }
                });
            }

            // Restore LinkedIn tokens asynchronously (best-effort).
            if let Some(access_token) = saved_li_access_token {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<AppState>();
                    let mut li = state.linkedin.lock().await;
                    if let Some(connector) = li.as_mut() {
                        if let Err(e) = connector
                            .restore_tokens(access_token, saved_li_refresh_token)
                            .await
                        {
                            log::warn!("Failed to restore LinkedIn session: {}", e);
                        } else {
                            // Persist any rotated tokens back to Keychain.
                            if let Some(token) = connector.access_token() {
                                let _ = SecretStore::set("linkedin_access_token", token);
                            }
                            if let Some(token) = connector.refresh_token() {
                                let _ = SecretStore::set("linkedin_refresh_token", token);
                            }
                        }
                    }
                });
            }

            // Open devtools in debug builds.
            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Board commands
            create_board,
            get_boards,
            delete_board,
            // Card commands
            create_card,
            get_cards_by_board,
            delete_card,
            // Post commands
            get_posts,
            get_post_by_id,
            // RSS commands
            add_rss_feed,
            remove_rss_feed,
            list_rss_feeds,
            fetch_rss_posts,
            // X/Twitter commands
            x_setup,
            x_get_auth_url,
            x_exchange_code,
            x_is_authenticated,
            x_fetch_timeline,
            x_start_oauth,
            // LinkedIn commands
            linkedin_setup,
            linkedin_get_auth_url,
            linkedin_exchange_code,
            linkedin_is_authenticated,
            linkedin_start_oauth,
            linkedin_add_profile,
            linkedin_remove_profile,
            linkedin_list_profiles,
            linkedin_fetch_posts,
            // Classifier commands
            classifier_is_available,
            classifier_list_models,
            classify_post,
            summarize_post,
            generate_derivative,
            // Settings commands
            save_setting,
            get_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
