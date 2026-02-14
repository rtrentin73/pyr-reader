import { invoke } from '@tauri-apps/api/core';

// ============================================================
// State
// ============================================================
const state = {
  // Navigation
  currentView: 'all-posts',   // 'board', 'all-posts', 'rss-feeds', 'x-twitter', 'linkedin', 'classifier'
  activeBoardId: null,

  // Data
  boards: [],
  cards: {},            // boardId -> Card[]
  posts: [],
  postsOffset: 0,
  postsLimit: 40,
  postsHasMore: true,

  // Filters (All Posts view)
  postSearch: '',
  postSourceFilter: 'all', // 'all' | 'XTwitter' | 'RSS'

  // RSS
  rssFeeds: [],
  rssFetchedPosts: [],

  // X/Twitter
  xAuthenticated: false,
  xTimelinePosts: [],

  // LinkedIn
  linkedinAuthenticated: false,
  linkedinFollowedProfiles: [],
  linkedinFetchedPosts: [],

  // Classifier
  classifierAvailable: false,
  classifierModels: [],

  // Loading flags
  loading: {},
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
  if (source === 'XTwitter') {
    return '<span class="card-source-badge badge-x">X</span>';
  }
  if (source === 'RSS') {
    return '<span class="card-source-badge badge-rss">RSS</span>';
  }
  if (source === 'LinkedIn') return '<span class="card-source-badge badge-linkedin">in</span>';
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

/** Inline SVG of the Great Pyrenees mascot (matches app icon) */
function mascotSVG(size = 80) {
  return `<svg viewBox="0 0 100 100" width="${size}" height="${size}" xmlns="http://www.w3.org/2000/svg">
    <!-- Blue circle background with white border -->
    <circle cx="50" cy="50" r="46" fill="#A8BFE6" stroke="#fff" stroke-width="3"/>
    <!-- Ears -->
    <ellipse cx="30" cy="40" rx="8" ry="12" fill="#F0ECE6" stroke="#3D4D65" stroke-width="1.5" transform="rotate(-10 30 40)"/>
    <ellipse cx="70" cy="40" rx="8" ry="12" fill="#F0ECE6" stroke="#3D4D65" stroke-width="1.5" transform="rotate(10 70 40)"/>
    <!-- Head -->
    <ellipse cx="50" cy="48" rx="22" ry="20" fill="#F5F2ED" stroke="#3D4D65" stroke-width="1.8"/>
    <!-- Fluffy top -->
    <ellipse cx="50" cy="32" rx="16" ry="8" fill="#F5F2ED" stroke="#3D4D65" stroke-width="1.5"/>
    <!-- Muzzle -->
    <ellipse cx="50" cy="56" rx="12" ry="10" fill="#fff" stroke="#3D4D65" stroke-width="1.2"/>
    <!-- Eyes -->
    <circle cx="41" cy="46" r="3" fill="#2C3E50"/>
    <circle cx="59" cy="46" r="3" fill="#2C3E50"/>
    <circle cx="42" cy="45" r="1" fill="#fff"/>
    <circle cx="60" cy="45" r="1" fill="#fff"/>
    <!-- Nose -->
    <ellipse cx="50" cy="54" rx="3.5" ry="2.5" fill="#2C3E50"/>
    <!-- Mouth -->
    <path d="M47 57 Q50 61 53 57" stroke="#2C3E50" stroke-width="1" fill="none" stroke-linecap="round"/>
    <!-- Tongue -->
    <ellipse cx="50" cy="60.5" rx="2" ry="2.5" fill="#E8787A"/>
  </svg>`;
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

async function checkXAuth() {
  try {
    state.xAuthenticated = await invoke('x_is_authenticated');
  } catch (e) {
    state.xAuthenticated = false;
  }
}

async function checkLinkedInAuth() {
  try {
    state.linkedinAuthenticated = await invoke('linkedin_is_authenticated');
  } catch (e) {
    state.linkedinAuthenticated = false;
  }
}

async function loadLinkedInProfiles() {
  try {
    state.linkedinFollowedProfiles = await invoke('linkedin_list_profiles');
  } catch (e) {
    state.linkedinFollowedProfiles = [];
  }
}

async function checkClassifier() {
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
  if (view === 'board' && boardId) {
    if (!state.cards[boardId]) {
      loadCardsForBoard(boardId);
    }
  } else if (view === 'all-posts') {
    if (state.posts.length === 0) {
      loadPosts(true);
    }
  } else if (view === 'rss-feeds') {
    loadRssFeeds().then(() => renderMainContent());
  } else if (view === 'x-twitter') {
    checkXAuth().then(() => renderMainContent());
  } else if (view === 'linkedin') {
    checkLinkedInAuth().then(() => {
      loadLinkedInProfiles().then(() => renderMainContent());
    });
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
    case 'board':
      renderBoardView(main);
      break;
    case 'all-posts':
      renderAllPostsView(main);
      break;
    case 'rss-feeds':
      renderRssFeedsView(main);
      break;
    case 'x-twitter':
      renderXTwitterView(main);
      break;
    case 'linkedin':
      renderLinkedInView(main);
      break;
    case 'classifier':
      renderClassifierView(main);
      break;
    default:
      renderAllPostsView(main);
  }
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
  return `
    <div class="card" data-card-id="${esc(card.id)}" data-post-id="${esc(card.post_id)}">
      <div class="card-header">
        <span class="card-timestamp">${relativeTime(card.created_at)}</span>
      </div>
      ${card.summary ? `<div class="card-summary">${esc(card.summary)}</div>` : ''}
      <div class="card-content" data-post-id="${esc(card.post_id)}">Loading post...</div>
      ${tagsHTML ? `<div class="card-tags">${tagsHTML}</div>` : ''}
      <div class="card-footer">
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
          <button class="filter-btn ${state.postSourceFilter === 'XTwitter' ? 'active' : ''}" data-filter="XTwitter">X / Twitter</button>
          <button class="filter-btn ${state.postSourceFilter === 'RSS' ? 'active' : ''}" data-filter="RSS">RSS</button>
          <button class="filter-btn ${state.postSourceFilter === 'LinkedIn' ? 'active' : ''}" data-filter="LinkedIn">LinkedIn</button>
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
        <button class="btn btn-primary btn-small" data-add-post-to-board="${esc(post.id)}">+ Board</button>
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
        <button class="btn btn-primary" id="fetch-rss-btn">${isLoading('rss-fetch') ? '<span class="spinner"></span> Fetching...' : 'Fetch Now'}</button>
      </div>

      ${fetchedHTML}
    </div>`;
}

// ---- X/Twitter View ----

function renderXTwitterView(container) {
  const authStatus = state.xAuthenticated
    ? '<span class="status-indicator"><span class="status-dot connected"></span> Connected</span>'
    : '<span class="status-indicator"><span class="status-dot disconnected"></span> Not Connected</span>';

  let timelineHTML = '';
  if (state.xTimelinePosts.length > 0) {
    timelineHTML = `
      <div class="inline-results mt-24">
        <h3>Timeline Posts (${state.xTimelinePosts.length})</h3>
        <div class="posts-list">
          ${state.xTimelinePosts.map(p => renderPostRowHTML(p)).join('')}
        </div>
      </div>`;
  }

  let authSectionHTML = '';
  if (!state.xAuthenticated) {
    authSectionHTML = `
      <div class="settings-section">
        <div class="settings-section-title">Authorization</div>
        <p class="text-secondary mb-12">Click the button below to open X in your browser and authorize this app. The flow completes automatically.</p>
        <button class="btn btn-primary" id="x-start-oauth-btn">${isLoading('x-oauth') ? '<span class="spinner"></span> Waiting for authorization...' : 'Connect to X / Twitter'}</button>
      </div>`;
  }

  container.innerHTML = `
    <div class="view-header">
      <h2>X / Twitter</h2>
      <p>Connect your X account to fetch your timeline</p>
      <div class="view-header-actions">
        ${authStatus}
      </div>
    </div>
    <div class="view-body">
      <div class="settings-section">
        <div class="settings-section-title">API Credentials</div>
        <div class="form-group">
          <label for="x-client-id">Client ID</label>
          <input type="text" id="x-client-id" placeholder="Enter your X API Client ID">
        </div>
        <div class="form-group">
          <label for="x-client-secret">Client Secret</label>
          <input type="password" id="x-client-secret" placeholder="Enter your X API Client Secret">
        </div>
        <button class="btn btn-primary" id="x-setup-btn">Setup</button>
      </div>

      ${authSectionHTML}

      ${state.xAuthenticated ? `
      <div class="settings-section">
        <div class="settings-section-title">Fetch Timeline</div>
        <button class="btn btn-primary" id="x-fetch-timeline-btn">${isLoading('x-timeline') ? '<span class="spinner"></span> Fetching...' : 'Fetch Timeline'}</button>
      </div>` : ''}

      ${timelineHTML}
    </div>`;
}

// ---- LinkedIn View ----

function renderLinkedInView(container) {
  const authStatus = state.linkedinAuthenticated
    ? '<span class="status-indicator"><span class="status-dot connected"></span> Connected</span>'
    : '<span class="status-indicator"><span class="status-dot disconnected"></span> Not Connected</span>';

  let profilesListHTML = '';
  if (state.linkedinFollowedProfiles.length === 0) {
    profilesListHTML = '<p class="text-secondary mt-8">No profiles being followed yet.</p>';
  } else {
    profilesListHTML = '<div class="feed-list">';
    for (const urn of state.linkedinFollowedProfiles) {
      profilesListHTML += `
        <div class="feed-item">
          <span class="feed-item-url">${esc(urn)}</span>
          <button class="btn btn-danger btn-small" data-remove-li-profile="${esc(urn)}">Remove</button>
        </div>`;
    }
    profilesListHTML += '</div>';
  }

  let fetchedHTML = '';
  if (state.linkedinFetchedPosts.length > 0) {
    fetchedHTML = `
      <div class="inline-results mt-24">
        <h3>LinkedIn Posts (${state.linkedinFetchedPosts.length})</h3>
        <div class="posts-list">
          ${state.linkedinFetchedPosts.map(p => renderPostRowHTML(p)).join('')}
        </div>
      </div>`;
  }

  let authSectionHTML = '';
  if (!state.linkedinAuthenticated) {
    authSectionHTML = `
      <div class="settings-section">
        <div class="settings-section-title">Authorization</div>
        <p class="text-secondary mb-12">Click the button below to open LinkedIn in your browser and authorize this app. The flow completes automatically.</p>
        <button class="btn btn-primary" id="li-start-oauth-btn">${isLoading('li-oauth') ? '<span class="spinner"></span> Waiting for authorization...' : 'Connect to LinkedIn'}</button>
      </div>`;
  }

  container.innerHTML = `
    <div class="view-header">
      <h2>LinkedIn</h2>
      <p>Connect your LinkedIn account to follow profiles and company pages</p>
      <div class="view-header-actions">
        ${authStatus}
      </div>
    </div>
    <div class="view-body">
      <div class="settings-section">
        <div class="settings-section-title">API Credentials</div>
        <div class="form-group">
          <label for="li-client-id">Client ID</label>
          <input type="text" id="li-client-id" placeholder="LinkedIn App Client ID">
        </div>
        <div class="form-group">
          <label for="li-client-secret">Client Secret</label>
          <input type="password" id="li-client-secret" placeholder="LinkedIn App Client Secret">
        </div>
        <button class="btn btn-primary" id="li-setup-btn">Save Credentials</button>
      </div>

      ${authSectionHTML}

      ${state.linkedinAuthenticated ? `
      <div class="settings-section">
        <div class="settings-section-title">Followed Profiles / Pages</div>
        <div class="form-inline">
          <input type="text" id="li-profile-input" placeholder="urn:li:person:{id} or urn:li:organization:{id}">
          <button class="btn btn-primary" id="li-add-profile-btn">Add</button>
        </div>
        ${profilesListHTML}
      </div>

      <div class="settings-section">
        <div class="settings-section-title">Fetch Posts</div>
        <button class="btn btn-primary" id="li-fetch-posts-btn">${isLoading('li-fetch') ? '<span class="spinner"></span> Fetching...' : 'Fetch Posts'}</button>
      </div>` : ''}

      ${fetchedHTML}
    </div>`;
}

// ---- Classifier View ----

function renderClassifierView(container) {
  const statusHTML = state.classifierAvailable
    ? '<span class="status-indicator"><span class="status-dot connected"></span> Ollama Available</span>'
    : '<span class="status-indicator"><span class="status-dot disconnected"></span> Ollama Unavailable</span>';

  let modelsHTML = '';
  if (state.classifierModels.length > 0) {
    modelsHTML = `
      <div class="model-list">
        ${state.classifierModels.map(m => `<span class="model-tag">${esc(m)}</span>`).join('')}
      </div>`;
  } else {
    modelsHTML = '<p class="text-secondary mt-8">No models found. Make sure Ollama is running.</p>';
  }

  container.innerHTML = `
    <div class="view-header">
      <h2>Classifier</h2>
      <p>AI-powered content classification and summarization via Ollama</p>
      <div class="view-header-actions">
        ${statusHTML}
      </div>
    </div>
    <div class="view-body">
      <div class="settings-section">
        <div class="settings-section-title">Status</div>
        <p class="text-secondary">The classifier uses a local Ollama instance to categorize posts, generate summaries, and create derivative content.</p>
      </div>

      <div class="settings-section">
        <div class="settings-section-title">Available Models</div>
        ${modelsHTML}
        <div class="mt-12">
          <button class="btn btn-secondary" id="refresh-classifier-btn">${isLoading('classifier') ? '<span class="spinner"></span>' : 'Refresh'}</button>
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

  body.innerHTML = `
    <div class="post-detail-source">${sourceBadge(post.source)}</div>
    <div class="post-detail-author">${esc(post.author || 'Unknown')}</div>
    <div class="post-detail-time">${relativeTime(post.timestamp)}${post.timestamp ? ' &mdash; ' + new Date(post.timestamp).toLocaleString() : ''}</div>
    <div class="post-detail-content">${esc(post.content)}</div>
    ${post.url ? `<a class="post-detail-url" href="${esc(post.url)}" target="_blank" rel="noopener noreferrer">${esc(post.url)}</a>` : ''}
    <div class="post-detail-actions">
      <button class="btn btn-primary btn-small" data-modal-add-to-board="${esc(post.id)}">Add to Board</button>
      <button class="btn btn-secondary btn-small" data-modal-classify="${esc(post.id)}">Classify</button>
      <button class="btn btn-secondary btn-small" data-modal-summarize="${esc(post.id)}">Summarize</button>
      <button class="btn btn-secondary btn-small" data-modal-derivative="${esc(post.id)}">Generate Derivative</button>
    </div>
    <div id="post-detail-extras"></div>
  `;
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
      if (board && confirm(`Delete board "${board.name}"?`)) {
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

    // Fetch RSS posts
    if (target.closest('#fetch-rss-btn')) {
      setLoading('rss-fetch', true);
      renderMainContent();
      try {
        state.rssFetchedPosts = await invoke('fetch_rss_posts');
        toast(`Fetched ${state.rssFetchedPosts.length} posts from RSS feeds`, 'success');
      } catch (err) {
        toast(`RSS fetch failed: ${err}`, 'error');
        state.rssFetchedPosts = [];
      }
      setLoading('rss-fetch', false);
      renderMainContent();
      return;
    }

    // ---- X/Twitter events ----
    // Setup X
    if (target.closest('#x-setup-btn')) {
      const clientId = document.getElementById('x-client-id')?.value.trim();
      const clientSecret = document.getElementById('x-client-secret')?.value.trim();
      if (!clientId || !clientSecret) {
        toast('Please enter both Client ID and Client Secret', 'error');
        return;
      }
      try {
        await invoke('x_setup', { clientId, clientSecret });
        toast('X API credentials saved', 'success');
      } catch (err) {
        toast(`Setup failed: ${err}`, 'error');
      }
      return;
    }

    // One-click X OAuth
    if (target.closest('#x-start-oauth-btn')) {
      setLoading('x-oauth', true);
      renderMainContent();
      try {
        await invoke('x_start_oauth');
        state.xAuthenticated = true;
        toast('Successfully connected to X', 'success');
      } catch (err) {
        toast(`X authorization failed: ${err}`, 'error');
      }
      setLoading('x-oauth', false);
      renderMainContent();
      return;
    }

    // Fetch X timeline
    if (target.closest('#x-fetch-timeline-btn')) {
      setLoading('x-timeline', true);
      renderMainContent();
      try {
        state.xTimelinePosts = await invoke('x_fetch_timeline');
        toast(`Fetched ${state.xTimelinePosts.length} timeline posts`, 'success');
      } catch (err) {
        toast(`Timeline fetch failed: ${err}`, 'error');
        state.xTimelinePosts = [];
      }
      setLoading('x-timeline', false);
      renderMainContent();
      return;
    }

    // ---- LinkedIn events ----
    if (target.closest('#li-setup-btn')) {
      const clientId = document.getElementById('li-client-id')?.value.trim();
      const clientSecret = document.getElementById('li-client-secret')?.value.trim();
      if (!clientId || !clientSecret) { toast('Enter both Client ID and Client Secret', 'error'); return; }
      try {
        await invoke('linkedin_setup', { clientId, clientSecret });
        toast('LinkedIn credentials saved', 'success');
      } catch (err) { toast(`Setup failed: ${err}`, 'error'); }
      return;
    }

    // One-click LinkedIn OAuth
    if (target.closest('#li-start-oauth-btn')) {
      setLoading('li-oauth', true);
      renderMainContent();
      try {
        await invoke('linkedin_start_oauth');
        state.linkedinAuthenticated = true;
        toast('Successfully connected to LinkedIn', 'success');
        await loadLinkedInProfiles();
      } catch (err) {
        toast(`LinkedIn authorization failed: ${err}`, 'error');
      }
      setLoading('li-oauth', false);
      renderMainContent();
      return;
    }

    if (target.closest('#li-add-profile-btn')) {
      const urn = document.getElementById('li-profile-input')?.value.trim();
      if (!urn) { toast('Enter a profile or organization URN', 'error'); return; }
      try {
        await invoke('linkedin_add_profile', { urn });
        toast('Profile added', 'success');
        await loadLinkedInProfiles();
        renderMainContent();
      } catch (err) { toast(`Failed to add profile: ${err}`, 'error'); }
      return;
    }

    if (target.closest('[data-remove-li-profile]')) {
      const urn = target.closest('[data-remove-li-profile]').dataset.removeLiProfile;
      try {
        await invoke('linkedin_remove_profile', { urn });
        toast('Profile removed', 'success');
        await loadLinkedInProfiles();
        renderMainContent();
      } catch (err) { toast('Failed to remove profile', 'error'); }
      return;
    }

    if (target.closest('#li-fetch-posts-btn')) {
      setLoading('li-fetch', true);
      renderMainContent();
      try {
        state.linkedinFetchedPosts = await invoke('linkedin_fetch_posts');
        toast(`Fetched ${state.linkedinFetchedPosts.length} LinkedIn posts`, 'success');
      } catch (err) {
        toast(`LinkedIn fetch failed: ${err}`, 'error');
        state.linkedinFetchedPosts = [];
      }
      setLoading('li-fetch', false);
      renderMainContent();
      return;
    }

    // ---- Classifier events ----
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
        extras.innerHTML = `<div class="post-detail-section"><h4>Derivative Post</h4><pre>${esc(derivative)}</pre></div>`;
      } catch (err) {
        extras.innerHTML = `<div class="post-detail-section"><h4>Derivative Post</h4><p class="text-secondary">Failed: ${esc(String(err))}</p></div>`;
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
    }
  });
  observer.observe(main, { childList: true, subtree: true });
}

// ============================================================
// Initialization
// ============================================================

window.addEventListener('DOMContentLoaded', async () => {
  setupEventListeners();

  // Load initial data
  await loadBoards();

  // Default to All Posts view
  navigateTo('all-posts');
});
