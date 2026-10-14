/**
 * MangaRead client-side features.
 *
 * Handles read-status tracking via localStorage,
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
    const el = h('div', {
      className: 'notification ' + (type === 'success' ? 'is-success' : 'is-danger'),
      'data-toast': true,
      textContent: message,
    });
    container.appendChild(el);
    setTimeout(function () {
      el.classList.add('removing');
      setTimeout(function () { el.remove(); }, 300);
    }, 4000);
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
    document.querySelectorAll('#chapter-list > li[data-chapter-id]').forEach(li => {
      const chId = li.getAttribute('data-chapter-id');
      const info = state['ch_' + chId];
      li.classList.remove('ch-unread', 'ch-started', 'ch-finished');
      if (!info || info.status === 'unread') {
        li.classList.add('ch-unread');
      } else if (info.status === 'started') {
        li.classList.add('ch-started');
        const resumeLink = li.querySelector('.ch-resume');
        if (resumeLink && info.page) {
          resumeLink.textContent = 'Resume p.' + info.page;
          resumeLink.style.display = '';
        }
      } else if (info.status === 'finished') {
        li.classList.add('ch-finished');
      }
    });
  }

  // ============================================================
  //  SHARED CONTENT BUILDERS
  // ============================================================

  function buildCover({ href, title, coverUrl, badge, alt }) {
    const figure = h('figure', { className: 'image is-2by3 m-cover' });
    if (coverUrl) {
      figure.appendChild(h('img', {
        src: coverUrl,
        alt: alt == null ? title : alt,
        loading: 'lazy',
      }));
    } else {
      figure.appendChild(h('span', { 'data-cover-placeholder': true, 'aria-hidden': 'true' }));
      figure.appendChild(h('span', { 'data-cover-initial': true, textContent: (title || '?').charAt(0) }));
    }
    if (badge) {
      figure.appendChild(h('span', {
        className: 'tag is-primary is-light',
        'data-cover-badge': true,
        textContent: badge,
      }));
    }
    figure.appendChild(h('span', { 'data-cover-overlay': true, 'aria-hidden': 'true' }));
    if (href) return h('a', { href: href, 'aria-label': title }, figure);
    return figure;
  }

  function buildMangaCard({ href, title, coverUrl, badge, variant, progress }) {
    const card = h('div', { className: 'card' });
    card.appendChild(h('div', { className: 'card-image' },
      buildCover({ href: href, title: title, coverUrl: coverUrl, badge: badge, alt: title })
    ));
    const content = h('div', { className: 'card-content' },
      h('a', { href: href }, h('p', { className: 'title is-6', textContent: title }))
    );
    if (variant === 'continue' && progress) {
      const value = progress.totalPages > 0
        ? Math.round((progress.page / progress.totalPages) * 100)
        : 0;
      content.appendChild(h('progress', {
        className: 'progress is-primary is-small',
        max: 100,
        value: value,
      }));
      content.appendChild(h('p', {
        className: 'is-size-7 m-text-muted',
        textContent: 'p. ' + progress.page + (progress.totalPages > 0 ? ' / ' + progress.totalPages : ''),
      }));
    }
    if (variant === 'grid') {
      const seriesId = href.split('/').pop();
      const button = h('button', {
        className: 'button is-small is-ghost',
        'data-series-id': seriesId,
        'data-bookmark-button': true,
      }, h('span', {
        'data-bookmark-label': true,
        textContent: 'Bookmark',
      }));
      content.appendChild(button);
    }
    card.appendChild(content);
    return card;
  }

  function buildEmptyState(message) {
    return h('div', { className: 'm-empty' },
      h('span', { className: 'm-empty-icon', 'aria-hidden': 'true' }),
      h('p', { textContent: message })
    );
  }

  function initContinueReading() {
    const container = document.getElementById('continue-reading-container');
    if (!container) return;

    // Collect unique series IDs from localStorage.
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
        container.innerHTML = '';
        container.appendChild(h('h2', { className: 'title is-3', textContent: 'Continue Reading' }));
        const strip = h('div', { className: 'columns is-mobile is-flex-wrap-nowrap' });
        data.forEach(function (item) {
          const totalPages = item.total_pages || 0;
          strip.appendChild(h('div', { className: 'column is-6-mobile is-4-tablet is-3-desktop' },
            buildMangaCard({
              href: '/read/' + item.manga_id + '/' + item.chapter_id,
              title: item.title,
              coverUrl: item.cover_url,
              badge: 'Ch. ' + item.chapter_number,
              variant: 'continue',
              progress: { page: item.page || 1, totalPages: totalPages },
            })
          ));
        });
        container.appendChild(strip);
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

    // --- Series detail page ---
    const chapterList = document.getElementById('chapter-list');
    if (chapterList) {
      const seriesId = chapterList.getAttribute('data-series-id');

      // Read status marks
      applyReadStatus(seriesId);
    }

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
    const btns = document.querySelectorAll('[data-bookmark-button]');
    btns.forEach(function (btn) {
      const seriesId = parseInt(btn.getAttribute('data-series-id'), 10);
      const label = btn.querySelector('[data-bookmark-label]');

      function update() {
        const marked = isBookmarked(seriesId);
        btn.classList.toggle('is-primary', marked);
        btn.classList.toggle('is-ghost', !marked);
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
          container.appendChild(h('div', {
            className: 'column is-6-mobile is-4-tablet is-3-desktop',
          }, buildMangaCard({
            href: '/series/' + s.id,
            title: s.title,
            coverUrl: s.cover_url,
            badge: null,
            variant: 'grid',
          })));
        });
        initBookmarkButtons();
      })
      .catch(function () {
        container.innerHTML = '';
        container.appendChild(buildEmptyState('Failed to load bookmarks.'));
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
