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

  /** @param {number|string} seriesId */
  function storageKey(seriesId) {
    return 'mangaread_read_' + seriesId;
  }

  /** @returns {Object} chapterId -> {status, page, updated} */
  function getReadState(seriesId) {
    try {
      const raw = localStorage.getItem(storageKey(seriesId));
      if (raw) return JSON.parse(raw);
    } catch (_) { /* ignore */ }
    return {};
  }

  /**
   * Persist the full read-state object for a series to localStorage.
   * @param {number|string} seriesId
   * @param {Object} state
   */
  function saveReadState(seriesId, state) {
    localStorage.setItem(storageKey(seriesId), JSON.stringify(state));
  }

  /**
   * @param {number|string} seriesId
   * @param {number|string} chapterId
   * @param {'unread'|'started'|'finished'} status
   * @param {number} [page] last viewed page
   */
  function setChapterStatus(seriesId, chapterId, status, page) {
    const state = getReadState(seriesId);
    state['ch_' + chapterId] = { status, page: page || 1, updated: new Date().toISOString() };
    saveReadState(seriesId, state);
  }

  /** @returns {{status:string, page:number}|null} */
  function getChapterStatus(seriesId, chapterId) {
    const state = getReadState(seriesId);
    return state['ch_' + chapterId] || null;
  }

  /**
   * Apply read-status CSS classes (`ch-unread`, `ch-started`, `ch-finished`)
   * to each chapter list item based on the saved read state.
   * @param {number|string} seriesId
   */
  function applyReadStatus(seriesId) {
    const state = getReadState(seriesId);
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
  //  READER PAGE: progress tracking
  // ============================================================

  /**
   * Initialize reading progress tracking on the reader page.
   *
   * Observes `.reader-page` elements (which already have `data-page-idx`
   * from the server template).  Saves the last viewed page to localStorage
   * on `beforeunload` and auto-marks the chapter as finished when the last
   * page scrolls into view.
   *
   * @param {number|string} seriesId
   * @param {number|string} chapterId
   * @param {number} totalPages
   */
  function initReader(seriesId, chapterId, totalPages) {
    // Mark as started
    setChapterStatus(seriesId, chapterId, 'started', 1);

    const pages = document.querySelectorAll('.reader-page[data-page-idx]');
    if (pages.length === 0) return;
    let lastVisible = 1;
    let finishedMarked = false;

    const observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          let idx = parseInt(entry.target.getAttribute('data-page-idx'), 10);
          if (idx > lastVisible) {
            lastVisible = idx;
          }
        }
      });
    }, { threshold: 0.4 });

    pages.forEach(function (p) { observer.observe(p); });

    // Save progress on unload
    function saveProgress() {
      setChapterStatus(seriesId, chapterId, 'started', lastVisible);
    }
    window.addEventListener('beforeunload', saveProgress);

    // Auto-mark finished when last page is reached
    function checkFinished() {
      if (finishedMarked) return;
      if (lastVisible >= totalPages) {
        setChapterStatus(seriesId, chapterId, 'finished', totalPages);
        finishedMarked = true;
          let badge = document.createElement('span');
        badge.className = 'completed-badge';
        badge.textContent = '✓ Read';
        let target = document.querySelector('.reader-topbar-chapter');
        if (target) target.appendChild(badge);
      }
    }

    // Check on scroll and after images load
    window.addEventListener('scroll', function () {
      if (window._finishCheck) clearTimeout(window._finishCheck);
      window._finishCheck = setTimeout(checkFinished, 500);
    });

    let loaded = 0;
    pages.forEach(function (p) {
      var img = p.querySelector('img');
      if (img) {
        img.addEventListener('load', function () {
          loaded++;
          if (loaded >= pages.length) setTimeout(checkFinished, 300);
        });
      }
    });
  }

  // ============================================================
  //  READER: page navigation & UX
  // ============================================================

  /**
   * Initialize reader UX: page navigation via click zones, keyboard arrows,
   * scroll-aware topbar visibility, and a fading hint for first-time readers.
   */
  function initReaderUX() {
    const images = document.getElementById('reader-images');
    if (!images) return;
    const pages = images.querySelectorAll('.reader-page');
    if (pages.length === 0) return;

    const topbar = document.getElementById('reader-topbar');
    const pageIndicator = document.getElementById('reader-page-indicator');
    const clickLeft = document.getElementById('reader-click-left');
    const clickRight = document.getElementById('reader-click-right');

    let currentPage = 1;
    const totalPages = pages.length;

    function updateIndicator() {
      if (pageIndicator) {
        pageIndicator.textContent = currentPage + ' / ' + totalPages;
      }
    }

    function goToPage(idx) {
      if (idx < 1 || idx > totalPages) return;
      currentPage = idx;
      updateIndicator();
      pages[idx - 1].scrollIntoView({ behavior: 'instant', block: 'start' });
    }

    function nextPage() {
      if (currentPage < totalPages) {
        goToPage(currentPage + 1);
      }
    }

    function prevPage() {
      if (currentPage > 1) {
        goToPage(currentPage - 1);
      }
    }

    // Track visible page on scroll
    const pageObserver = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          const idx = parseInt(entry.target.getAttribute('data-page-idx'), 10);
          if (idx > 0) {
            currentPage = idx;
            updateIndicator();
          }
        }
      });
    }, { threshold: 0.3 });

    pages.forEach(function (p) { pageObserver.observe(p); });

    // Click zones
    if (clickLeft) clickLeft.addEventListener('click', prevPage);
    if (clickRight) clickRight.addEventListener('click', nextPage);

    // Keyboard nav
    document.addEventListener('keydown', function (e) {
      if (e.key === 'ArrowLeft') prevPage();
      if (e.key === 'ArrowRight') nextPage();
    });

    // Show/hide topbar on scroll
    let lastScrollY = window.scrollY;
    let topbarTimer = null;

    function showTopbar() {
      if (topbar) topbar.classList.add('visible');
      if (topbarTimer) clearTimeout(topbarTimer);
      topbarTimer = setTimeout(function () {
        if (topbar) topbar.classList.remove('visible');
      }, 3000);
    }

    window.addEventListener('scroll', function () {
      const sy = window.scrollY;
      if (sy < lastScrollY - 20 || sy < 60) {
        showTopbar();
      } else if (sy > lastScrollY + 60) {
        if (topbar) topbar.classList.remove('visible');
      }
      lastScrollY = sy;
    });

    // Show on mouse near top
    document.addEventListener('mousemove', function (e) {
      if (e.clientY < 80) showTopbar();
    });

    // Hide hint on first interaction
    const hint = document.getElementById('reader-hint');
    function hideHint() {
      if (hint) hint.classList.add('hidden');
      document.removeEventListener('click', hideHint);
      document.removeEventListener('keydown', hideHint);
    }
    document.addEventListener('click', hideHint);
    document.addEventListener('keydown', hideHint);

    updateIndicator();
  }

  // ============================================================
  //  BOOT: run on page load
  // ============================================================

  document.addEventListener('DOMContentLoaded', function () {
    // Reader UX setup
    initReaderUX();
    // --- Series detail page ---
    const chapterList = document.getElementById('chapter-list');
    if (chapterList) {
      const seriesId = chapterList.getAttribute('data-series-id');

      // Build source toggles
      const sources = JSON.parse(chapterList.getAttribute('data-sources') || '[]');
      const toggleContainer = document.getElementById('source-toggles');
      if (toggleContainer && sources.length > 1) {
        sources.forEach(src => {
          const label = document.createElement('label');
          label.className = 'source-toggle';
          const cb = document.createElement('input');
          cb.type = 'checkbox';
          cb.value = src;
          cb.addEventListener('change', () => toggleSource(src));
          label.appendChild(cb);
          label.appendChild(document.createTextNode(' ' + src));
          toggleContainer.appendChild(label);
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

    // --- Reader page ---
    const readerEl = document.getElementById('reader-track');
    if (readerEl) {
      const seriesId = readerEl.getAttribute('data-series-id');
      const chapterId = readerEl.getAttribute('data-chapter-id');
      const totalPages = parseInt(readerEl.getAttribute('data-total-pages') || '0', 10);
      if (seriesId && chapterId) {
        initReader(seriesId, chapterId, totalPages);
      }
    }

    // --- Reader: scroll-to-resume ---
    const readerTrack = document.getElementById('reader-track');
    const resumeTarget = document.getElementById('resume-target');
    if (readerTrack && resumeTarget) {
      const seriesId = readerTrack.getAttribute('data-series-id');
      const chapterId = readerTrack.getAttribute('data-chapter-id');
      const saved = getChapterStatus(seriesId, chapterId);
      if (saved && saved.page && saved.page > 1) {
        resumeTarget.setAttribute('data-page', String(saved.page));
        const page = saved.page;
        // The server template already sets data-page-idx on .reader-page divs
        const el = document.querySelector('.reader-page[data-page-idx="' + page + '"]');
        if (el) {
          el.scrollIntoView({ behavior: 'instant', block: 'start' });
        }
      }
    }
  });

})();
