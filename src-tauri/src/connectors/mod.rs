// Data source connectors module
pub mod x_twitter;
pub mod rss;
pub mod linkedin;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub source: DataSource,
    pub author: String,
    pub content: String,
    pub url: Option<String>,
    pub timestamp: i64,
    pub raw_data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    XTwitter,
    RSS,
    LinkedIn,
}

pub trait Connector {
    async fn fetch_posts(&self) -> anyhow::Result<Vec<Post>>;
    async fn authenticate(&mut self) -> anyhow::Result<()>;
    fn is_authenticated(&self) -> bool;
}
