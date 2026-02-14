# Pyr Reader

Personal Mac app to ingest posts from X (Twitter), RSS feeds, and news sources, classify them into boards/cards, summarize content, and generate derivative posts.

## Features

- **Multi-source ingestion**: X (Twitter) API, RSS feeds, news APIs
- **Content classification**: Organize posts into customizable boards and cards
- **Summarization**: AI-powered content summarization
- **Derivative content**: Generate new posts based on ingested content

## Tech Stack

- **Backend**: Rust (Tauri)
- **Frontend**: Vanilla JS + Vite
- **Package Manager**: Bun

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (latest)

## Development

Install dependencies:
```bash
bun install
```

Run in development mode:
```bash
bun run tauri:dev
```

Build for production:
```bash
bun run tauri:build
```

## Project Structure

```
pyr-reader/
├── src/                    # Frontend source
│   ├── main.js            # Frontend entry point
│   └── styles.css         # Styles
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── main.rs        # Application entry
│   │   ├── connectors/    # Data source connectors
│   │   │   ├── x_twitter.rs
│   │   │   └── rss.rs
│   │   ├── storage/       # Local data storage
│   │   └── classifier/    # Classification & summarization
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── package.json           # Node/Bun dependencies
└── vite.config.js         # Vite configuration
```

## API Compliance

- **X (Twitter)**: Uses official X API v2 only - no scraping
- **RSS/News**: Standard RSS/Atom feed parsing
- **LinkedIn**: Future implementation (requires ToS review)

## License

Private project
