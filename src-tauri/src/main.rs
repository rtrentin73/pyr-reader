// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod classifier;
mod connectors;
mod storage;

use classifier::{Classification, ClassifierConfig, Classifier, LlmProvider};
use connectors::rss::RssConnector;
use connectors::Post;
use storage::{Board, Card, SecretStore, StorageManager};

use std::str::FromStr;

use std::fs;
use tauri::{Manager, State};

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub storage: std::sync::Mutex<StorageManager>,
    pub rss: tokio::sync::Mutex<RssConnector>,
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

            // Build and manage application state.
            let app_state = AppState {
                storage: std::sync::Mutex::new(storage),
                rss: tokio::sync::Mutex::new(rss),
                classifier: tokio::sync::Mutex::new(classifier),
            };

            app.manage(app_state);

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
            // Classifier commands
            classifier_is_available,
            classifier_list_models,
            classify_post,
            summarize_post,
            generate_derivative,
            classifier_get_config,
            classifier_set_provider,
            classifier_set_model,
            classifier_set_api_key,
            // Settings commands
            save_setting,
            get_setting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
