# Pyr Reader

![Pyr Reader](app-icon.png)

A native macOS app that aggregates content from RSS feeds and Gmail, classifies it with AI into organized boards, and helps you learn from what you read — with summarization, web research, and text-to-speech.

## Features

- **Multi-source ingestion** — RSS/Atom feeds with scheduled auto-fetch, Gmail via OAuth2 with sender/subject filtering
- **AI classification** — Automatically organize posts into boards using Ollama (local), OpenAI, or Anthropic Claude
- **Summarization & derivatives** — Generate AI summaries or derivative posts from any content
- **Learn mode** — Deep web research on any topic via Tavily API, with source references and Markdown export
- **Text-to-speech** — Listen to posts and research using browser TTS or OpenAI voices (6 voices, adjustable speed)
- **Interest profiling** — Tracks your reading patterns and surfaces what matters most with a "For You" filter
- **Dashboard** — Visual board grid with gradient cards, emoji badges, and interest indicators
- **Dark mode** — Full light/dark/system theme support
- **Secure storage** — API keys and tokens stored in macOS Keychain, SQLite for local data

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop Framework | Tauri 2 |
| Backend | Rust + Tokio |
| Frontend | Vanilla JS + Vite |
| Package Manager | Bun |
| Database | SQLite (rusqlite) |
| Secrets | macOS Keychain |
| AI Providers | OpenAI, Anthropic Claude, Ollama |
| Web Research | Tavily API |

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (latest)

## Development

```bash
# Install dependencies
bun install

# Run in development mode
bun run tauri:dev

# Build for production
bun run tauri:build
```

## Project Structure

```
pyr-reader/
├── src/                        # Frontend
│   ├── main.js                 # App entry point & UI logic
│   └── styles.css              # Styles with dark mode support
├── src-tauri/                  # Rust backend
│   ├── src/
│   │   ├── main.rs             # Tauri commands & app state
│   │   ├── connectors/         # Data source connectors
│   │   │   ├── mod.rs          # Post struct, Connector trait
│   │   │   ├── rss.rs          # RSS/Atom feed connector
│   │   │   └── gmail.rs        # Gmail OAuth2 connector
│   │   ├── storage/
│   │   │   ├── mod.rs          # SQLite persistence
│   │   │   └── secrets.rs      # macOS Keychain wrapper
│   │   └── classifier/
│   │       └── mod.rs          # Multi-provider LLM classification
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
└── vite.config.js
```

## License

Private project
