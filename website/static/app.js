/**
 * MangaRead client-side features.
 *
 * Handles source preference toggling, read-status tracking via localStorage,
 * reader page navigation (keyboard, click zones), and auto-scroll to resume
 * reading position.
 */

(function () {
  'use strict';

  // ============================================================
  //  DOM UTILITIES
  // ============================================================

  /**
   * Shorthand for document.createElement with props and children.
   *
   * Props: className, id, textContent, htmlContent, style (object), on* events.
   * Strings become text nodes, DOM nodes get appended.
   */
  function h(tag, props) {
    const el = document.createElement(tag);
    let key, val, child, i;
    if (props) {
      for (key in props) {
        if (!Object.prototype.hasOwnProperty.call(props, key)) continue;
        val = props[key];
        if (key === 'className') { el.className = val; }
        else if (key === 'id') { el.id = val; }
        else if (key === 'textContent') { el.textContent = val; }
        else if (key === 'htmlContent') { el.innerHTML = val; }
        else if (key === 'style' && typeof val === 'object') { Object.assign(el.style, val); }
        else if (key.slice(0, 2) === 'on') { el.addEventListener(key.slice(2).toLowerCase(), val); }
        else if (val !== false && val != null) { el.setAttribute(key, val); }
      }
    }
    for (i = 2; i < arguments.length; i++) {
      child = arguments[i];
      if (typeof child === 'string') { el.appendChild(document.createTextNode(child)); }
      else if (child) { el.appendChild(child); }
    }
    return el;
  }

  // ============================================================
  //  TOAST SYSTEM
  // ============================================================

  function showToast(message, type) {
    type = type || 'error';
    let container = document.getElementById('toast-container');
    if (!container) {
      container = h('div', { id: 'toast-container' });
      document.body.appendChild(container);
    }
    const el = h('div', { className: 'toast ' + type, textContent: message });
    container.appendChild(el);
    setTimeout(function () {
      el.classList.add('removing');
      setTimeout(function () { el.remove(); }, 300);
    }, 4000);
  }

  // ============================================================
  //  SOURCE PREFERENCES
  // ============================================================

  const STORAGE_SOURCES = 'mangaread_sources';

  /** @returns {string[]} list of enabled source names */
  function getEnabledSources() {
    try {
      const raw = localStorage.getItem(STORAGE_SOURCES);
      if (raw) return JSON.parse(raw);
    } catch (_) { /* ignore */ }
    return []; // empty = all enabled
  }

  /** @param {string[]} sources */
  function setEnabledSources(sources) {
    localStorage.setItem(STORAGE_SOURCES, JSON.stringify(sources));
  }

  /** @param {string} name source name */
  function toggleSource(name) {
    const list = getEnabledSources();
    const idx = list.indexOf(name);
    if (idx >= 0) list.splice(idx, 1);
    else list.push(name);
    setEnabledSources(list);
    applySourceFilters();
  }

  /** Apply source filter checkboxes and show/hide chapter list items accordingly. */
  function applySourceFilters() {
    const enabled = getEnabledSources();
    const toggles = document.querySelectorAll('.source-toggle input');
    toggles.forEach(t => {
      t.checked = enabled.length === 0 || enabled.indexOf(t.value) >= 0;
    });
    // Show/hide chapters
    document.querySelectorAll('.chapter-list > li[data-source]').forEach(li => {
      const src = li.getAttribute('data-source');
      const visible = enabled.length === 0 || enabled.indexOf(src) >= 0;
      li.style.display = visible ? '' : 'none';
    });
  }

  // ============================================================
  //  READ TRACKING
  // ============================================================

  // The read-state helpers are the single write path for progress on every
  // surface (series list, home, and reader.js). `reader.js` reaches them
  // through `window.MangaReadState`; keeping them here means the
  // `MangaSync.pushProgress` write-through lives in exactly one place.
  window.MangaReadState = {
    /** @param {number|string} seriesId */
    storageKey(seriesId) {
      return 'mangaread_read_' + seriesId;
    },

    /** @returns {Object} chapterId -> {status, page, updated} */
    getReadState(seriesId) {
      try {
        const raw = localStorage.getItem(window.MangaReadState.storageKey(seriesId));
        if (raw) return JSON.parse(raw);
      } catch (_) { /* ignore */ }
      return {};
    },

    /**
     * Persist the full read-state object for a series to localStorage.
     * @param {number|string} seriesId
     * @param {Object} state
     */
    saveReadState(seriesId, state) {
      localStorage.setItem(window.MangaReadState.storageKey(seriesId), JSON.stringify(state));
    },

    /**
     * @param {number|string} seriesId
     * @param {number|string} chapterId
     * @param {'unread'|'started'|'finished'} status
     * @param {number} [page] last viewed page
     */
    setChapterStatus(seriesId, chapterId, status, page) {
      const state = window.MangaReadState.getReadState(seriesId);
      state['ch_' + chapterId] = { status, page: page || 1, updated: new Date().toISOString() };
      window.MangaReadState.saveReadState(seriesId, state);
      // Write-through to server when logged in; failures are swallowed
      // (localStorage stays the source of truth offline).
      try {
        if (typeof window.MangaSync !== 'undefined') {
          window.MangaSync.pushProgress(chapterId, page || 1, status);
        }
      } catch (_) { /* ignore */ }
    },

    /** @returns {{status:string, page:number}|null} */
    getChapterStatus(seriesId, chapterId) {
      const state = window.MangaReadState.getReadState(seriesId);
      return state['ch_' + chapterId] || null;
    },
  };

  /**
   * Apply read-status CSS classes (`ch-unread`, `ch-started`, `ch-finished`)
   * to each chapter list item based on the saved read state.
   * @param {number|string} seriesId
   */
  function applyReadStatus(seriesId) {
    const state = window.MangaReadState.getReadState(seriesId);
    document.querySelectorAll('.chapter-list > li[data-chapter-id]').forEach(li => {
      const chId = li.getAttribute('data-chapter-id');
      const info = state['ch_' + chId];
      li.classList.remove('ch-unread', 'ch-started', 'ch-finished');
      if (!info || info.status === 'unread') {
        li.classList.add('ch-unread');
      } else if (info.status === 'started') {
        li.classList.add('ch-started');
        // Add resume badge if there's a saved page
        const numPages = parseInt(li.getAttribute('data-pages') || '0', 10);
        const resumeLink = li.querySelector('.ch-resume');
        if (resumeLink && info.page) {
          resumeLink.textContent = 'Resume p.' + info.page + (numPages ? '/' + numPages : '');
          resumeLink.style.display = '';
        }
      } else if (info.status === 'finished') {
        li.classList.add('ch-finished');
      }
    });
  }

  // ============================================================
  //  CONTINUE READING
  // ============================================================

  function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
  }

  function initContinueReading() {
    const container = document.getElementById('continue-reading-container');
    if (!container) return;

    // Collect unique series IDs from localStorage
    const inProgress = {};
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key && key.indexOf('mangaread_read_') === 0) {
        const seriesId = key.replace('mangaread_read_', '');
        try {
          const state = JSON.parse(localStorage.getItem(key));
          for (const chKey in state) {
            if (state[chKey].status === 'started') {
              inProgress[seriesId] = (inProgress[seriesId] || 0) + 1;
            }
          }
        } catch (_) {}
      }
    }

    const ids = Object.keys(inProgress);
    if (ids.length === 0) {
      container.remove();
      return;
    }

    fetch('/api/progress-series?ids=' + ids.join(','))
      .then(function (r) { return r.json(); })
      .then(function (data) {
        if (!data || data.length === 0) {
          container.remove();
          return;
        }
        let html = '<h2 class="section-title">Continue Reading</h2>';
        html += '<div class="continue-scroll">';
        data.forEach(function (item) {
          const cover = item.cover_url || '';
          const url = '/read/' + item.manga_id + '/' + item.chapter_id;
          const pct = item.total_pages > 0
            ? Math.round((item.page / item.total_pages) * 100)
            : 0;
          html += '<a href="' + url + '" class="continue-card">';
          html += '<div class="cover-wrapper"' + (cover ? ' style="background-image: url(\'' + cover.replace(/'/g, '%27') + '\')"' : '') + '>';
          html += '<div class="continue-overlay">';
          html += '<span class="continue-ch">Ch. ' + item.chapter_number + '</span>';
          if (item.total_pages > 0) {
            html += '<div class="continue-pct">p. ' + item.page + ' / ' + item.total_pages + '</div>';
          }
          html += '</div>';
          html += '<div class="continue-progress">';
          html += '<div class="continue-progress-bar" style="width:' + pct + '%"></div>';
          html += '</div>';
          html += '</div>';
          html += '</a>';
        });
        html += '</div>';
        container.innerHTML = html;
      })
      .catch(function () { container.remove(); });
  }

  document.addEventListener('DOMContentLoaded', function () {
    // Init continue reading
    initContinueReading();

    // Rescan button handler
    const rescanBtn = document.getElementById('rescan-btn');
    if (rescanBtn) {
      rescanBtn.addEventListener('click', function () {
        const mangaId = rescanBtn.getAttribute('data-manga-id');
        rescanBtn.disabled = true;
        rescanBtn.textContent = ' Syncing...';
        fetch('/api/sync-manga/' + mangaId, { method: 'POST' })
          .then(function () {
            showToast('Sync started - page will refresh soon.', 'success');
            setTimeout(function () { location.reload(); }, 5000);
          })
          .catch(function () {
            showToast('Failed to start sync.');
            rescanBtn.disabled = false;
            rescanBtn.textContent = ' Refresh';
          });
      });
    }

    // Check for sync errors on series page
    const syncCheck = document.querySelector('[data-series-id]');
    if (syncCheck) {
      fetch('/api/sync-status')
        .then(function (r) { return r.json(); })
        .then(function (data) {
          if (data.last_error) {
            showToast(data.last_error, 'error');
          }
        })
        .catch(function () {});
    }

    // --- Series detail page ---
    const chapterList = document.getElementById('chapter-list');
    if (chapterList) {
      const seriesId = chapterList.getAttribute('data-series-id');

      // Build source toggles
      let sources = [];
      try { sources = JSON.parse(chapterList.getAttribute('data-sources') || '[]'); } catch (_) {}
      const toggleContainer = document.getElementById('source-toggles');
      if (toggleContainer && sources.length > 1) {
        sources.forEach(src => {
          toggleContainer.appendChild(
            h('label', { className: 'source-toggle' },
              h('input', { type: 'checkbox', value: src, onChange: function () { toggleSource(src); } }),
              ' ' + src
            )
          );
        });
      }

      // Apply filters and read status
      applySourceFilters();
      applyReadStatus(seriesId);

      // Setup filter reset
      const resetBtn = document.getElementById('source-reset');
      if (resetBtn) {
        resetBtn.addEventListener('click', () => {
          localStorage.removeItem(STORAGE_SOURCES);
          applySourceFilters();
        });
      }
    }

    // Bookmark buttons on series detail page
    // Bookmark buttons on series detail page
    initBookmarkButtons();

    // Bookmarks page
    initBookmarksPage();
  });

  // ============================================================
  //  BOOKMARKS
  // ============================================================

  const STORAGE_BOOKMARKS = 'mangaread_bookmarks';

  function getBookmarks() {
    try {
      const raw = localStorage.getItem(STORAGE_BOOKMARKS);
      if (raw) return JSON.parse(raw);
    } catch (_) {}
    return [];
  }

  function setBookmarks(ids) {
    localStorage.setItem(STORAGE_BOOKMARKS, JSON.stringify(ids));
  }

  function isBookmarked(seriesId) {
    return getBookmarks().indexOf(seriesId) !== -1;
  }

  function toggleBookmark(seriesId) {
    const ids = getBookmarks();
    const idx = ids.indexOf(seriesId);
    if (idx !== -1) {
      ids.splice(idx, 1);
    } else {
      ids.push(seriesId);
    }
    setBookmarks(ids);
    const added = idx === -1;
    // Write-through to server when logged in; failures are swallowed.
    try {
      if (typeof window.MangaSync !== 'undefined') {
        window.MangaSync.pushBookmark(seriesId, added);
      }
    } catch (_) { /* ignore */ }
    return added;
  }

  function initBookmarkButtons() {
    const btns = document.querySelectorAll('.bookmark-btn');
    btns.forEach(function (btn) {
      const seriesId = parseInt(btn.getAttribute('data-series-id'), 10);
      const label = btn.querySelector('.bookmark-label');

      function update() {
        const marked = isBookmarked(seriesId);
        btn.classList.toggle('bookmarked', marked);
        if (label) label.textContent = marked ? 'Bookmarked' : 'Bookmark';
      }

      update();
      btn.addEventListener('click', function (e) {
        e.preventDefault();
        toggleBookmark(seriesId);
        update();
        showToast(
          isBookmarked(seriesId) ? 'Series bookmarked' : 'Bookmark removed',
          'success'
        );
      });
    });
  }

  function initBookmarksPage() {
    const container = document.getElementById('bookmarks-container');
    const emptyEl = document.getElementById('bookmarks-empty');
    if (!container) return;

    // When logged in, seed the grid from the server; fall back to
    // localStorage on any fetch failure.
    let loggedIn = false;
    try {
      loggedIn = !!(document.body && document.body.dataset && document.body.dataset.userId);
    } catch (_) { /* ignore */ }

    if (loggedIn) {
      fetch('/api/bookmarks', { headers: { 'Accept': '*/*' } })
        .then(function (r) {
          if (!r.ok) throw new Error('bookmarks fetch failed');
          return r.json();
        })
        .then(function (data) {
          renderBookmarkIds(data && Array.isArray(data.series) ? data.series : getBookmarks());
        })
        .catch(function () {
          renderBookmarkIds(getBookmarks());
        });
      return;
    }

    renderBookmarkIds(getBookmarks());
  }

  function renderBookmarkIds(ids) {
    const container = document.getElementById('bookmarks-container');
    const emptyEl = document.getElementById('bookmarks-empty');
    if (!container) return;

    if (ids.length === 0) {
      container.style.display = 'none';
      if (emptyEl) emptyEl.style.display = '';
      return;
    }

    fetch('/api/progress-series?ids=' + ids.join(','))
      .then(function (r) { return r.json(); })
      .then(function (series) {
        if (!series || series.length === 0) {
          container.style.display = 'none';
          if (emptyEl) emptyEl.style.display = '';
          return;
        }
        container.innerHTML = '';
        series.forEach(function (s) {
          const coverContent = s.cover_url
            ? h('img', { src: s.cover_url, alt: '', loading: 'lazy' })
            : h('div', { className: 'card-placeholder' });
          const link = h('a', { href: '/series/' + s.id },
            h('div', { className: 'cover-wrapper' },
              coverContent,
              h('div', { className: 'card-overlay' })
            ),
            h('div', { className: 'info' },
              h('h3', { textContent: s.title })
            )
          );
          const btn = h('button', {
            className: 'bookmark-btn bookmarked',
            'data-series-id': s.id,
            htmlContent: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z"></path></svg>'
          });
          const card = h('div', { className: 'manga-card' }, link, btn);
          container.appendChild(card);
        });
        // Wire up bookmark buttons in the grid
        initBookmarkButtons();
      })
      .catch(function () {
        container.innerHTML = '<div class="empty-state"><p>Failed to load bookmarks.</p></div>';
      });
  }

})();

// Bulma ships no JavaScript, so the navbar burger toggle is ours.
document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll('.navbar-burger').forEach((burger) => {
    burger.addEventListener('click', () => {
      const target = document.getElementById(burger.dataset.target);
      if (!target) return;
      burger.classList.toggle('is-active');
      target.classList.toggle('is-active');
      burger.setAttribute(
        'aria-expanded',
        String(burger.classList.contains('is-active')),
      );
    });
  });
});
