// RSS/News feed connector
use super::{normalize_content, Connector, DataSource, Post};
use anyhow::Result;
use chrono::Utc;
use feed_rs::parser;
use log::error;
use reqwest::Client;
use serde_json::json;
use uuid::Uuid;

pub struct RssConnector {
    feed_urls: Vec<String>,
    client: Client,
}

impl RssConnector {
    pub fn new(feed_urls: Vec<String>) -> Self {
        let client = Client::builder()
            .user_agent("PyrReader/0.1")
            .build()
            .expect("Failed to build HTTP client");

        Self { feed_urls, client }
    }

    pub fn add_feed(&mut self, url: String) {
        self.feed_urls.push(url);
    }

    pub fn remove_feed(&mut self, url: &str) {
        self.feed_urls.retain(|u| u != url);
    }

    pub fn list_feeds(&self) -> &[String] {
        &self.feed_urls
    }

    /// Fetch and parse a single feed URL, converting its entries into Posts.
    async fn fetch_feed(&self, feed_url: &str) -> Result<Vec<Post>> {
        let response = self
            .client
            .get(feed_url)
            .send()
            .await?
            .error_for_status()?;

        let bytes = response.bytes().await?;
        let feed = parser::parse(&bytes[..])?;

        let feed_title = feed
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default();

        let mut posts = Vec::with_capacity(feed.entries.len());

        for entry in &feed.entries {
            // ID: use the entry's own ID if present, otherwise generate a UUID
            let id = if entry.id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                entry.id.clone()
            };

            // Author: first author name -> feed title -> "Unknown"
            let author = entry
                .authors
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| {
                    if feed_title.is_empty() {
                        "Unknown".to_string()
                    } else {
                        feed_title.clone()
                    }
                });

            // Content: prefer summary text, then first content body, fallback to empty
            let raw_content = entry
                .summary
                .as_ref()
                .map(|s| s.content.clone())
                .or_else(|| {
                    entry
                        .content
                        .as_ref()
                        .and_then(|c| c.body.clone())
                })
                .unwrap_or_default();

            let content = normalize_content(&raw_content);

            // URL: first link's href
            let url = entry.links.first().map(|link| link.href.clone());

            // Timestamp: published -> updated -> now
            let timestamp = entry
                .published
                .or(entry.updated)
                .map(|dt| dt.timestamp())
                .unwrap_or_else(|| Utc::now().timestamp());

            // Raw data: serialize title, summary, and links for downstream use
            let title_text = entry
                .title
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_default();

            let summary_text = entry
                .summary
                .as_ref()
                .map(|s| s.content.clone())
                .unwrap_or_default();

            let links: Vec<String> = entry.links.iter().map(|l| l.href.clone()).collect();

            let raw_data = json!({
                "title": title_text,
                "summary": summary_text,
                "links": links,
            });

            posts.push(Post {
                id,
                source: DataSource::RSS,
                author,
                content,
                url,
                timestamp,
                raw_data,
            });
        }

        Ok(posts)
    }
}

impl Connector for RssConnector {
    async fn fetch_posts(&self) -> Result<Vec<Post>> {
        let mut all_posts = Vec::new();

        for feed_url in &self.feed_urls {
            match self.fetch_feed(feed_url).await {
                Ok(posts) => {
                    all_posts.extend(posts);
                }
                Err(e) => {
                    error!("Failed to fetch feed '{}': {}", feed_url, e);
                }
            }
        }

        Ok(all_posts)
    }

    fn is_authenticated(&self) -> bool {
        true // RSS is always "authenticated"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_connector() {
        let urls = vec![
            "https://example.com/feed.xml".to_string(),
            "https://example.com/rss".to_string(),
        ];
        let connector = RssConnector::new(urls.clone());
        assert_eq!(connector.list_feeds(), &urls);
        assert!(connector.is_authenticated());
    }

    #[test]
    fn test_add_feed() {
        let mut connector = RssConnector::new(vec![]);
        connector.add_feed("https://example.com/feed.xml".to_string());
        assert_eq!(connector.list_feeds().len(), 1);
        assert_eq!(connector.list_feeds()[0], "https://example.com/feed.xml");
    }

    #[test]
    fn test_remove_feed() {
        let mut connector = RssConnector::new(vec![
            "https://a.com/feed".to_string(),
            "https://b.com/feed".to_string(),
        ]);
        connector.remove_feed("https://a.com/feed");
        assert_eq!(connector.list_feeds(), &["https://b.com/feed".to_string()]);
    }

    #[test]
    fn test_remove_feed_not_found() {
        let mut connector = RssConnector::new(vec!["https://a.com/feed".to_string()]);
        connector.remove_feed("https://nonexistent.com/feed");
        assert_eq!(connector.list_feeds().len(), 1);
    }
}
