import { invoke } from '@tauri-apps/api/core';
import { open as shellOpen } from '@tauri-apps/plugin-shell';

// ============================================================
// State
// ============================================================
const state = {
  // Navigation
  currentView: 'dashboard',   // 'dashboard', 'board', 'all-posts', 'rss-feeds', 'classifier'
  activeBoardId: null,

  // Data
  boards: [],
  cards: {},            // boardId -> Card[]
  boardCardCounts: {},  // boardId -> count
  posts: [],
  postsOffset: 0,
  postsLimit: 40,
  postsHasMore: true,

  // Filters (All Posts view)
  postSearch: '',
  postSourceFilter: 'all', // 'all' | 'RSS'

  // RSS
  rssFeeds: [],
  rssFetchedPosts: [],
  autoOrganizeEnabled: false,
  excludedTopics: [],  // lowercase category names to skip during auto-organize
  autoFetchEnabled: false,
  autoFetchInterval: 30,  // minutes

  // Classifier
  classifierAvailable: false,
  classifierModels: [],
  classifierConfig: null,  // { provider, model, ollama_url, has_anthropic_key, has_openai_key }

  // Loading flags
  loading: {},
};

const AUTO_FETCH_INTERVALS = [
  { value: 15, label: '15 min' },
  { value: 30, label: '30 min' },
  { value: 60, label: '1 hour' },
  { value: 120, label: '2 hours' },
  { value: 240, label: '4 hours' },
];

const ORGANIZE_TOPICS = [
  'Technology', 'Politics', 'Science', 'Business', 'Entertainment',
  'Sports', 'Health', 'Education', 'Environment', 'Culture', 'Other',
];

const BOARD_THEMES = {
  'Technology':    { gradient: 'linear-gradient(135deg, #667eea, #764ba2)', emoji: '\u{1F4BB}' },
  'Politics':      { gradient: 'linear-gradient(135deg, #f093fb, #f5576c)', emoji: '\u{1F3DB}' },
  'Science':       { gradient: 'linear-gradient(135deg, #4facfe, #00f2fe)', emoji: '\u{1F52C}' },
  'Business':      { gradient: 'linear-gradient(135deg, #43e97b, #38f9d7)', emoji: '\u{1F4C8}' },
  'Entertainment': { gradient: 'linear-gradient(135deg, #fa709a, #fee140)', emoji: '\u{1F3AC}' },
  'Sports':        { gradient: 'linear-gradient(135deg, #a18cd1, #fbc2eb)', emoji: '\u{26BD}' },
  'Health':        { gradient: 'linear-gradient(135deg, #ffecd2, #fcb69f)', emoji: '\u{1F3E5}' },
  'Education':     { gradient: 'linear-gradient(135deg, #89f7fe, #66a6ff)', emoji: '\u{1F393}' },
  'Environment':   { gradient: 'linear-gradient(135deg, #a8edea, #fed6e3)', emoji: '\u{1F33F}' },
  'Culture':       { gradient: 'linear-gradient(135deg, #fccb90, #d57eeb)', emoji: '\u{1F3A8}' },
  'Other':         { gradient: 'linear-gradient(135deg, #c1c1c1, #8e8e8e)', emoji: '\u{1F4CC}' },
  '_default':      { gradient: 'linear-gradient(135deg, #667eea, #764ba2)', emoji: '\u{1F4CB}' },
};

// ============================================================
// TTS (Text-to-Speech) Manager
// ============================================================
const TTS_RATES = [0.75, 1, 1.25, 1.5, 2];

const tts = {
  currentCardId: null,
  rate: 1,

  play(text, cardId) {
    this.stop();
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.rate = this.rate;
    this.currentCardId = cardId;
    utterance.onend = () => this._reset(cardId);
    utterance.onerror = () => this._reset(cardId);
    window.speechSynthesis.speak(utterance);
  },

  stop() {
    window.speechSynthesis.cancel();
    if (this.currentCardId) {
      this._resetButton(this.currentCardId);
      this.currentCardId = null;
    }
  },

  cycleRate() {
    const idx = TTS_RATES.indexOf(this.rate);
    this.rate = TTS_RATES[(idx + 1) % TTS_RATES.length];
    // Update all visible speed buttons
    document.querySelectorAll('[data-tts-speed]').forEach(btn => {
      btn.textContent = this.rate + 'x';
    });
    return this.rate;
  },

  isPlaying(cardId) {
    return this.currentCardId === cardId && window.speechSynthesis.speaking;
  },

  _reset(cardId) {
    if (this.currentCardId === cardId) {
      this._resetButton(cardId);
      this.currentCardId = null;
    }
  },

  _resetButton(cardId) {
    const btn = document.querySelector(`[data-play-card="${cardId}"]`)
      || (cardId === 'modal-enrichment' ? document.querySelector('[data-play-enrichment]') : null);
    if (btn) btn.textContent = '\u25B6 Play';
  },
};

// ============================================================
// Theme Manager
// ============================================================

const theme = {
  // 'light' | 'dark' | 'system'
  current: 'system',

  init() {
    // Sync the system-preference class on <html>
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const syncSystemClass = () => {
      document.documentElement.classList.toggle('prefers-dark', mq.matches);
    };
    syncSystemClass();
    mq.addEventListener('change', syncSystemClass);

    // Default to system
    document.documentElement.setAttribute('data-theme', 'system');
  },

  apply(mode) {
    this.current = mode;
    document.documentElement.setAttribute('data-theme', mode);
    this.updateToggleUI();
  },

  isDark() {
    if (this.current === 'dark') return true;
    if (this.current === 'light') return false;
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  },

  toggle() {
    // Simple toggle: if currently dark -> go light, if light -> go dark
    this.apply(this.isDark() ? 'light' : 'dark');
  },

  updateToggleUI() {
    const btn = document.getElementById('theme-toggle-btn');
    const icon = document.getElementById('theme-toggle-icon');
    const text = document.getElementById('theme-toggle-text');
    if (!btn) return;
    const dark = this.isDark();
    btn.classList.toggle('toggle-active', dark);
    if (icon) icon.textContent = dark ? '\u2600' : '\u263D';
    if (text) text.textContent = 'Dark Mode';
  },
};

// ============================================================
// Helpers
// ============================================================

/** Show a toast notification. type: 'success' | 'error' | 'info' */
function toast(message, type = 'info') {
  const container = document.getElementById('toast-container');
  const el = document.createElement('div');
  el.className = `toast toast-${type}`;
  el.textContent = message;
  container.appendChild(el);
  setTimeout(() => {
    el.classList.add('removing');
    el.addEventListener('animationend', () => el.remove());
  }, 3500);
}

/** Format a timestamp string to relative time ("2h ago", "yesterday", etc.) */
function relativeTime(ts) {
  if (!ts) return '';
  const date = new Date(ts);
  const now = new Date();
  const diffMs = now - date;
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);

  if (diffSec < 60) return 'just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHr < 24) return `${diffHr}h ago`;
  if (diffDay === 1) return 'yesterday';
  if (diffDay < 7) return `${diffDay}d ago`;
  if (diffDay < 30) return `${Math.floor(diffDay / 7)}w ago`;
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: date.getFullYear() !== now.getFullYear() ? 'numeric' : undefined });
}

/** Escape HTML to prevent XSS */
function esc(str) {
  if (str == null) return '';
  const div = document.createElement('div');
  div.textContent = String(str);
  return div.innerHTML;
}

/** Build a source badge HTML string */
function sourceBadge(source) {
  if (source === 'RSS') {
    return '<span class="card-source-badge badge-rss">RSS</span>';
  }
  return `<span class="card-source-badge">${esc(source)}</span>`;
}

/** Build a sentiment badge HTML string */
function sentimentBadge(sentiment) {
  if (!sentiment) return '';
  const lower = sentiment.toLowerCase();
  let cls = 'sentiment-neutral';
  if (lower === 'positive') cls = 'sentiment-positive';
  else if (lower === 'negative') cls = 'sentiment-negative';
  return `<span class="sentiment-badge ${cls}">${esc(sentiment)}</span>`;
}

/** Set a loading flag and optionally show spinner in a target element */
function setLoading(key, value) {
  state.loading[key] = value;
}

function isLoading(key) {
  return !!state.loading[key];
}

/** Render a loading spinner HTML */
function spinnerHTML(large = false) {
  return `<div class="loading-state"><div class="spinner ${large ? 'spinner-large' : ''}"></div><span>Loading...</span></div>`;
}

/** App mascot icon (uses app-icon.png) */
function mascotSVG(size = 80) {
  return `<img src="/src/app-icon.png" width="${size}" height="${size}" alt="Pyr Reader" style="border-radius: 50%; opacity: 0.7;">`;
}

/** Render an empty state HTML */
function emptyStateHTML(icon, title, description) {
  return `
    <div class="empty-state">
      <div class="empty-state-icon">${mascotSVG(72)}</div>
      <h3>${esc(title)}</h3>
      <p>${esc(description)}</p>
    </div>`;
}

// ============================================================
// Data Fetching
// ============================================================

async function loadBoards() {
  try {
    state.boards = await invoke('get_boards');
  } catch (e) {
    console.error('Failed to load boards:', e);
    toast('Failed to load boards', 'error');
    state.boards = [];
  }
  renderSidebarBoards();
}

async function loadCardsForBoard(boardId) {
  setLoading('cards', true);
  renderMainContent();
  try {
    state.cards[boardId] = await invoke('get_cards_by_board', { boardId });
  } catch (e) {
    console.error('Failed to load cards:', e);
    toast('Failed to load cards', 'error');
    state.cards[boardId] = [];
  }
  setLoading('cards', false);
  renderMainContent();
}

async function loadPosts(reset = false) {
  if (reset) {
    state.postsOffset = 0;
    state.posts = [];
    state.postsHasMore = true;
  }
  setLoading('posts', true);
  renderMainContent();
  try {
    const fetched = await invoke('get_posts', { limit: state.postsLimit, offset: state.postsOffset });
    if (reset) {
      state.posts = fetched;
    } else {
      state.posts = state.posts.concat(fetched);
    }
    state.postsHasMore = fetched.length >= state.postsLimit;
    state.postsOffset += fetched.length;
  } catch (e) {
    console.error('Failed to load posts:', e);
    toast('Failed to load posts', 'error');
  }
  setLoading('posts', false);
  renderMainContent();
}

async function loadRssFeeds() {
  try {
    state.rssFeeds = await invoke('list_rss_feeds');
  } catch (e) {
    console.error('Failed to load RSS feeds:', e);
    state.rssFeeds = [];
  }
}

async function loadBoardCardCounts() {
  try {
    state.boardCardCounts = await invoke('get_board_card_counts');
  } catch (e) {
    console.error('Failed to load card counts:', e);
    state.boardCardCounts = {};
  }
  if (state.currentView === 'dashboard') {
    renderMainContent();
  }
}

async function checkClassifier() {
  try {
    state.classifierConfig = await invoke('classifier_get_config');
  } catch (e) {
    state.classifierConfig = null;
  }
  try {
    state.classifierAvailable = await invoke('classifier_is_available');
  } catch (e) {
    state.classifierAvailable = false;
  }
  try {
    state.classifierModels = await invoke('classifier_list_models');
  } catch (e) {
    state.classifierModels = [];
  }
}

// ============================================================
// Navigation
// ============================================================

function navigateTo(view, boardId = null) {
  state.currentView = view;
  state.activeBoardId = boardId;
  updateActiveNav();
  renderMainContent();

  // Load data for view
  if (view === 'dashboard') {
    loadBoardCardCounts();
  } else if (view === 'board' && boardId) {
    if (!state.cards[boardId]) {
      loadCardsForBoard(boardId);
    }
  } else if (view === 'all-posts') {
    if (state.posts.length === 0) {
      loadPosts(true);
    }
  } else if (view === 'rss-feeds') {
    loadRssFeeds().then(() => renderMainContent());
  } else if (view === 'classifier') {
    checkClassifier().then(() => renderMainContent());
  }
}

function updateActiveNav() {
  // Clear all active states
  document.querySelectorAll('.nav-item').forEach(el => el.classList.remove('active'));

  if (state.currentView === 'board' && state.activeBoardId) {
    const boardBtn = document.querySelector(`.nav-item[data-board-id="${state.activeBoardId}"]`);
    if (boardBtn) boardBtn.classList.add('active');
  } else {
    const viewBtn = document.querySelector(`.nav-item[data-view="${state.currentView}"]`);
    if (viewBtn) viewBtn.classList.add('active');
  }
}

// ============================================================
// Sidebar Rendering
// ============================================================

function renderSidebarBoards() {
  const list = document.getElementById('boards-list');
  if (!list) return;

  if (state.boards.length === 0) {
    list.innerHTML = '<li class="text-secondary text-small" style="padding:4px 8px;">No boards yet</li>';
    return;
  }

  list.innerHTML = state.boards.map(b => `
    <li>
      <div class="nav-item-board">
        <button class="nav-item" data-board-id="${esc(b.id)}" data-view="board">
          <span class="nav-icon">&gt;</span> ${esc(b.name)}
        </button>
        <button class="board-delete-btn" data-delete-board="${esc(b.id)}" title="Delete board">&times;</button>
      </div>
    </li>
  `).join('');

  updateActiveNav();
}

// ============================================================
// Main Content Rendering
// ============================================================

function renderMainContent() {
  const main = document.getElementById('main-content');
  switch (state.currentView) {
    case 'dashboard':
      renderDashboardView(main);
      break;
    case 'board':
      renderBoardView(main);
      break;
    case 'all-posts':
      renderAllPostsView(main);
      break;
    case 'rss-feeds':
      renderRssFeedsView(main);
      break;
    case 'classifier':
      renderClassifierView(main);
      break;
    default:
      renderDashboardView(main);
  }
}

// ---- Dashboard View ----

function getBoardTheme(boardName) {
  return BOARD_THEMES[boardName] || BOARD_THEMES['_default'];
}

function renderDashboardView(container) {
  let content = `
    <div class="view-header">
      <h2>Dashboard</h2>
      <p>${state.boards.length} board${state.boards.length !== 1 ? 's' : ''}</p>
    </div>
    <div class="view-body">`;

  if (state.boards.length === 0) {
    content += `
      <div class="empty-state">
        <div class="empty-state-icon">${mascotSVG(72)}</div>
        <h3>No boards yet</h3>
        <p>Create a board from the sidebar, or enable auto-organize in RSS Feeds to get started.</p>
      </div>`;
  } else {
    content += '<div class="dashboard-grid">';
    for (const board of state.boards) {
      const theme = getBoardTheme(board.name);
      const count = state.boardCardCounts[board.id] || 0;
      const desc = board.description || '';
      content += `
        <div class="dashboard-card" data-board-id="${esc(board.id)}" data-view="board">
          <div class="dashboard-card-header" style="background: ${theme.gradient}">
            <div class="dashboard-card-mascot">
              <img src="/src/app-icon.png" width="64" height="64" alt="" class="dashboard-card-mascot-img">
              <span class="dashboard-card-mascot-badge">${theme.emoji}</span>
            </div>
          </div>
          <div class="dashboard-card-body">
            <div class="dashboard-card-name">${esc(board.name)}</div>
            ${desc ? `<div class="dashboard-card-desc">${esc(desc)}</div>` : ''}
            <div class="dashboard-card-count">${count} card${count !== 1 ? 's' : ''}</div>
          </div>
        </div>`;
    }
    content += '</div>';
  }

  content += '</div>';
  container.innerHTML = content;
}

// ---- Board View ----

function renderBoardView(container) {
  const board = state.boards.find(b => b.id === state.activeBoardId);
  if (!board) {
    container.innerHTML = emptyStateHTML('&#9633;', 'Board not found', 'Select a board from the sidebar.');
    return;
  }

  const cards = state.cards[board.id] || [];

  let content = `
    <div class="view-header">
      <h2>${esc(board.name)}</h2>
      ${board.description ? `<p>${esc(board.description)}</p>` : ''}
    </div>
    <div class="view-body">`;

  if (isLoading('cards')) {
    content += spinnerHTML(true);
  } else if (cards.length === 0) {
    content += emptyStateHTML('&#9776;', 'No cards yet', 'Add posts to this board from the All Posts view or feed views.');
  } else {
    content += '<div class="cards-grid">';
    for (const card of cards) {
      content += renderCardHTML(card, board.id);
    }
    content += '</div>';
  }

  content += '</div>';
  container.innerHTML = content;
}

function renderCardHTML(card) {
  const tagsHTML = (card.tags || []).map(t => `<span class="tag">${esc(t)}</span>`).join('');
  const isSaved = !!card.saved;
  const starIcon = isSaved ? '&#9733;' : '&#9734;';
  const savedClass = isSaved ? 'card-saved' : '';
  const unsavedClass = isSaved ? '' : 'card-unsaved';
  return `
    <div class="card ${unsavedClass}" data-card-id="${esc(card.id)}" data-post-id="${esc(card.post_id)}">
      <div class="card-header">
        <button class="card-save-btn ${savedClass}" data-toggle-save="${esc(card.id)}" data-currently-saved="${isSaved}" title="${isSaved ? 'Saved' : 'Click to save'}">${starIcon}</button>
        <span class="card-timestamp">${relativeTime(card.created_at)}</span>
      </div>
      ${card.summary ? `<div class="card-summary">${esc(card.summary)}</div>` : ''}
      <div class="card-content" data-post-id="${esc(card.post_id)}">Loading post...</div>
      ${tagsHTML ? `<div class="card-tags">${tagsHTML}</div>` : ''}
      <div class="card-enrichment" data-post-id="${esc(card.post_id)}"></div>
      <div class="card-footer">
        <button class="card-action-btn card-play-btn" data-play-card="${esc(card.id)}">&#9654; Play</button>
        <button class="card-action-btn card-speed-btn" data-tts-speed>${tts.rate}x</button>
        <button class="card-action-btn" data-delete-card="${esc(card.id)}" data-board-id="${esc(card.board_id)}">Remove</button>
      </div>
    </div>`;
}

// Lazy-load post content into cards after render
async function hydrateCardPosts() {
  const cardEls = document.querySelectorAll('.card[data-post-id]');
  for (const el of cardEls) {
    const postId = el.dataset.postId;
    const contentEl = el.querySelector('.card-content[data-post-id]');
    if (!contentEl || contentEl.dataset.loaded) continue;
    contentEl.dataset.loaded = 'true';
    try {
      const post = await invoke('get_post_by_id', { id: postId });
      if (post) {
        // Update badge in header
        const header = el.querySelector('.card-header');
        if (header && !header.querySelector('.card-source-badge')) {
          header.insertAdjacentHTML('afterbegin', sourceBadge(post.source));
        }
        contentEl.textContent = post.content || '(no content)';
        // update author
        const authorEl = document.createElement('div');
        authorEl.className = 'card-author';
        authorEl.textContent = post.author || 'Unknown';
        contentEl.before(authorEl);
      }
    } catch (e) {
      contentEl.textContent = '(post not found)';
    }
  }
}

// Render enrichment HTML for a card
function renderEnrichmentHTML(enrichment) {
  const sourcesHTML = enrichment.sources.map((s, i) => `
    <li>
      <span class="enrichment-source-num">[${i + 1}]</span>
      <a class="enrichment-source-link" href="${esc(s.url)}" target="_blank" rel="noopener noreferrer" title="${esc(s.title)}">${esc(s.title)}</a>
      ${s.snippet ? `<div class="enrichment-source-snippet">${esc(s.snippet)}</div>` : ''}
    </li>
  `).join('');

  const queriesText = enrichment.search_queries.map(q => `"${esc(q)}"`).join(', ');

  return `
    <div class="enrichment-section">
      <button class="enrichment-toggle" data-toggle-enrichment>
        <span>Research Insights</span>
        <span class="collapsible-chevron">&#9660;</span>
      </button>
      <div class="enrichment-body">
        <div class="enrichment-synthesis">${esc(enrichment.synthesis)}</div>
        ${enrichment.sources.length > 0 ? `
          <div class="enrichment-references-heading">References</div>
          <ul class="enrichment-sources">${sourcesHTML}</ul>
        ` : ''}
        <div class="enrichment-meta">
          Researched ${relativeTime(enrichment.created_at)} &middot; Queries: ${queriesText}
        </div>
      </div>
    </div>`;
}

// Render enrichment HTML for the post detail modal (with Copy button)
function renderModalEnrichmentHTML(enrichment) {
  lastModalEnrichment = enrichment;

  const sourcesHTML = enrichment.sources.map((s, i) => `
    <li>
      <span class="enrichment-source-num">[${i + 1}]</span>
      <a class="enrichment-source-link" href="${esc(s.url)}" target="_blank" rel="noopener noreferrer" title="${esc(s.title)}">${esc(s.title)}</a>
      ${s.snippet ? `<div class="enrichment-source-snippet">${esc(s.snippet)}</div>` : ''}
    </li>
  `).join('');

  const queriesText = enrichment.search_queries.map(q => `"${esc(q)}"`).join(', ');

  return `
    <div class="post-detail-section">
      <div class="post-detail-section-header">
        <h4>Research Insights</h4>
        <div class="enrichment-actions">
          <button class="btn btn-secondary btn-small" data-play-enrichment>&#9654; Play</button>
          <button class="btn btn-secondary btn-small" data-tts-speed>${tts.rate}x</button>
          <button class="btn btn-secondary btn-small" data-save-enrichment>Save .md</button>
          <button class="btn btn-secondary btn-small" data-copy-text="enrichment-synthesis-text">Copy</button>
        </div>
      </div>
      <div class="enrichment-synthesis" id="enrichment-synthesis-text">${esc(enrichment.synthesis)}</div>
      ${enrichment.sources.length > 0 ? `
        <div class="enrichment-references-heading">References</div>
        <ul class="enrichment-sources">${sourcesHTML}</ul>
      ` : ''}
      <div class="enrichment-meta">
        Researched ${relativeTime(enrichment.created_at)} &middot; Queries: ${queriesText}
      </div>
    </div>`;
}

/** Convert an enrichment object to Markdown */
function enrichmentToMarkdown(enrichment) {
  let md = `# Research Insights\n\n${enrichment.synthesis}\n`;
  if (enrichment.sources.length > 0) {
    md += '\n## References\n\n';
    enrichment.sources.forEach((s, i) => {
      md += `${i + 1}. [${s.title}](${s.url})`;
      if (s.snippet) md += `\n   > ${s.snippet}`;
      md += '\n';
    });
  }
  if (enrichment.search_queries.length > 0) {
    md += `\n---\n*Search queries: ${enrichment.search_queries.map(q => `"${q}"`).join(', ')}*\n`;
  }
  return md;
}

/** Trigger a browser download for the given content */
function downloadFile(content, filename, mimeType = 'text/markdown') {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

// Lazy-load cached enrichments into cards after render
async function hydrateCardEnrichments() {
  const enrichmentEls = document.querySelectorAll('.card-enrichment[data-post-id]');
  for (const el of enrichmentEls) {
    if (el.dataset.loaded) continue;
    el.dataset.loaded = 'true';
    const postId = el.dataset.postId;
    try {
      const enrichment = await invoke('get_enrichment', { postId });
      if (enrichment) {
        el.innerHTML = renderEnrichmentHTML(enrichment);
      }
    } catch (e) {
      // Silently skip — no cached enrichment
    }
  }
}

// ---- All Posts View ----

function renderAllPostsView(container) {
  const filtered = getFilteredPosts();

  let content = `
    <div class="view-header">
      <h2>All Posts</h2>
      <p>${state.posts.length} posts loaded</p>
      <div class="view-header-actions">
        <div class="search-bar">
          <input type="text" id="post-search" placeholder="Search posts..." value="${esc(state.postSearch)}">
        </div>
        <div class="filter-bar">
          <button class="filter-btn ${state.postSourceFilter === 'all' ? 'active' : ''}" data-filter="all">All</button>
          <button class="filter-btn ${state.postSourceFilter === 'RSS' ? 'active' : ''}" data-filter="RSS">RSS</button>
        </div>
        <button class="btn btn-secondary btn-small" id="refresh-posts-btn">Refresh</button>
      </div>
    </div>
    <div class="view-body">`;

  if (isLoading('posts') && state.posts.length === 0) {
    content += spinnerHTML(true);
  } else if (filtered.length === 0) {
    content += emptyStateHTML('&#9783;', 'No posts found', state.postSearch ? 'Try a different search term.' : 'Fetch posts from RSS feeds or X/Twitter to get started.');
  } else {
    content += '<div class="posts-list">';
    for (const post of filtered) {
      content += renderPostRowHTML(post);
    }
    content += '</div>';

    if (state.postsHasMore && !state.postSearch && state.postSourceFilter === 'all') {
      content += `
        <div class="load-more-bar">
          <button class="btn btn-secondary btn-small" id="load-more-posts">${isLoading('posts') ? '<span class="spinner"></span>' : 'Load More'}</button>
        </div>`;
    }
  }

  content += '</div>';
  container.innerHTML = content;
}

function getFilteredPosts() {
  let posts = state.posts;
  if (state.postSourceFilter !== 'all') {
    posts = posts.filter(p => p.source === state.postSourceFilter);
  }
  if (state.postSearch) {
    const q = state.postSearch.toLowerCase();
    posts = posts.filter(p =>
      (p.content && p.content.toLowerCase().includes(q)) ||
      (p.author && p.author.toLowerCase().includes(q))
    );
  }
  return posts;
}

function renderPostRowHTML(post) {
  return `
    <div class="post-row" data-post-id="${esc(post.id)}">
      <div class="post-row-badge">${sourceBadge(post.source)}</div>
      <div class="post-row-body">
        <div class="post-row-meta">
          <span class="post-row-author">${esc(post.author || 'Unknown')}</span>
          <span class="post-row-time">${relativeTime(post.timestamp)}</span>
        </div>
        <div class="post-row-content">${esc(post.content || '(no content)')}</div>
      </div>
      <div class="post-row-actions">
        <button class="btn btn-primary btn-small" data-add-post-to-board="${esc(post.id)}">Save</button>
      </div>
    </div>`;
}

// ---- RSS Feeds View ----

function renderRssFeedsView(container) {
  let feedsListHTML = '';
  if (state.rssFeeds.length === 0) {
    feedsListHTML = '<p class="text-secondary mt-8">No feeds configured yet.</p>';
  } else {
    feedsListHTML = '<div class="feed-list">';
    for (const url of state.rssFeeds) {
      feedsListHTML += `
        <div class="feed-item">
          <span class="feed-item-url">${esc(url)}</span>
          <button class="btn btn-danger btn-small" data-remove-feed="${esc(url)}">Remove</button>
        </div>`;
    }
    feedsListHTML += '</div>';
  }

  let fetchedHTML = '';
  if (state.rssFetchedPosts.length > 0) {
    fetchedHTML = `
      <div class="inline-results mt-24">
        <h3>Fetched Posts (${state.rssFetchedPosts.length})</h3>
        <div class="posts-list">
          ${state.rssFetchedPosts.map(p => renderPostRowHTML(p)).join('')}
        </div>
      </div>`;
  }

  const toggleActive = state.autoOrganizeEnabled ? 'toggle-active' : '';
  const fetchLabel = isLoading('rss-fetch')
    ? '<span class="spinner"></span> ' + (state.autoOrganizeEnabled ? 'Fetching & Organizing...' : 'Fetching...')
    : (state.autoOrganizeEnabled ? 'Fetch & Organize' : 'Fetch Now');

  let autoOrganizeWarning = '';
  if (state.autoOrganizeEnabled && !state.classifierAvailable) {
    autoOrganizeWarning = '<p class="text-secondary mt-4" style="color: var(--color-warning);">Classifier is unavailable. Posts will be fetched but not organized.</p>';
  }

  container.innerHTML = `
    <div class="view-header">
      <h2>RSS Feeds</h2>
      <p>Manage your RSS and Atom feed subscriptions</p>
    </div>
    <div class="view-body">
      <div class="settings-section">
        <div class="settings-section-title">Add Feed</div>
        <div class="form-inline">
          <input type="url" id="rss-url-input" placeholder="https://example.com/feed.xml">
          <button class="btn btn-primary" id="add-rss-btn">Add Feed</button>
        </div>
      </div>

      <div class="settings-section">
        <div class="settings-section-title">Current Feeds</div>
        ${feedsListHTML}
      </div>

      <div class="settings-section">
        <div class="settings-section-title">Auto-Organize</div>
        <div class="settings-row">
          <div>
            <label>Automatically classify and organize fetched posts into boards</label>
            <p class="text-secondary text-small mt-4">When enabled, each fetched post will be classified via the configured LLM and placed into category boards.</p>
            ${autoOrganizeWarning}
          </div>
          <button class="toggle-btn ${toggleActive}" id="auto-organize-toggle">
            <span class="toggle-knob"></span>
          </button>
        </div>
        ${state.autoOrganizeEnabled ? `
        <div class="mt-16">
          <label class="text-small text-secondary mb-8" style="display:block;">Topics — click to disable categories you don't want</label>
          <div class="topic-grid">
            ${ORGANIZE_TOPICS.map(t => {
              const excluded = state.excludedTopics.includes(t.toLowerCase());
              return `<button class="topic-item ${excluded ? 'topic-item-excluded' : ''}" data-toggle-topic="${esc(t)}">${esc(t)}</button>`;
            }).join('')}
          </div>
        </div>` : ''}
      </div>

      <div class="settings-section">
        <div class="settings-section-title">Scheduled Fetch</div>
        <div class="settings-row">
          <div>
            <label>Automatically fetch new posts on a timer</label>
            <p class="text-secondary text-small mt-4">When enabled, RSS feeds will be fetched at the selected interval.</p>
          </div>
          <button class="toggle-btn ${state.autoFetchEnabled ? 'toggle-active' : ''}" id="auto-fetch-toggle">
            <span class="toggle-knob"></span>
          </button>
        </div>
        ${state.autoFetchEnabled ? `
        <div class="mt-12">
          <div class="interval-selector">
            ${AUTO_FETCH_INTERVALS.map(i =>
              `<button class="interval-btn ${state.autoFetchInterval === i.value ? 'interval-btn-active' : ''}" data-set-interval="${i.value}">${esc(i.label)}</button>`
            ).join('')}
          </div>
          ${autoFetchNextAt ? `<p class="text-secondary text-small mt-8 auto-fetch-countdown">${getNextFetchText()}</p>` : ''}
        </div>` : ''}
      </div>

      <div class="settings-section">
        <button class="btn btn-primary" id="fetch-rss-btn">${fetchLabel}</button>
        ${state.autoFetchEnabled && autoFetchNextAt ? `<span class="text-secondary text-small auto-fetch-countdown" style="margin-left:12px;">${getNextFetchText()}</span>` : ''}
      </div>

      ${fetchedHTML}
    </div>`;
}

// ---- Classifier View ----

function renderClassifierView(container) {
  const cfg = state.classifierConfig || { provider: 'ollama', model: '', ollama_url: '', has_anthropic_key: false, has_openai_key: false, has_tavily_key: false };
  const currentProvider = cfg.provider;

  const statusHTML = state.classifierAvailable
    ? '<span class="status-indicator"><span class="status-dot connected"></span> Available</span>'
    : '<span class="status-indicator"><span class="status-dot disconnected"></span> Unavailable</span>';

  // Provider cards
  const providers = [
    { id: 'ollama', label: 'Ollama', desc: 'Local models via Ollama' },
    { id: 'anthropic', label: 'Anthropic', desc: 'Claude API' },
    { id: 'openai', label: 'OpenAI', desc: 'GPT models' },
  ];

  const providerCardsHTML = providers.map(p => `
    <button class="provider-card ${currentProvider === p.id ? 'provider-card-active' : ''}" data-select-provider="${p.id}">
      <div class="provider-card-label">${esc(p.label)}</div>
      <div class="provider-card-desc">${esc(p.desc)}</div>
    </button>
  `).join('');

  // API key section (for Anthropic / OpenAI only)
  let apiKeyHTML = '';
  if (currentProvider === 'anthropic' || currentProvider === 'openai') {
    const hasKey = currentProvider === 'anthropic' ? cfg.has_anthropic_key : cfg.has_openai_key;
    const providerLabel = currentProvider === 'anthropic' ? 'Anthropic' : 'OpenAI';
    apiKeyHTML = `
      <div class="settings-section">
        <div class="settings-section-title">${esc(providerLabel)} API Key</div>
        ${hasKey ? '<p class="text-secondary mb-8">API key is set.</p>' : '<p class="text-secondary mb-8">No API key configured yet.</p>'}
        <div class="form-inline">
          <input type="password" id="classifier-api-key-input" placeholder="Enter ${esc(providerLabel)} API key">
          <button class="btn btn-primary" id="save-api-key-btn">Save Key</button>
        </div>
      </div>`;
  }

  // Models
  let modelsHTML = '';
  if (state.classifierModels.length > 0) {
    modelsHTML = `
      <div class="model-list">
        ${state.classifierModels.map(m => `<button class="model-tag ${cfg.model === m ? 'model-tag-selected' : ''}" data-select-model="${esc(m)}">${esc(m)}</button>`).join('')}
      </div>`;
  } else {
    const hint = currentProvider === 'ollama'
      ? 'No models found. Make sure Ollama is running.'
      : currentProvider === 'openai'
        ? (cfg.has_openai_key ? 'No models found. Try refreshing.' : 'Set an API key to see available models.')
        : '';
    if (hint) {
      modelsHTML = `<p class="text-secondary mt-8">${esc(hint)}</p>`;
    }
  }

  container.innerHTML = `
    <div class="view-header">
      <h2>Classifier</h2>
      <p>AI-powered content classification, summarization, and derivative generation</p>
      <div class="view-header-actions">
        ${statusHTML}
      </div>
    </div>
    <div class="view-body">
      <div class="settings-section">
        <div class="settings-section-title">Provider</div>
        <div class="provider-cards">
          ${providerCardsHTML}
        </div>
      </div>

      ${apiKeyHTML}

      <div class="settings-section">
        <div class="settings-section-title">Models</div>
        ${modelsHTML}
        <div class="mt-12">
          <button class="btn btn-secondary" id="refresh-classifier-btn">${isLoading('classifier') ? '<span class="spinner"></span>' : 'Refresh'}</button>
        </div>
      </div>

      <div class="settings-section">
        <div class="settings-section-title">Tavily API Key (Web Search for Learn Mode)</div>
        ${cfg.has_tavily_key ? '<p class="text-secondary mb-8">Tavily API key is set.</p>' : '<p class="text-secondary mb-8">No Tavily API key configured. Required for the Learn feature on cards.</p>'}
        <div class="form-inline">
          <input type="password" id="tavily-api-key-input" placeholder="Enter Tavily API key">
          <button class="btn btn-primary" id="save-tavily-key-btn">Save Key</button>
        </div>
      </div>
    </div>`;
}

// ============================================================
// Post Detail Modal
// ============================================================

async function showPostDetail(postId) {
  const overlay = document.getElementById('modal-overlay');
  const body = document.getElementById('modal-body');

  body.innerHTML = spinnerHTML();
  overlay.hidden = false;

  let post;
  try {
    post = await invoke('get_post_by_id', { id: postId });
  } catch (e) {
    body.innerHTML = '<p>Failed to load post.</p>';
    return;
  }

  if (!post) {
    body.innerHTML = '<p>Post not found.</p>';
    return;
  }

  const urlHost = post.url ? (() => { try { return new URL(post.url).hostname.replace(/^www\./, ''); } catch { return ''; } })() : '';

  body.innerHTML = `
    <div class="post-reader">
      <div class="post-reader-meta">
        ${sourceBadge(post.source)}
        <span class="post-reader-author">${esc(post.author || 'Unknown')}</span>
        <span class="post-reader-sep">&middot;</span>
        <span class="post-reader-time">${relativeTime(post.timestamp)}${post.timestamp ? ' &mdash; ' + new Date(post.timestamp).toLocaleString() : ''}</span>
      </div>
      <div class="post-reader-content">${esc(post.content)}</div>
      ${post.url ? `<a class="post-reader-source-link" href="${esc(post.url)}" target="_blank" rel="noopener noreferrer">${esc(urlHost || post.url)}</a>` : ''}
      <div class="post-detail-actions">
        <button class="btn btn-primary btn-small" data-modal-add-to-board="${esc(post.id)}">Add to Board</button>
        <button class="btn btn-secondary btn-small" data-modal-classify="${esc(post.id)}">Classify</button>
        <button class="btn btn-secondary btn-small" data-modal-summarize="${esc(post.id)}">Summarize</button>
        <button class="btn btn-secondary btn-small" data-modal-derivative="${esc(post.id)}">Generate Derivative</button>
        <button class="btn btn-secondary btn-small" data-modal-learn="${esc(post.id)}" id="modal-learn-btn">Learn</button>
      </div>
      <div id="post-detail-extras"></div>
    </div>
  `;

  // Load cached enrichment if available
  try {
    const cached = await invoke('get_enrichment', { postId: post.id });
    if (cached) {
      const extras = document.getElementById('post-detail-extras');
      extras.innerHTML = renderModalEnrichmentHTML(cached);
      const learnBtn = document.getElementById('modal-learn-btn');
      if (learnBtn) {
        learnBtn.textContent = 'Learned';
        learnBtn.classList.add('btn-learn-done');
      }
    }
  } catch (e) {
    // No cached enrichment — that's fine
  }
}

function closeModal(overlayId) {
  const overlay = document.getElementById(overlayId);
  if (overlay) overlay.hidden = true;
}

// ============================================================
// Add to Board Dialog
// ============================================================

async function showAddToBoardDialog(postId) {
  const overlay = document.getElementById('add-to-board-overlay');
  const content = document.getElementById('add-to-board-content');

  // Make sure boards are loaded
  if (state.boards.length === 0) {
    await loadBoards();
  }

  if (state.boards.length === 0) {
    content.innerHTML = '<p class="text-secondary">No boards available. Create a board first.</p>';
    overlay.hidden = false;
    return;
  }

  content.innerHTML = `
    <p class="text-secondary mb-12">Select a board to add this post to:</p>
    <div class="board-select-list">
      ${state.boards.map(b => `
        <div class="board-select-item" data-select-board="${esc(b.id)}" data-post-id="${esc(postId)}">
          <div>
            <div class="board-select-name">${esc(b.name)}</div>
            ${b.description ? `<div class="board-select-desc">${esc(b.description)}</div>` : ''}
          </div>
        </div>
      `).join('')}
    </div>
    <div class="add-to-board-fields" id="add-to-board-fields" style="display:none;">
      <div class="form-group">
        <label for="card-summary-input">Summary (optional)</label>
        <textarea id="card-summary-input" rows="2" placeholder="Brief summary of this post"></textarea>
      </div>
      <div class="form-group">
        <label for="card-tags-input">Tags (comma-separated, optional)</label>
        <input type="text" id="card-tags-input" placeholder="e.g. tech, news, ai">
      </div>
      <div class="form-actions">
        <button class="btn btn-secondary" id="cancel-add-card">Cancel</button>
        <button class="btn btn-primary" id="confirm-add-card">Add Card</button>
      </div>
    </div>
  `;

  overlay.hidden = false;
}

let pendingCard = { boardId: null, postId: null };
let lastModalEnrichment = null;
let autoFetchTimerId = null;
let autoFetchNextAt = null;

// ============================================================
// RSS Auto-Fetch Scheduler
// ============================================================

async function performRssFetch({ silent = false } = {}) {
  if (isLoading('rss-fetch')) return;
  setLoading('rss-fetch', true);
  if (!silent) renderMainContent();
  try {
    state.rssFetchedPosts = await invoke('fetch_rss_posts');
    toast(`Fetched ${state.rssFetchedPosts.length} posts from RSS feeds`, 'success');
  } catch (err) {
    toast(`RSS fetch failed: ${err}`, 'error');
    state.rssFetchedPosts = [];
  }

  // Auto-organize if toggle is on and posts were fetched.
  if (state.autoOrganizeEnabled && state.rssFetchedPosts.length > 0) {
    try {
      const postIds = state.rssFetchedPosts.map(p => p.id);
      const result = await invoke('auto_organize_posts', { postIds, excludedCategories: state.excludedTopics });
      let msg = `Organized ${result.organized}/${result.total} posts.`;
      if (result.boards_created.length > 0) {
        msg += ` New boards: ${result.boards_created.join(', ')}`;
      }
      toast(msg, 'success');
      if (result.failed.length > 0) {
        toast(`${result.failed.length} post(s) failed to organize`, 'error');
      }
      await loadBoards();
      state.cards = {};
    } catch (err) {
      toast(`Auto-organize failed: ${err}`, 'error');
    }
  }

  setLoading('rss-fetch', false);
  if (!silent) renderMainContent();

  // Reset next-fetch countdown if scheduler is active.
  if (state.autoFetchEnabled && autoFetchTimerId != null) {
    autoFetchNextAt = Date.now() + state.autoFetchInterval * 60 * 1000;
  }
}

function startAutoFetchScheduler() {
  stopAutoFetchScheduler();
  if (!state.autoFetchEnabled) return;
  const intervalMs = state.autoFetchInterval * 60 * 1000;
  autoFetchNextAt = Date.now() + intervalMs;
  autoFetchTimerId = setInterval(() => {
    performRssFetch({ silent: state.currentView !== 'rss-feeds' });
  }, intervalMs);
}

function stopAutoFetchScheduler() {
  if (autoFetchTimerId != null) {
    clearInterval(autoFetchTimerId);
    autoFetchTimerId = null;
  }
  autoFetchNextAt = null;
}

function restartAutoFetchScheduler() {
  stopAutoFetchScheduler();
  if (state.autoFetchEnabled) startAutoFetchScheduler();
}

function getNextFetchText() {
  if (!autoFetchNextAt) return '';
  const remaining = Math.max(0, autoFetchNextAt - Date.now());
  const mins = Math.ceil(remaining / 60000);
  if (mins <= 0) return 'fetching soon...';
  if (mins === 1) return 'next fetch in 1m';
  return `next fetch in ${mins}m`;
}

// ============================================================
// Event Handlers
// ============================================================

function setupEventListeners() {
  const sidebar = document.getElementById('sidebar');
  const main = document.getElementById('main-content');

  // ---- Sidebar clicks (event delegation) ----
  sidebar.addEventListener('click', async (e) => {
    const target = e.target.closest('[data-view], [data-board-id], [data-delete-board], #new-board-btn');
    if (!target) return;

    // New board button
    if (target.id === 'new-board-btn') {
      document.getElementById('new-board-overlay').hidden = false;
      document.getElementById('board-name-input').focus();
      return;
    }

    // Delete board
    if (target.dataset.deleteBoard) {
      e.stopPropagation();
      const boardId = target.dataset.deleteBoard;
      const board = state.boards.find(b => b.id === boardId);
      if (!board) return;
      try {
        await invoke('delete_board', { id: boardId });
        toast(`Board "${board.name}" deleted`, 'success');
        await loadBoards();
        if (state.activeBoardId === boardId) {
          navigateTo('all-posts');
        }
      } catch (err) {
        toast('Failed to delete board', 'error');
      }
      return;
    }

    // Navigate to board
    if (target.dataset.boardId && target.dataset.view === 'board') {
      navigateTo('board', target.dataset.boardId);
      return;
    }

    // Navigate to view
    if (target.dataset.view && target.dataset.view !== 'board') {
      navigateTo(target.dataset.view);
      return;
    }
  });

  // ---- Main content clicks (event delegation) ----
  main.addEventListener('click', async (e) => {
    const target = e.target;

    // Dashboard card click -> navigate to board
    const dashCard = target.closest('.dashboard-card');
    if (dashCard && dashCard.dataset.boardId) {
      navigateTo('board', dashCard.dataset.boardId);
      return;
    }

    // Post row click -> show detail
    const postRow = target.closest('.post-row');
    if (postRow && !target.closest('button')) {
      showPostDetail(postRow.dataset.postId);
      return;
    }

    // Card click -> show detail
    const card = target.closest('.card');
    if (card && !target.closest('button')) {
      showPostDetail(card.dataset.postId);
      return;
    }

    // Add post to board button (in post row)
    if (target.closest('[data-add-post-to-board]')) {
      const btn = target.closest('[data-add-post-to-board]');
      showAddToBoardDialog(btn.dataset.addPostToBoard);
      return;
    }

    // TTS speed cycle
    if (target.closest('[data-tts-speed]')) {
      tts.cycleRate();
      return;
    }

    // Play card TTS
    if (target.closest('[data-play-card]')) {
      const btn = target.closest('[data-play-card]');
      const cardId = btn.dataset.playCard;
      if (tts.isPlaying(cardId)) {
        tts.stop();
      } else {
        const cardEl = btn.closest('.card');
        const contentEl = cardEl ? cardEl.querySelector('.card-content') : null;
        const text = contentEl ? contentEl.textContent : '';
        if (text && text !== 'Loading post...' && text !== '(no content)') {
          tts.play(text, cardId);
          btn.textContent = '\u23F9 Stop';
        }
      }
      return;
    }

    // Toggle card saved status
    if (target.closest('[data-toggle-save]')) {
      const btn = target.closest('[data-toggle-save]');
      const cardId = btn.dataset.toggleSave;
      const currentlySaved = btn.dataset.currentlySaved === 'true';
      const newSaved = !currentlySaved;
      try {
        await invoke('toggle_card_saved', { id: cardId, saved: newSaved });
        // Update local state
        for (const boardId of Object.keys(state.cards)) {
          const cards = state.cards[boardId];
          if (cards) {
            const card = cards.find(c => c.id === cardId);
            if (card) { card.saved = newSaved; break; }
          }
        }
        // Update button in-place
        btn.innerHTML = newSaved ? '&#9733;' : '&#9734;';
        btn.dataset.currentlySaved = String(newSaved);
        btn.title = newSaved ? 'Saved' : 'Click to save';
        btn.classList.toggle('card-saved', newSaved);
        // Update card element class
        const cardEl = btn.closest('.card');
        if (cardEl) cardEl.classList.toggle('card-unsaved', !newSaved);
        toast(newSaved ? 'Card saved' : 'Card unsaved', 'success');
      } catch (err) {
        toast('Failed to update card', 'error');
      }
      return;
    }

    // Delete card
    if (target.closest('[data-delete-card]')) {
      const btn = target.closest('[data-delete-card]');
      const cardId = btn.dataset.deleteCard;
      const boardId = btn.dataset.boardId;
      try {
        await invoke('delete_card', { id: cardId });
        toast('Card removed', 'success');
        await loadCardsForBoard(boardId);
      } catch (err) {
        toast('Failed to remove card', 'error');
      }
      return;
    }

    // Toggle enrichment body visibility (on cards in board view)
    if (target.closest('[data-toggle-enrichment]')) {
      const toggleBtn = target.closest('[data-toggle-enrichment]');
      const body = toggleBtn.nextElementSibling;
      if (body && body.classList.contains('enrichment-body')) {
        const isHidden = body.style.display === 'none';
        body.style.display = isHidden ? '' : 'none';
        const chevron = toggleBtn.querySelector('.collapsible-chevron');
        if (chevron) {
          chevron.style.transform = isHidden ? '' : 'rotate(-90deg)';
        }
      }
      return;
    }

    // Filter buttons
    if (target.closest('.filter-btn')) {
      const filterBtn = target.closest('.filter-btn');
      state.postSourceFilter = filterBtn.dataset.filter;
      renderMainContent();
      return;
    }

    // Refresh posts
    if (target.closest('#refresh-posts-btn')) {
      loadPosts(true);
      return;
    }

    // Load more posts
    if (target.closest('#load-more-posts')) {
      loadPosts(false);
      return;
    }

    // ---- RSS events ----
    // Add RSS feed
    if (target.closest('#add-rss-btn')) {
      const input = document.getElementById('rss-url-input');
      const url = input ? input.value.trim() : '';
      if (!url) {
        toast('Please enter a feed URL', 'error');
        return;
      }
      try {
        await invoke('add_rss_feed', { url });
        toast('Feed added', 'success');
        await loadRssFeeds();
        renderMainContent();
      } catch (err) {
        toast(`Failed to add feed: ${err}`, 'error');
      }
      return;
    }

    // Remove RSS feed
    if (target.closest('[data-remove-feed]')) {
      const btn = target.closest('[data-remove-feed]');
      const url = btn.dataset.removeFeed;
      try {
        await invoke('remove_rss_feed', { url });
        toast('Feed removed', 'success');
        await loadRssFeeds();
        renderMainContent();
      } catch (err) {
        toast('Failed to remove feed', 'error');
      }
      return;
    }

    // Auto-organize toggle
    if (target.closest('#auto-organize-toggle')) {
      state.autoOrganizeEnabled = !state.autoOrganizeEnabled;
      try {
        await invoke('save_setting', { key: 'auto_organize_enabled', value: state.autoOrganizeEnabled ? 'true' : 'false' });
      } catch (err) {
        console.error('Failed to save auto-organize setting:', err);
      }
      renderMainContent();
      return;
    }

    // Topic filter toggle
    if (target.closest('[data-toggle-topic]')) {
      const btn = target.closest('[data-toggle-topic]');
      const topic = btn.dataset.toggleTopic.toLowerCase();
      const idx = state.excludedTopics.indexOf(topic);
      if (idx >= 0) {
        state.excludedTopics.splice(idx, 1);
      } else {
        state.excludedTopics.push(topic);
      }
      try {
        await invoke('save_setting', { key: 'excluded_topics', value: JSON.stringify(state.excludedTopics) });
      } catch (err) {
        console.error('Failed to save excluded topics:', err);
      }
      renderMainContent();
      return;
    }

    // Auto-fetch toggle
    if (target.closest('#auto-fetch-toggle')) {
      state.autoFetchEnabled = !state.autoFetchEnabled;
      try {
        await invoke('save_setting', { key: 'auto_fetch_enabled', value: state.autoFetchEnabled ? 'true' : 'false' });
      } catch (err) {
        console.error('Failed to save auto-fetch setting:', err);
      }
      restartAutoFetchScheduler();
      renderMainContent();
      return;
    }

    // Auto-fetch interval selector
    if (target.closest('[data-set-interval]')) {
      const btn = target.closest('[data-set-interval]');
      state.autoFetchInterval = parseInt(btn.dataset.setInterval, 10);
      try {
        await invoke('save_setting', { key: 'auto_fetch_interval', value: String(state.autoFetchInterval) });
      } catch (err) {
        console.error('Failed to save auto-fetch interval:', err);
      }
      restartAutoFetchScheduler();
      renderMainContent();
      return;
    }

    // Fetch RSS posts
    if (target.closest('#fetch-rss-btn')) {
      await performRssFetch();
      return;
    }

    // ---- Open external URL ----
    if (target.closest('[data-open-url]')) {
      const url = target.closest('[data-open-url]').dataset.openUrl;
      shellOpen(url).catch(() => toast('Failed to open URL', 'error'));
      return;
    }

    // ---- Classifier events ----
    // Select provider
    if (target.closest('[data-select-provider]')) {
      const btn = target.closest('[data-select-provider]');
      const provider = btn.dataset.selectProvider;
      try {
        await invoke('classifier_set_provider', { provider });
        toast(`Switched to ${provider}`, 'success');
        await checkClassifier();
      } catch (err) {
        toast(`Failed to switch provider: ${err}`, 'error');
      }
      renderMainContent();
      return;
    }

    // Select model
    if (target.closest('[data-select-model]')) {
      const btn = target.closest('[data-select-model]');
      const model = btn.dataset.selectModel;
      try {
        await invoke('classifier_set_model', { model });
        if (state.classifierConfig) state.classifierConfig.model = model;
        toast(`Model set to ${model}`, 'success');
      } catch (err) {
        toast(`Failed to set model: ${err}`, 'error');
      }
      renderMainContent();
      return;
    }

    // Save API key
    if (target.closest('#save-api-key-btn')) {
      const input = document.getElementById('classifier-api-key-input');
      const apiKey = input ? input.value.trim() : '';
      if (!apiKey) { toast('Please enter an API key', 'error'); return; }
      const provider = state.classifierConfig?.provider || 'anthropic';
      try {
        await invoke('classifier_set_api_key', { provider, apiKey });
        toast('API key saved', 'success');
        await checkClassifier();
        renderMainContent();
      } catch (err) {
        toast(`Failed to save API key: ${err}`, 'error');
      }
      return;
    }

    // Save Tavily API key
    if (target.closest('#save-tavily-key-btn')) {
      const input = document.getElementById('tavily-api-key-input');
      const apiKey = input ? input.value.trim() : '';
      if (!apiKey) { toast('Please enter a Tavily API key', 'error'); return; }
      try {
        await invoke('set_tavily_api_key', { apiKey });
        toast('Tavily API key saved', 'success');
        await checkClassifier();
        renderMainContent();
      } catch (err) {
        toast(`Failed to save Tavily key: ${err}`, 'error');
      }
      return;
    }

    // Refresh classifier
    if (target.closest('#refresh-classifier-btn')) {
      setLoading('classifier', true);
      renderMainContent();
      await checkClassifier();
      setLoading('classifier', false);
      renderMainContent();
      toast('Classifier status refreshed', 'info');
      return;
    }
  });

  // ---- Search input ----
  main.addEventListener('input', (e) => {
    if (e.target.id === 'post-search') {
      state.postSearch = e.target.value;
      // Debounced render
      clearTimeout(state._searchTimer);
      state._searchTimer = setTimeout(() => renderMainContent(), 200);
    }
  });

  // ---- Modal events ----
  const modalOverlay = document.getElementById('modal-overlay');
  const modalBody = document.getElementById('modal-body');

  // Close main modal
  document.getElementById('modal-close').addEventListener('click', () => closeModal('modal-overlay'));
  modalOverlay.addEventListener('click', (e) => {
    if (e.target === modalOverlay) closeModal('modal-overlay');
  });

  // Modal action buttons (event delegation inside modal body)
  modalBody.addEventListener('click', async (e) => {
    const target = e.target.closest('button');
    if (!target) return;

    // Add to board from modal
    if (target.dataset.modalAddToBoard) {
      showAddToBoardDialog(target.dataset.modalAddToBoard);
      return;
    }

    // Classify
    if (target.dataset.modalClassify) {
      const postId = target.dataset.modalClassify;
      const extras = document.getElementById('post-detail-extras');
      extras.innerHTML = `<div class="post-detail-section">${spinnerHTML()}</div>`;
      try {
        const result = await invoke('classify_post', { postId });
        let html = '<div class="post-detail-section"><h4>Classification</h4>';
        if (result.categories && result.categories.length) {
          html += '<div class="flex flex-wrap gap-8 mb-8">';
          html += result.categories.map(c => `<span class="tag">${esc(c)}</span>`).join('');
          html += '</div>';
        }
        if (result.tags && result.tags.length) {
          html += '<div class="flex flex-wrap gap-8 mb-8">';
          html += result.tags.map(t => `<span class="tag">${esc(t)}</span>`).join('');
          html += '</div>';
        }
        if (result.sentiment) {
          html += sentimentBadge(result.sentiment);
        }
        if (result.confidence != null) {
          html += `
            <div class="mt-8 text-small text-secondary">Confidence: ${Math.round(result.confidence * 100)}%</div>
            <div class="confidence-bar"><div class="confidence-fill" style="width:${result.confidence * 100}%"></div></div>`;
        }
        html += '</div>';
        extras.innerHTML = html;
      } catch (err) {
        extras.innerHTML = `<div class="post-detail-section"><h4>Classification</h4><p class="text-secondary">Failed: ${esc(String(err))}</p></div>`;
      }
      return;
    }

    // Summarize
    if (target.dataset.modalSummarize) {
      const postId = target.dataset.modalSummarize;
      const extras = document.getElementById('post-detail-extras');
      extras.innerHTML = `<div class="post-detail-section">${spinnerHTML()}</div>`;
      try {
        const summary = await invoke('summarize_post', { postId });
        extras.innerHTML = `<div class="post-detail-section"><h4>Summary</h4><p>${esc(summary)}</p></div>`;
      } catch (err) {
        extras.innerHTML = `<div class="post-detail-section"><h4>Summary</h4><p class="text-secondary">Failed: ${esc(String(err))}</p></div>`;
      }
      return;
    }

    // Generate derivative
    if (target.dataset.modalDerivative) {
      const postId = target.dataset.modalDerivative;
      const extras = document.getElementById('post-detail-extras');
      extras.innerHTML = `<div class="post-detail-section">${spinnerHTML()}</div>`;
      try {
        const derivative = await invoke('generate_derivative', { postId });
        extras.innerHTML = `
          <div class="post-detail-section">
            <div class="post-detail-section-header">
              <h4>Derivative Post</h4>
              <button class="btn btn-secondary btn-small" data-copy-text="derivative-text">Copy</button>
            </div>
            <pre id="derivative-text">${esc(derivative)}</pre>
          </div>`;
      } catch (err) {
        extras.innerHTML = `<div class="post-detail-section"><h4>Derivative Post</h4><p class="text-secondary">Failed: ${esc(String(err))}</p></div>`;
      }
      return;
    }

    // Learn (enrich post)
    if (target.dataset.modalLearn) {
      const postId = target.dataset.modalLearn;
      const extras = document.getElementById('post-detail-extras');
      target.disabled = true;
      target.textContent = 'Researching...';
      extras.innerHTML = `<div class="post-detail-section">${spinnerHTML()}</div>`;
      try {
        const enrichment = await invoke('enrich_post_learn', { postId });
        extras.innerHTML = renderModalEnrichmentHTML(enrichment);
        target.textContent = 'Learned';
        target.classList.add('btn-learn-done');
        target.disabled = false;
        toast('Research insights generated', 'success');
      } catch (err) {
        extras.innerHTML = `<div class="post-detail-section"><h4>Research Insights</h4><p class="text-secondary">Failed: ${esc(String(err))}</p></div>`;
        target.textContent = 'Learn';
        target.disabled = false;
        toast(`Learn failed: ${err}`, 'error');
      }
      return;
    }

    // TTS speed cycle (in modal)
    if (target.dataset.ttsSpeed !== undefined) {
      tts.cycleRate();
      return;
    }

    // Save enrichment as .md
    if (target.dataset.saveEnrichment !== undefined) {
      if (!lastModalEnrichment) { toast('No enrichment to save', 'error'); return; }
      const md = enrichmentToMarkdown(lastModalEnrichment);
      downloadFile(md, 'research-insights.md');
      toast('Saved as Markdown', 'success');
      return;
    }

    // Play enrichment TTS
    if (target.dataset.playEnrichment !== undefined) {
      const id = 'modal-enrichment';
      if (tts.isPlaying(id)) {
        tts.stop();
        target.textContent = '\u25B6 Play';
      } else if (lastModalEnrichment) {
        tts.play(lastModalEnrichment.synthesis, id);
        target.textContent = '\u23F9 Stop';
      }
      return;
    }

    // Copy text to clipboard
    if (target.dataset.copyText) {
      const sourceEl = document.getElementById(target.dataset.copyText);
      if (sourceEl) {
        try {
          await navigator.clipboard.writeText(sourceEl.textContent);
          const original = target.textContent;
          target.textContent = 'Copied!';
          setTimeout(() => { target.textContent = original; }, 1500);
        } catch (err) {
          toast('Failed to copy to clipboard', 'error');
        }
      }
      return;
    }
  });

  // ---- New Board Dialog ----
  document.getElementById('new-board-close').addEventListener('click', () => closeModal('new-board-overlay'));
  document.getElementById('new-board-cancel').addEventListener('click', () => closeModal('new-board-overlay'));
  document.getElementById('new-board-overlay').addEventListener('click', (e) => {
    if (e.target.id === 'new-board-overlay') closeModal('new-board-overlay');
  });

  document.getElementById('new-board-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const nameInput = document.getElementById('board-name-input');
    const descInput = document.getElementById('board-desc-input');
    const name = nameInput.value.trim();
    const description = descInput.value.trim() || null;

    if (!name) {
      toast('Board name is required', 'error');
      return;
    }

    try {
      await invoke('create_board', { name, description });
      toast(`Board "${name}" created`, 'success');
      nameInput.value = '';
      descInput.value = '';
      closeModal('new-board-overlay');
      await loadBoards();
    } catch (err) {
      toast(`Failed to create board: ${err}`, 'error');
    }
  });

  // ---- Add to Board Dialog ----
  document.getElementById('add-to-board-close').addEventListener('click', () => closeModal('add-to-board-overlay'));
  document.getElementById('add-to-board-overlay').addEventListener('click', (e) => {
    if (e.target.id === 'add-to-board-overlay') closeModal('add-to-board-overlay');
  });

  // Board selection & card creation (event delegation inside add-to-board-content)
  document.getElementById('add-to-board-content').addEventListener('click', async (e) => {
    const selectItem = e.target.closest('[data-select-board]');
    if (selectItem) {
      pendingCard.boardId = selectItem.dataset.selectBoard;
      pendingCard.postId = selectItem.dataset.postId;

      // Highlight selected
      document.querySelectorAll('.board-select-item').forEach(el => {
        el.style.borderColor = '';
        el.style.background = '';
      });
      selectItem.style.borderColor = 'var(--color-accent)';
      selectItem.style.background = 'rgba(0, 122, 255, 0.04)';

      // Show summary/tags fields
      const fields = document.getElementById('add-to-board-fields');
      if (fields) fields.style.display = 'block';
      return;
    }

    // Cancel add card
    if (e.target.closest('#cancel-add-card')) {
      closeModal('add-to-board-overlay');
      pendingCard = { boardId: null, postId: null };
      return;
    }

    // Confirm add card
    if (e.target.closest('#confirm-add-card')) {
      if (!pendingCard.boardId || !pendingCard.postId) {
        toast('Please select a board', 'error');
        return;
      }
      const summary = document.getElementById('card-summary-input')?.value.trim() || null;
      const tagsRaw = document.getElementById('card-tags-input')?.value.trim() || '';
      const tags = tagsRaw ? tagsRaw.split(',').map(t => t.trim()).filter(Boolean) : [];

      try {
        await invoke('create_card', {
          boardId: pendingCard.boardId,
          postId: pendingCard.postId,
          summary,
          tags,
          saved: true,
        });
        toast('Post added to board', 'success');
        closeModal('add-to-board-overlay');

        // Refresh cards if viewing the target board
        if (state.currentView === 'board' && state.activeBoardId === pendingCard.boardId) {
          await loadCardsForBoard(pendingCard.boardId);
        }
        // Also refresh cached cards
        delete state.cards[pendingCard.boardId];
        pendingCard = { boardId: null, postId: null };
      } catch (err) {
        toast(`Failed to add card: ${err}`, 'error');
      }
      return;
    }
  });

  // ---- Theme toggle ----
  document.getElementById('theme-toggle').addEventListener('click', async () => {
    theme.toggle();
    try {
      await invoke('save_setting', { key: 'theme', value: theme.current });
    } catch (err) {
      console.error('Failed to save theme:', err);
    }
  });

  // ---- Keyboard shortcuts ----
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      closeModal('modal-overlay');
      closeModal('new-board-overlay');
      closeModal('add-to-board-overlay');
    }
  });

  // ---- MutationObserver to hydrate card posts after DOM updates ----
  const observer = new MutationObserver(() => {
    if (state.currentView === 'board') {
      hydrateCardPosts();
      hydrateCardEnrichments();
    }
  });
  observer.observe(main, { childList: true, subtree: true });
}

// ============================================================
// Initialization
// ============================================================

window.addEventListener('DOMContentLoaded', async () => {
  // Initialize theme early to prevent flash.
  theme.init();
  try {
    const saved = await invoke('get_setting', { key: 'theme' });
    if (saved === 'light' || saved === 'dark') {
      theme.apply(saved);
    } else {
      theme.updateToggleUI();
    }
  } catch (_) {
    theme.updateToggleUI();
  }

  setupEventListeners();

  // Load initial data
  await loadBoards();

  // Restore auto-organize settings.
  try {
    const val = await invoke('get_setting', { key: 'auto_organize_enabled' });
    state.autoOrganizeEnabled = val === 'true';
  } catch (_) {}
  try {
    const raw = await invoke('get_setting', { key: 'excluded_topics' });
    if (raw) state.excludedTopics = JSON.parse(raw);
  } catch (_) {}

  // Restore auto-fetch settings.
  try {
    const afVal = await invoke('get_setting', { key: 'auto_fetch_enabled' });
    state.autoFetchEnabled = afVal === 'true';
  } catch (_) {}
  try {
    const afInt = await invoke('get_setting', { key: 'auto_fetch_interval' });
    if (afInt) state.autoFetchInterval = parseInt(afInt, 10) || 30;
  } catch (_) {}
  if (state.autoFetchEnabled) startAutoFetchScheduler();

  // Pre-check classifier availability for auto-organize warning.
  checkClassifier();

  // Countdown updater — refresh countdown text every 30s while on RSS Feeds view.
  setInterval(() => {
    if (state.currentView === 'rss-feeds' && autoFetchNextAt) {
      document.querySelectorAll('.auto-fetch-countdown').forEach(el => {
        el.textContent = getNextFetchText();
      });
    }
  }, 30000);

  // Default to Dashboard view
  navigateTo('dashboard');
});
