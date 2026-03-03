// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod classifier;
mod connectors;
mod storage;

use classifier::{Classification, ClassifierConfig, Classifier, Enrichment, LlmProvider};
use connectors::gmail::{wait_for_oauth_callback, EmailFilter, GmailConnector};
use connectors::rss::RssConnector;
use connectors::Post;
use serde::{Deserialize, Serialize};
use storage::{Board, Card, InterestProfile, SecretStore, StorageManager};

use std::collections::HashMap;
use std::str::FromStr;

use std::fs;
use tauri::{Emitter, Manager, State};

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub storage: std::sync::Mutex<StorageManager>,
    pub rss: tokio::sync::Mutex<RssConnector>,
    pub gmail: tokio::sync::Mutex<GmailConnector>,
    pub classifier: tokio::sync::Mutex<Classifier>,
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

#[tauri::command]
fn get_board_card_counts(state: State<'_, AppState>) -> Result<HashMap<String, i64>, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.get_board_card_counts().map_err(|e| e.to_string())
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
    saved: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Card, String> {
    let saved = saved.unwrap_or(true);
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .create_card(&board_id, &post_id, summary.as_deref(), &tags, saved)
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

#[tauri::command]
fn toggle_card_saved(id: String, saved: bool, state: State<'_, AppState>) -> Result<(), String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.set_card_saved(&id, saved).map_err(|e| e.to_string())
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
// Classifier commands
// ===========================================================================

#[tauri::command]
async fn classifier_is_available(state: State<'_, AppState>) -> Result<bool, String> {
    let classifier = state.classifier.lock().await;
    classifier
        .is_available()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn classifier_list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let classifier = state.classifier.lock().await;
    classifier
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
    let classification = {
        let classifier = state.classifier.lock().await;
        classifier
            .classify_post(&post)
            .await
            .map_err(|e| e.to_string())?
    };

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

    let classifier = state.classifier.lock().await;
    classifier
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

    let classifier = state.classifier.lock().await;
    classifier
        .generate_derivative(&post)
        .await
        .map_err(|e| e.to_string())
}

// ===========================================================================
// Auto-organize command
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutoOrganizeFailure {
    post_id: String,
    error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AutoOrganizeResult {
    total: usize,
    organized: usize,
    failed: Vec<AutoOrganizeFailure>,
    boards_created: Vec<String>,
}

#[tauri::command]
async fn auto_organize_posts(
    post_ids: Vec<String>,
    excluded_categories: Vec<String>,
    state: State<'_, AppState>,
) -> Result<AutoOrganizeResult, String> {
    let excluded_lower: Vec<String> = excluded_categories.iter().map(|c| c.to_lowercase()).collect();
    let mut organized: usize = 0;
    let mut failed: Vec<AutoOrganizeFailure> = Vec::new();
    let mut boards_created: Vec<String> = Vec::new();
    let total = post_ids.len();

    // Clear all ephemeral (unsaved) cards before re-organizing.
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        let deleted = storage.delete_unsaved_cards().map_err(|e| e.to_string())?;
        if deleted > 0 {
            log::info!("Cleared {} unsaved cards before auto-organize", deleted);
        }
    }

    for post_id in &post_ids {
        // 1. Retrieve the post from storage.
        let post = {
            let storage = state.storage.lock().map_err(|e| e.to_string())?;
            match storage.get_post_by_id(post_id).map_err(|e| e.to_string())? {
                Some(p) => p,
                None => {
                    failed.push(AutoOrganizeFailure {
                        post_id: post_id.clone(),
                        error: "Post not found".to_string(),
                    });
                    continue;
                }
            }
        };

        // 2. Classify via LLM.
        let classification = {
            let classifier = state.classifier.lock().await;
            match classifier.classify_post(&post).await {
                Ok(c) => c,
                Err(e) => {
                    failed.push(AutoOrganizeFailure {
                        post_id: post_id.clone(),
                        error: format!("Classification failed: {}", e),
                    });
                    continue;
                }
            }
        };

        // 3. Save classification.
        {
            let storage = state.storage.lock().map_err(|e| e.to_string())?;
            if let Err(e) = storage.save_classification(post_id, &classification) {
                failed.push(AutoOrganizeFailure {
                    post_id: post_id.clone(),
                    error: format!("Failed to save classification: {}", e),
                });
                continue;
            }
        }

        // 4. For each category, get or create a board and add a card.
        let categories = if classification.categories.is_empty() {
            vec!["Other".to_string()]
        } else {
            classification.categories.clone()
        };

        let mut post_organized = false;
        for category in &categories {
            // Skip categories the user has excluded.
            if excluded_lower.contains(&category.to_lowercase()) {
                continue;
            }

            let storage = state.storage.lock().map_err(|e| e.to_string())?;

            let (board, created) = storage
                .get_or_create_board(category, Some(&format!("Auto-created board for {} posts", category)))
                .map_err(|e| e.to_string())?;

            if created && !boards_created.contains(&board.name) {
                boards_created.push(board.name.clone());
            }

            // Skip if card already exists (prevent duplicates).
            match storage.card_exists(&board.id, post_id) {
                Ok(true) => {
                    post_organized = true;
                    continue;
                }
                Ok(false) => {}
                Err(e) => {
                    failed.push(AutoOrganizeFailure {
                        post_id: post_id.clone(),
                        error: format!("Failed to check card existence: {}", e),
                    });
                    continue;
                }
            }

            // Create the card with classification tags (unsaved / ephemeral).
            if let Err(e) = storage.create_card(
                &board.id,
                post_id,
                None,
                &classification.tags,
                false,
            ) {
                failed.push(AutoOrganizeFailure {
                    post_id: post_id.clone(),
                    error: format!("Failed to create card: {}", e),
                });
            } else {
                post_organized = true;
            }
        }

        // Count as organized if at least one card was placed, or if all
        // categories were excluded (classification was still saved).
        let all_excluded = categories.iter().all(|c| excluded_lower.contains(&c.to_lowercase()));
        if post_organized || all_excluded {
            organized += 1;
        }
    }

    Ok(AutoOrganizeResult {
        total,
        organized,
        failed,
        boards_created,
    })
}

// ===========================================================================
// Classifier config commands
// ===========================================================================

#[tauri::command]
async fn classifier_get_config(state: State<'_, AppState>) -> Result<ClassifierConfig, String> {
    let classifier = state.classifier.lock().await;
    Ok(classifier.get_config())
}

#[tauri::command]
async fn classifier_set_provider(
    provider: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let llm_provider = LlmProvider::from_str(&provider).map_err(|e| e.to_string())?;
    {
        let mut classifier = state.classifier.lock().await;
        classifier.set_provider(llm_provider);
    }
    // Persist to SQLite settings.
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .save_setting("classifier_provider", &provider)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn classifier_set_model(
    model: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut classifier = state.classifier.lock().await;
        classifier.set_model(model.clone());
    }
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .save_setting("classifier_model", &model)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn classifier_set_api_key(
    provider: String,
    api_key: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let llm_provider = LlmProvider::from_str(&provider).map_err(|e| e.to_string())?;

    // Store in Keychain.
    let keychain_key = match llm_provider {
        LlmProvider::Anthropic => "anthropic_api_key",
        LlmProvider::OpenAI => "openai_api_key",
        LlmProvider::Ollama => return Err("Ollama does not use API keys".to_string()),
    };
    SecretStore::set(keychain_key, &api_key).map_err(|e| e.to_string())?;

    // Load into Classifier in memory.
    let mut classifier = state.classifier.lock().await;
    classifier.set_api_key(&llm_provider, api_key);
    Ok(())
}

// ===========================================================================
// Tavily API key command
// ===========================================================================

#[tauri::command]
async fn set_tavily_api_key(
    api_key: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Store in Keychain.
    SecretStore::set("tavily_api_key", &api_key).map_err(|e| e.to_string())?;

    // Load into Classifier in memory.
    let mut classifier = state.classifier.lock().await;
    classifier.set_tavily_api_key(api_key);
    Ok(())
}

// ===========================================================================
// Enrichment commands (Learn Mode)
// ===========================================================================

#[tauri::command]
async fn enrich_post_learn(
    post_id: String,
    state: State<'_, AppState>,
) -> Result<Enrichment, String> {
    // 1. Check if enrichment is already cached in DB.
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        if let Some(cached) = storage.get_enrichment(&post_id).map_err(|e| e.to_string())? {
            return Ok(cached);
        }
    }

    // 2. Look up the post.
    let post = {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage
            .get_post_by_id(&post_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Post not found: {}", post_id))?
    };

    // 3. Run enrichment pipeline.
    let enrichment = {
        let classifier = state.classifier.lock().await;
        classifier
            .enrich_post(&post)
            .await
            .map_err(|e| e.to_string())?
    };

    // 4. Save enrichment to DB.
    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        storage
            .save_enrichment(&post_id, &enrichment)
            .map_err(|e| e.to_string())?;
    }

    Ok(enrichment)
}

#[tauri::command]
fn get_enrichment(
    post_id: String,
    state: State<'_, AppState>,
) -> Result<Option<Enrichment>, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .get_enrichment(&post_id)
        .map_err(|e| e.to_string())
}

// ===========================================================================
// Cleanup commands
// ===========================================================================

#[tauri::command]
fn cleanup_stale_posts(state: State<'_, AppState>) -> Result<usize, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.cleanup_stale_posts(86400).map_err(|e| e.to_string()) // 24 hours
}

// ===========================================================================
// Interest Tuning commands
// ===========================================================================

#[tauri::command]
fn record_interaction(
    event_type: String,
    board_id: Option<String>,
    card_id: Option<String>,
    post_id: Option<String>,
    category: Option<String>,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .record_interaction(
            &event_type,
            board_id.as_deref(),
            card_id.as_deref(),
            post_id.as_deref(),
            category.as_deref(),
            &tags,
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_interest_profile(state: State<'_, AppState>) -> Result<InterestProfile, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.get_interest_scores().map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_interest_profile(state: State<'_, AppState>) -> Result<usize, String> {
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage.clear_interactions().map_err(|e| e.to_string())
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
// TTS commands
// ===========================================================================

#[tauri::command]
async fn synthesize_speech(
    text: String,
    voice: String,
    state: State<'_, AppState>,
) -> Result<Vec<u8>, String> {
    let classifier = state.classifier.lock().await;
    let api_key = classifier
        .openai_api_key()
        .ok_or_else(|| "OpenAI API key not set".to_string())?
        .to_string();
    let client = classifier.http_client().clone();
    drop(classifier); // release lock before HTTP call

    let body = serde_json::json!({
        "model": "tts-1",
        "input": text,
        "voice": voice,
    });

    let resp = client
        .post("https://api.openai.com/v1/audio/speech")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to reach OpenAI TTS API: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI TTS returned HTTP {}: {}", status, text));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read TTS response: {}", e))?;

    Ok(bytes.to_vec())
}

// ===========================================================================
// Gmail commands
// ===========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GmailConfig {
    client_id: String,
    has_client_secret: bool,
    has_refresh_token: bool,
    filters: EmailFilter,
}

/// Save client_id to settings and client_secret to Keychain; update in-memory state.
#[tauri::command]
async fn gmail_set_credentials(
    client_id: String,
    client_secret: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut gmail = state.gmail.lock().await;
        gmail.client_id = client_id.clone();
        if !client_secret.is_empty() {
            SecretStore::set("gmail_client_secret", &client_secret).map_err(|e| e.to_string())?;
            gmail.client_secret = client_secret;
        }
    }
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .save_setting("gmail_client_id", &client_id)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Return current Gmail configuration (no secrets).
#[tauri::command]
async fn gmail_get_config(state: State<'_, AppState>) -> Result<GmailConfig, String> {
    let gmail = state.gmail.lock().await;
    Ok(GmailConfig {
        client_id: gmail.client_id.clone(),
        has_client_secret: !gmail.client_secret.is_empty(),
        has_refresh_token: gmail.refresh_token.is_some(),
        filters: gmail.filters.clone(),
    })
}

/// Update filter lists and persist them.
#[tauri::command]
async fn gmail_set_filters(
    from_addresses: Vec<String>,
    subject_keywords: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let filters = EmailFilter { from_addresses, subject_keywords };
    {
        let mut gmail = state.gmail.lock().await;
        gmail.filters = filters.clone();
    }
    let filters_json = serde_json::to_string(&filters).map_err(|e| e.to_string())?;
    let storage = state.storage.lock().map_err(|e| e.to_string())?;
    storage
        .save_setting("gmail_filters", &filters_json)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Start the OAuth2 flow: spawn a background listener, return the auth URL.
/// When auth completes, the background task emits "gmail-auth-complete" or "gmail-auth-error".
#[tauri::command]
async fn gmail_start_auth(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let (client_id, client_secret, http_client, auth_url) = {
        let gmail = state.gmail.lock().await;
        if gmail.client_id.is_empty() || gmail.client_secret.is_empty() {
            return Err(
                "Client ID and Client Secret must be saved before connecting.".to_string(),
            );
        }
        (
            gmail.client_id.clone(),
            gmail.client_secret.clone(),
            gmail.http_client.clone(),
            gmail.build_auth_url(),
        )
    };

    tokio::spawn(async move {
        match wait_for_oauth_callback(client_id, client_secret, http_client).await {
            Ok((access_token, refresh_token, expires_in)) => {
                if let Err(e) = SecretStore::set("gmail_refresh_token", &refresh_token) {
                    app_handle
                        .emit("gmail-auth-error", format!("Keychain error: {}", e))
                        .ok();
                    return;
                }
                let app_state = app_handle.state::<AppState>();
                let mut gmail = app_state.gmail.lock().await;
                gmail.access_token = Some(access_token);
                gmail.refresh_token = Some(refresh_token);
                gmail.token_expiry =
                    Some(chrono::Utc::now().timestamp() + expires_in);
                app_handle.emit("gmail-auth-complete", ()).ok();
            }
            Err(e) => {
                app_handle
                    .emit("gmail-auth-error", e.to_string())
                    .ok();
            }
        }
    });

    Ok(auth_url)
}

/// Remove Gmail tokens from Keychain and clear in-memory state.
#[tauri::command]
async fn gmail_revoke(state: State<'_, AppState>) -> Result<(), String> {
    SecretStore::delete("gmail_refresh_token").map_err(|e| e.to_string())?;
    let mut gmail = state.gmail.lock().await;
    gmail.access_token = None;
    gmail.refresh_token = None;
    gmail.token_expiry = None;
    Ok(())
}

/// Fetch emails matching the configured filters, save them to storage, return them.
#[tauri::command]
async fn fetch_gmail_posts(state: State<'_, AppState>) -> Result<Vec<Post>, String> {
    let posts = {
        let mut gmail = state.gmail.lock().await;
        gmail.fetch_posts().await.map_err(|e| e.to_string())?
    };

    {
        let storage = state.storage.lock().map_err(|e| e.to_string())?;
        for post in &posts {
            storage.save_post(post).map_err(|e| e.to_string())?;
        }
    }

    Ok(posts)
}

// ===========================================================================
// Application entry point
// ===========================================================================

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
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

            // Migrate enrichments table schema if needed (card_id -> post_id).
            if let Err(e) = storage.migrate_enrichments_to_post_id() {
                eprintln!("Warning: enrichments migration failed: {}", e);
            }

            // Add saved column to cards table if needed.
            if let Err(e) = storage.migrate_cards_add_saved() {
                eprintln!("Warning: cards saved migration failed: {}", e);
            }

            // Clean up stale posts (older than 24h with no saved cards).
            match storage.cleanup_stale_posts(86400) {
                Ok(count) if count > 0 => eprintln!("Cleaned up {} stale posts on startup", count),
                Err(e) => eprintln!("Warning: stale post cleanup failed: {}", e),
                _ => {}
            }

            // Restore RSS feeds from settings.
            let rss_feeds: Vec<String> = storage
                .get_setting("rss_feeds")
                .ok()
                .flatten()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default();
            let rss = RssConnector::new(rss_feeds);

            // Restore Gmail connector from settings + Keychain.
            let gmail_client_id = storage
                .get_setting("gmail_client_id")
                .ok()
                .flatten()
                .unwrap_or_default();
            let gmail_filters: EmailFilter = storage
                .get_setting("gmail_filters")
                .ok()
                .flatten()
                .and_then(|j| serde_json::from_str(&j).ok())
                .unwrap_or_default();
            let gmail_client_secret = SecretStore::get("gmail_client_secret")
                .ok()
                .flatten()
                .unwrap_or_default();
            let gmail_refresh_token = SecretStore::get("gmail_refresh_token")
                .ok()
                .flatten();
            let mut gmail = GmailConnector::new(gmail_client_id, gmail_client_secret, gmail_filters);
            gmail.refresh_token = gmail_refresh_token;

            // Initialize the classifier, restoring provider/model/url from settings.
            let saved_classifier_provider = storage.get_setting("classifier_provider").ok().flatten();
            let saved_classifier_model = storage.get_setting("classifier_model").ok().flatten();
            let saved_classifier_ollama_url = storage.get_setting("classifier_ollama_url").ok().flatten();

            let mut classifier = Classifier::new(saved_classifier_ollama_url, saved_classifier_model);

            // Restore provider if previously saved.
            if let Some(ref provider_str) = saved_classifier_provider {
                if let Ok(provider) = LlmProvider::from_str(provider_str) {
                    classifier.set_provider(provider);
                }
            }

            // Restore API keys from Keychain into memory.
            if let Ok(Some(key)) = SecretStore::get("anthropic_api_key") {
                classifier.set_api_key(&LlmProvider::Anthropic, key);
            }
            if let Ok(Some(key)) = SecretStore::get("openai_api_key") {
                classifier.set_api_key(&LlmProvider::OpenAI, key);
            }
            if let Ok(Some(key)) = SecretStore::get("tavily_api_key") {
                classifier.set_tavily_api_key(key);
            }

            // Capture warmup data before moving classifier into AppState.
            let warmup_client = classifier.http_client().clone();
            let warmup_has_openai = classifier.openai_api_key().is_some();

            // Build and manage application state.
            let app_state = AppState {
                storage: std::sync::Mutex::new(storage),
                rss: tokio::sync::Mutex::new(rss),
                gmail: tokio::sync::Mutex::new(gmail),
                classifier: tokio::sync::Mutex::new(classifier),
            };

            app.manage(app_state);

            // Pre-warm HTTP connections in the background so the first TTS /
            // classification request doesn't pay the full TCP+TLS handshake cost.
            tauri::async_runtime::spawn(async move {
                if warmup_has_openai {
                    let _ = warmup_client
                        .head("https://api.openai.com/v1/models")
                        .send()
                        .await;
                }
                let _ = warmup_client
                    .head("https://api.anthropic.com/v1/messages")
                    .send()
                    .await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Board commands
            create_board,
            get_boards,
            delete_board,
            get_board_card_counts,
            // Card commands
            create_card,
            get_cards_by_board,
            delete_card,
            toggle_card_saved,
            // Post commands
            get_posts,
            get_post_by_id,
            // RSS commands
            add_rss_feed,
            remove_rss_feed,
            list_rss_feeds,
            fetch_rss_posts,
            // Gmail commands
            gmail_set_credentials,
            gmail_get_config,
            gmail_set_filters,
            gmail_start_auth,
            gmail_revoke,
            fetch_gmail_posts,
            // Classifier commands
            auto_organize_posts,
            classifier_is_available,
            classifier_list_models,
            classify_post,
            summarize_post,
            generate_derivative,
            classifier_get_config,
            classifier_set_provider,
            classifier_set_model,
            classifier_set_api_key,
            // Tavily / Enrichment commands
            set_tavily_api_key,
            enrich_post_learn,
            get_enrichment,
            // Cleanup commands
            cleanup_stale_posts,
            // Interest Tuning commands
            record_interaction,
            get_interest_profile,
            clear_interest_profile,
            // Settings commands
            save_setting,
            get_setting,
            // TTS commands
            synthesize_speech,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
