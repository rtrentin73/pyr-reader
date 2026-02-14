# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**pyr-reader** is a personal Mac app designed to ingest posts from social media platforms and RSS feeds, classify them into boards/cards, summarize content, and generate derivative posts.

### Data Sources (Priority Order)
1. **X (Twitter)** - Primary integration, must be fully compliant with X API
2. **RSS/News feeds** - Secondary integration using standard APIs
3. **LinkedIn** - Future integration (treat as separate connector due to ToS and anti-scraping concerns)

## Tech Stack

- **Backend**: Rust with Tauri framework
- **Frontend**: Vanilla JS + Vite
- **Package Manager**: **bun** (not npm)

## Development Commands

```bash
# Install dependencies
bun install

# Run in development mode (opens app with hot reload)
bun run tauri:dev

# Build for production
bun run tauri:build

# Frontend only (for testing UI without Tauri)
bun run dev
```

## Architecture

### Backend (Rust - src-tauri/)

**Modular connector pattern**: Each data source implements the `Connector` trait:
- `connectors/x_twitter.rs` - X API integration (OAuth 2.0, must use official API only)
- `connectors/rss.rs` - RSS/Atom feed parsing
- Future: `connectors/linkedin.rs` (blocked on ToS review)

**Storage** (`storage/`): Local persistence for boards, cards, and classifications. Consider SQLite (rusqlite) or embedded DB (sled).

**Classifier** (`classifier/`): Content classification, summarization, and derivative post generation. Will likely integrate with LLM APIs.

**Main app** (`main.rs`): Tauri application setup, command handlers, window management.

### Frontend (src/)

Simple Vite-based frontend. Currently vanilla JS - can evolve to React/Vue/Svelte as needed.

## API Compliance Requirements

- **X/Twitter**: Must use official X API v2 endpoints only - no scraping or ToS violations
- **RSS/News**: Use standard RSS/Atom parsing with proper user-agent headers
- **LinkedIn**: Defer implementation until X and RSS are working; requires careful ToS review

## Key Design Principles

- **Connector isolation**: Each data source is a separate module with the same interface
- **Async-first**: Use `tokio` for all I/O operations
- **Error handling**: Use `anyhow::Result` consistently, propagate errors to frontend
- **Security**: Never commit API keys; use secure credential storage (consider keychain integration)
