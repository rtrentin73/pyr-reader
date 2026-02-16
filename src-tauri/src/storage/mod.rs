// Local storage module for boards, cards, and classifications
// Uses SQLite via rusqlite for persistent local storage

pub mod secrets;
pub use secrets::SecretStore;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::classifier::Classification;
use crate::connectors::{DataSource, Post};

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub board_id: String,
    pub post_id: String,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
}

// ---------------------------------------------------------------------------
// DataSource <-> String helpers
// ---------------------------------------------------------------------------

fn datasource_to_string(ds: &DataSource) -> &'static str {
    match ds {
        DataSource::RSS => "RSS",
    }
}

fn datasource_from_string(s: &str) -> Result<DataSource> {
    match s {
        "RSS" => Ok(DataSource::RSS),
        other => Err(anyhow::anyhow!("Unknown DataSource: {}", other)),
    }
}

// ---------------------------------------------------------------------------
// StorageManager
// ---------------------------------------------------------------------------

pub struct StorageManager {
    conn: Connection,
}

impl StorageManager {
    /// Open (or create) the SQLite database at `db_path` and run migrations.
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open SQLite database at {}", db_path))?;

        // Enable WAL mode for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        // Enforce foreign key constraints.
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        let manager = Self { conn };
        manager.run_migrations()?;
        Ok(manager)
    }

    // -----------------------------------------------------------------------
    // Migrations
    // -----------------------------------------------------------------------

    fn run_migrations(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS posts (
                    id         TEXT PRIMARY KEY,
                    source     TEXT NOT NULL,
                    author     TEXT NOT NULL,
                    content    TEXT NOT NULL,
                    url        TEXT,
                    timestamp  INTEGER NOT NULL,
                    raw_data   TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS boards (
                    id          TEXT PRIMARY KEY,
                    name        TEXT NOT NULL,
                    description TEXT,
                    created_at  TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS cards (
                    id         TEXT PRIMARY KEY,
                    board_id   TEXT NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
                    post_id    TEXT NOT NULL REFERENCES posts(id),
                    summary    TEXT,
                    tags       TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS classifications (
                    id         TEXT PRIMARY KEY,
                    post_id    TEXT NOT NULL UNIQUE REFERENCES posts(id),
                    categories TEXT NOT NULL,
                    tags       TEXT NOT NULL,
                    sentiment  TEXT,
                    confidence REAL NOT NULL
                );

                CREATE TABLE IF NOT EXISTS settings (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                ",
            )
            .context("Failed to run database migrations")?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Posts
    // -----------------------------------------------------------------------

    /// Upsert a post by its `id`.
    pub fn save_post(&self, post: &Post) -> Result<()> {
        let source_str = datasource_to_string(&post.source);
        let raw_data_str =
            serde_json::to_string(&post.raw_data).context("Failed to serialize raw_data")?;
        let now = Utc::now().to_rfc3339();

        self.conn
            .execute(
                "INSERT INTO posts (id, source, author, content, url, timestamp, raw_data, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                     source     = excluded.source,
                     author     = excluded.author,
                     content    = excluded.content,
                     url        = excluded.url,
                     timestamp  = excluded.timestamp,
                     raw_data   = excluded.raw_data",
                params![
                    post.id,
                    source_str,
                    post.author,
                    post.content,
                    post.url,
                    post.timestamp,
                    raw_data_str,
                    now,
                ],
            )
            .context("Failed to save post")?;

        Ok(())
    }

    /// Retrieve posts ordered by timestamp descending with limit/offset.
    pub fn get_posts(&self, limit: i64, offset: i64) -> Result<Vec<Post>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source, author, content, url, timestamp, raw_data
             FROM posts
             ORDER BY timestamp DESC
             LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(params![limit, offset], |row| {
            let id: String = row.get(0)?;
            let source_str: String = row.get(1)?;
            let author: String = row.get(2)?;
            let content: String = row.get(3)?;
            let url: Option<String> = row.get(4)?;
            let timestamp: i64 = row.get(5)?;
            let raw_data_str: String = row.get(6)?;
            Ok((id, source_str, author, content, url, timestamp, raw_data_str))
        })?;

        let mut posts = Vec::new();
        for row in rows {
            let (id, source_str, author, content, url, timestamp, raw_data_str) = row?;
            let source = datasource_from_string(&source_str)?;
            let raw_data: serde_json::Value = serde_json::from_str(&raw_data_str)?;
            posts.push(Post {
                id,
                source,
                author,
                content,
                url,
                timestamp,
                raw_data,
            });
        }

        Ok(posts)
    }

    /// Retrieve a single post by its `id`.
    pub fn get_post_by_id(&self, id: &str) -> Result<Option<Post>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source, author, content, url, timestamp, raw_data
             FROM posts
             WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            let id: String = row.get(0)?;
            let source_str: String = row.get(1)?;
            let author: String = row.get(2)?;
            let content: String = row.get(3)?;
            let url: Option<String> = row.get(4)?;
            let timestamp: i64 = row.get(5)?;
            let raw_data_str: String = row.get(6)?;
            Ok((id, source_str, author, content, url, timestamp, raw_data_str))
        })?;

        match rows.next() {
            Some(row) => {
                let (id, source_str, author, content, url, timestamp, raw_data_str) = row?;
                let source = datasource_from_string(&source_str)?;
                let raw_data: serde_json::Value = serde_json::from_str(&raw_data_str)?;
                Ok(Some(Post {
                    id,
                    source,
                    author,
                    content,
                    url,
                    timestamp,
                    raw_data,
                }))
            }
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // Boards
    // -----------------------------------------------------------------------

    /// Create a new board and return the created `Board`.
    pub fn create_board(&self, name: &str, description: Option<&str>) -> Result<Board> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        self.conn
            .execute(
                "INSERT INTO boards (id, name, description, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, name, description, now],
            )
            .context("Failed to create board")?;

        Ok(Board {
            id,
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            created_at: now,
        })
    }

    /// Retrieve all boards ordered by creation date descending.
    pub fn get_boards(&self) -> Result<Vec<Board>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, created_at
             FROM boards
             ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Board {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut boards = Vec::new();
        for row in rows {
            boards.push(row?);
        }

        Ok(boards)
    }

    /// Retrieve a single board by its `id`.
    pub fn get_board_by_id(&self, id: &str) -> Result<Option<Board>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, created_at
             FROM boards
             WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(Board {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Delete a board and all of its associated cards (via ON DELETE CASCADE).
    pub fn delete_board(&self, id: &str) -> Result<()> {
        // Cards are deleted automatically by the ON DELETE CASCADE constraint,
        // but we also delete them explicitly to be safe in case PRAGMA
        // foreign_keys is not enforced at runtime on every connection.
        self.conn
            .execute("DELETE FROM cards WHERE board_id = ?1", params![id])
            .context("Failed to delete cards for board")?;
        self.conn
            .execute("DELETE FROM boards WHERE id = ?1", params![id])
            .context("Failed to delete board")?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Cards
    // -----------------------------------------------------------------------

    /// Create a new card linking a post to a board.
    pub fn create_card(
        &self,
        board_id: &str,
        post_id: &str,
        summary: Option<&str>,
        tags: &[String],
    ) -> Result<Card> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(tags).context("Failed to serialize tags")?;

        self.conn
            .execute(
                "INSERT INTO cards (id, board_id, post_id, summary, tags, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, board_id, post_id, summary, tags_json, now],
            )
            .context("Failed to create card")?;

        Ok(Card {
            id,
            board_id: board_id.to_string(),
            post_id: post_id.to_string(),
            summary: summary.map(|s| s.to_string()),
            tags: tags.to_vec(),
            created_at: now,
        })
    }

    /// Retrieve all cards belonging to a board, ordered by creation date descending.
    pub fn get_cards_by_board(&self, board_id: &str) -> Result<Vec<Card>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, board_id, post_id, summary, tags, created_at
             FROM cards
             WHERE board_id = ?1
             ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map(params![board_id], |row| {
            let tags_str: String = row.get(4)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                tags_str,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut cards = Vec::new();
        for row in rows {
            let (id, board_id, post_id, summary, tags_str, created_at) = row?;
            let tags: Vec<String> = serde_json::from_str(&tags_str)
                .context("Failed to deserialize card tags")?;
            cards.push(Card {
                id,
                board_id,
                post_id,
                summary,
                tags,
                created_at,
            });
        }

        Ok(cards)
    }

    /// Delete a single card by its `id`.
    pub fn delete_card(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM cards WHERE id = ?1", params![id])
            .context("Failed to delete card")?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Classifications
    // -----------------------------------------------------------------------

    /// Upsert a classification for a given post.
    pub fn save_classification(
        &self,
        post_id: &str,
        classification: &Classification,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let categories_json = serde_json::to_string(&classification.categories)
            .context("Failed to serialize categories")?;
        let tags_json =
            serde_json::to_string(&classification.tags).context("Failed to serialize tags")?;

        self.conn
            .execute(
                "INSERT INTO classifications (id, post_id, categories, tags, sentiment, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(post_id) DO UPDATE SET
                     categories = excluded.categories,
                     tags       = excluded.tags,
                     sentiment  = excluded.sentiment,
                     confidence = excluded.confidence",
                params![
                    id,
                    post_id,
                    categories_json,
                    tags_json,
                    classification.sentiment,
                    classification.confidence,
                ],
            )
            .context("Failed to save classification")?;

        Ok(())
    }

    /// Retrieve the classification for a given post.
    pub fn get_classification(&self, post_id: &str) -> Result<Option<Classification>> {
        let mut stmt = self.conn.prepare(
            "SELECT categories, tags, sentiment, confidence
             FROM classifications
             WHERE post_id = ?1",
        )?;

        let mut rows = stmt.query_map(params![post_id], |row| {
            let categories_str: String = row.get(0)?;
            let tags_str: String = row.get(1)?;
            let sentiment: Option<String> = row.get(2)?;
            let confidence: f32 = row.get(3)?;
            Ok((categories_str, tags_str, sentiment, confidence))
        })?;

        match rows.next() {
            Some(row) => {
                let (categories_str, tags_str, sentiment, confidence) = row?;
                let categories: Vec<String> = serde_json::from_str(&categories_str)
                    .context("Failed to deserialize classification categories")?;
                let tags: Vec<String> = serde_json::from_str(&tags_str)
                    .context("Failed to deserialize classification tags")?;
                Ok(Some(Classification {
                    categories,
                    tags,
                    sentiment,
                    confidence,
                }))
            }
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // Settings
    // -----------------------------------------------------------------------

    /// Upsert a key-value setting.
    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO settings (key, value)
                 VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .context("Failed to save setting")?;

        Ok(())
    }

    /// Retrieve a setting value by key.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?1")?;

        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;

        match rows.next() {
            Some(value) => Ok(Some(value?)),
            None => Ok(None),
        }
    }

    /// Delete a setting by key.
    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", params![key])
            .context("Failed to delete setting")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper: create an in-memory StorageManager for testing.
    fn test_storage() -> StorageManager {
        StorageManager::new(":memory:").expect("Failed to create in-memory storage")
    }

    fn sample_post(id: &str) -> Post {
        Post {
            id: id.to_string(),
            source: DataSource::XTwitter,
            author: "testuser".to_string(),
            content: "Hello world".to_string(),
            url: Some("https://x.com/testuser/status/1".to_string()),
            timestamp: 1700000000,
            raw_data: json!({"key": "value"}),
        }
    }

    #[test]
    fn test_save_and_get_post() {
        let storage = test_storage();
        let post = sample_post("post-1");

        storage.save_post(&post).unwrap();

        let retrieved = storage.get_post_by_id("post-1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "post-1");
        assert_eq!(retrieved.author, "testuser");
        assert_eq!(retrieved.content, "Hello world");
    }

    #[test]
    fn test_upsert_post() {
        let storage = test_storage();
        let mut post = sample_post("post-1");

        storage.save_post(&post).unwrap();
        post.content = "Updated content".to_string();
        storage.save_post(&post).unwrap();

        let retrieved = storage.get_post_by_id("post-1").unwrap().unwrap();
        assert_eq!(retrieved.content, "Updated content");
    }

    #[test]
    fn test_get_posts_pagination() {
        let storage = test_storage();

        for i in 0..5 {
            let mut post = sample_post(&format!("post-{}", i));
            post.timestamp = 1700000000 + i;
            storage.save_post(&post).unwrap();
        }

        let page = storage.get_posts(2, 0).unwrap();
        assert_eq!(page.len(), 2);
        // Most recent first
        assert_eq!(page[0].id, "post-4");
        assert_eq!(page[1].id, "post-3");

        let page2 = storage.get_posts(2, 2).unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].id, "post-2");
    }

    #[test]
    fn test_get_post_not_found() {
        let storage = test_storage();
        let result = storage.get_post_by_id("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_create_and_get_boards() {
        let storage = test_storage();

        let board = storage.create_board("Tech", Some("Technology posts")).unwrap();
        assert_eq!(board.name, "Tech");
        assert_eq!(board.description, Some("Technology posts".to_string()));
        assert!(!board.id.is_empty());

        let boards = storage.get_boards().unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].name, "Tech");
    }

    #[test]
    fn test_get_board_by_id() {
        let storage = test_storage();
        let board = storage.create_board("News", None).unwrap();

        let retrieved = storage.get_board_by_id(&board.id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "News");

        let missing = storage.get_board_by_id("nonexistent").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_delete_board_cascades_cards() {
        let storage = test_storage();
        let post = sample_post("post-1");
        storage.save_post(&post).unwrap();

        let board = storage.create_board("Board1", None).unwrap();
        storage
            .create_card(&board.id, "post-1", Some("Summary"), &["tag1".to_string()])
            .unwrap();

        let cards = storage.get_cards_by_board(&board.id).unwrap();
        assert_eq!(cards.len(), 1);

        storage.delete_board(&board.id).unwrap();

        let boards = storage.get_boards().unwrap();
        assert!(boards.is_empty());

        let cards = storage.get_cards_by_board(&board.id).unwrap();
        assert!(cards.is_empty());
    }

    #[test]
    fn test_create_and_get_cards() {
        let storage = test_storage();
        let post = sample_post("post-1");
        storage.save_post(&post).unwrap();

        let board = storage.create_board("Board1", None).unwrap();
        let tags = vec!["rust".to_string(), "tauri".to_string()];
        let card = storage
            .create_card(&board.id, "post-1", Some("A summary"), &tags)
            .unwrap();

        assert_eq!(card.board_id, board.id);
        assert_eq!(card.post_id, "post-1");
        assert_eq!(card.summary, Some("A summary".to_string()));
        assert_eq!(card.tags, tags);

        let cards = storage.get_cards_by_board(&board.id).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].tags, tags);
    }

    #[test]
    fn test_delete_card() {
        let storage = test_storage();
        let post = sample_post("post-1");
        storage.save_post(&post).unwrap();

        let board = storage.create_board("Board1", None).unwrap();
        let card = storage
            .create_card(&board.id, "post-1", None, &[])
            .unwrap();

        storage.delete_card(&card.id).unwrap();

        let cards = storage.get_cards_by_board(&board.id).unwrap();
        assert!(cards.is_empty());
    }

    #[test]
    fn test_save_and_get_classification() {
        let storage = test_storage();
        let post = sample_post("post-1");
        storage.save_post(&post).unwrap();

        let classification = Classification {
            categories: vec!["tech".to_string(), "news".to_string()],
            tags: vec!["rust".to_string()],
            sentiment: Some("positive".to_string()),
            confidence: 0.95,
        };

        storage
            .save_classification("post-1", &classification)
            .unwrap();

        let retrieved = storage.get_classification("post-1").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.categories, vec!["tech", "news"]);
        assert_eq!(retrieved.tags, vec!["rust"]);
        assert_eq!(retrieved.sentiment, Some("positive".to_string()));
        assert!((retrieved.confidence - 0.95).abs() < f32::EPSILON);
    }

    #[test]
    fn test_upsert_classification() {
        let storage = test_storage();
        let post = sample_post("post-1");
        storage.save_post(&post).unwrap();

        let c1 = Classification {
            categories: vec!["tech".to_string()],
            tags: vec![],
            sentiment: None,
            confidence: 0.5,
        };
        storage.save_classification("post-1", &c1).unwrap();

        let c2 = Classification {
            categories: vec!["science".to_string()],
            tags: vec!["ai".to_string()],
            sentiment: Some("neutral".to_string()),
            confidence: 0.8,
        };
        storage.save_classification("post-1", &c2).unwrap();

        let retrieved = storage.get_classification("post-1").unwrap().unwrap();
        assert_eq!(retrieved.categories, vec!["science"]);
        assert_eq!(retrieved.tags, vec!["ai"]);
        assert!((retrieved.confidence - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_get_classification_not_found() {
        let storage = test_storage();
        let result = storage.get_classification("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_save_and_get_setting() {
        let storage = test_storage();

        storage.save_setting("theme", "dark").unwrap();
        let value = storage.get_setting("theme").unwrap();
        assert_eq!(value, Some("dark".to_string()));
    }

    #[test]
    fn test_upsert_setting() {
        let storage = test_storage();

        storage.save_setting("theme", "dark").unwrap();
        storage.save_setting("theme", "light").unwrap();

        let value = storage.get_setting("theme").unwrap();
        assert_eq!(value, Some("light".to_string()));
    }

    #[test]
    fn test_get_setting_not_found() {
        let storage = test_storage();
        let value = storage.get_setting("nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_datasource_roundtrip() {
        // Verify all DataSource variants survive a string round-trip.
        for ds in &[DataSource::RSS] {
            let s = datasource_to_string(ds);
            let back = datasource_from_string(s).unwrap();
            assert_eq!(datasource_to_string(&back), s);
        }
    }
}
