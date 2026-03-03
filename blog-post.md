# Meet Pyr Reader: An AI-Powered Content Hub Built with Rust and Tauri

I built a desktop app to solve a problem I kept running into: information overload. Between RSS feeds, email newsletters, and social media, I was drowning in content with no good way to organize, prioritize, or actually *learn* from it.

**Pyr Reader** is my answer — a native macOS app that pulls content from multiple sources, classifies it with AI, and helps me focus on what actually matters.

![Pyr Reader app icon](app-icon.png)

*Named after the Great Pyrenees — a loyal, watchful companion. Pyr Reader watches over your information feeds so you don't have to.*

---

## The Problem

Every morning I'd open a dozen tabs: RSS reader, Gmail, Twitter, news sites. I'd skim headlines, save some links "for later" (we all know how that goes), and close everything feeling like I missed something important.

What I wanted was simple:
- **One place** to see everything
- **Smart organization** that learns what I care about
- **Deeper engagement** with content I choose — summaries, research, even audio playback

So I built it.

## The Stack

I went with **Rust + Tauri 2** for the backend and **vanilla JavaScript + Vite** for the frontend. No React, no Vue — just clean JS that's fast and easy to iterate on. **Bun** as the package manager keeps everything snappy.

Why Tauri over Electron? The binary is tiny, startup is near-instant, and Rust gives me safe concurrency for background fetching without any GC pauses. It feels like a real Mac app because it practically is one.

| Layer | Technology |
|-------|-----------|
| Desktop Framework | Tauri 2 |
| Backend | Rust + Tokio |
| Frontend | Vanilla JS + Vite |
| Database | SQLite (rusqlite) |
| Secrets | macOS Keychain |
| AI | OpenAI, Claude, Ollama |

---

## The Dashboard

When you open Pyr Reader, you land on the Dashboard — a visual grid of your boards, each with a gradient header and emoji badge.

<!-- SCREENSHOT: Dashboard view showing the board grid with colorful gradient cards, card counts, and interest indicator dots -->
> **[Screenshot: Dashboard]** *The dashboard showing boards organized as colorful gradient cards with emoji badges, card counts, and interest indicators.*

Each board represents a topic or category: Tech, Science, Business, Design — whatever you set up. The interest dots (one to three) show you at a glance which topics you've been engaging with most. It's a subtle but powerful feedback loop that helps you notice your own reading patterns.

The "For You" toggle filters the dashboard down to boards matching your interests, which builds automatically from your interactions — no explicit configuration needed.

---

## Pulling in Content

### RSS Feeds

Adding an RSS feed is dead simple: paste a URL, give it a name, and hit Add. Pyr Reader uses the `feed_rs` crate under the hood to handle RSS 2.0 and Atom feeds gracefully.

<!-- SCREENSHOT: RSS Feeds view showing the feed list, the add-feed form, and some fetched posts -->
> **[Screenshot: RSS Feeds]** *The RSS feed management view with feed list, add form, and fetched posts below.*

The real power is in **scheduled auto-fetch**. Set an interval (15 minutes to 4 hours), and Pyr Reader quietly pulls new posts in the background. Pair that with **auto-organize** and incoming posts get classified and sorted into boards automatically — no manual triage needed.

### Gmail Integration

For newsletters and email digests, there's a Gmail connector with full OAuth2 authentication. You can filter by sender address or subject keyword, so only relevant emails make it into your feed.

<!-- SCREENSHOT: Gmail settings showing OAuth status, filters (from_address and subject chips), and the Connect button -->
> **[Screenshot: Gmail]** *Gmail connector with OAuth2 authentication, sender/subject filters, and connection status.*

The OAuth flow uses a localhost callback server and stores tokens securely in the macOS Keychain — no credentials ever touch the filesystem.

---

## AI Classification

This is where things get interesting. Pyr Reader integrates with three LLM providers:

- **Ollama** — for fully local, private classification
- **OpenAI** — GPT models via API
- **Anthropic Claude** — for when you want the best reasoning

<!-- SCREENSHOT: Classifier settings view showing provider selection (Ollama/OpenAI/Anthropic), model picker, and API key fields -->
> **[Screenshot: Classifier]** *The Classifier settings page where you choose your AI provider, model, and configure API keys.*

From any post, you can:
- **Classify** — AI suggests which board it belongs to
- **Summarize** — get a concise summary
- **Generate Derivative** — create a new post inspired by the source content

All of this happens through clean Tauri commands, so the UI stays responsive while Rust handles the API calls in the background.

---

## Deep Learning with "Learn Mode"

My favorite feature. When you find a post that sparks your curiosity, hit the **Learn** button and Pyr Reader uses the [Tavily API](https://tavily.com/) to run web research on the topic.

<!-- SCREENSHOT: Post detail modal with the Learn/enrichment panel expanded, showing the synthesized research, sources with snippets, and Save .md / Play buttons -->
> **[Screenshot: Learn Mode]** *The enrichment panel showing AI-synthesized research with numbered sources, snippets, and action buttons.*

You get back:
- A synthesized research summary
- Numbered source references with titles and snippets
- Options to **copy**, **save as Markdown**, or **listen via TTS**

It transforms passive reading into active learning. I've found myself going down fascinating rabbit holes I never would have explored otherwise.

---

## Text-to-Speech

Sometimes I want to absorb content while doing something else. Pyr Reader offers two TTS engines:

- **Browser Web Speech API** — free, works offline
- **OpenAI TTS** — six voices (alloy, echo, fable, onyx, nova, shimmer), significantly more natural

<!-- SCREENSHOT: A board card or post detail showing the TTS playback controls — play button, speed selector (0.75x to 2x), and the global stop button in the sidebar -->
> **[Screenshot: TTS Controls]** *Text-to-speech controls with voice selection, playback speed (0.75x–2x), and the global stop button.*

The OpenAI implementation does smart chunking — text is split at sentence boundaries into ~800-character chunks, and the next chunk prefetches while the current one plays. The result is seamless, uninterrupted playback.

A global stop button in the sidebar lets you kill TTS from anywhere in the app. Small detail, but it matters.

---

## Interest Profiling

Pyr Reader quietly tracks your interactions — which boards you visit, which posts you read, what you save, what you listen to — and builds an interest profile over time.

<!-- SCREENSHOT: The interest profile section (likely in Dashboard or a settings area) showing top categories with scores and the "Clear Profile" option -->
> **[Screenshot: Interest Profile]** *Your interest profile showing top categories, interaction scores, and the reset option.*

After about 5 interactions, the "For You" filter activates on the dashboard. It's not an algorithm deciding what you see — it's a mirror reflecting your own choices back at you. And you can reset it anytime.

---

## The Little Things

A few UX details I'm proud of:

- **Dark mode** that actually works — full AMOLED-friendly dark theme with a toggle in the sidebar
- **Stale post cleanup** — old posts auto-purge so the database stays lean
- **Reading reminders** — schedule a daily nudge with native macOS notifications
- **Toast notifications** — non-intrusive feedback for every action
- **Post deduplication** — same article from multiple feeds? You'll only see it once

<!-- SCREENSHOT: The app in dark mode, showing the sidebar navigation and a board or all-posts view -->
> **[Screenshot: Dark Mode]** *Pyr Reader in dark mode — clean, focused, easy on the eyes.*

---

## Architecture: The Connector Pattern

Under the hood, each data source implements a common `Connector` trait in Rust:

```rust
#[async_trait]
pub trait Connector {
    async fn fetch_posts(&self) -> Result<Vec<Post>>;
}
```

This makes adding new sources straightforward. The RSS connector, Gmail connector, and future connectors (X/Twitter, LinkedIn) all follow the same pattern. Posts from every source share a unified `Post` struct and flow through the same classification and organization pipeline.

State is managed through a shared `AppState` behind an `Arc<Mutex<>>`, and all I/O is async via Tokio. The result is a backend that handles multiple concurrent fetches without blocking the UI thread.

---

## What's Next

Pyr Reader is a personal tool, but I'm actively building toward:

- **X (Twitter) integration** — using the official API v2
- **LinkedIn connector** — pending ToS review
- **Smarter classification** — fine-tuning prompts based on user corrections
- **Cross-board insights** — connecting related content across different topics

---

## Try It Yourself

Pyr Reader is built with Tauri 2, which means the entire app compiles to a lightweight native binary. If you're interested in building something similar, the stack is approachable:

```bash
# Clone and install
bun install

# Run in dev mode
bun run tauri:dev

# Build for production
bun run tauri:build
```

The connector pattern makes it easy to add your own data sources, and swapping between local (Ollama) and cloud AI means you can keep everything private or leverage the best models available.

---

*Pyr Reader is a personal project by Ricardo Trentin. Built with Rust, Tauri, and a healthy obsession with staying informed without losing your mind.*
