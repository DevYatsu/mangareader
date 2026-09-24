/**
 * MangaRead — Reader 2.0 client module.
 *
 * Owns the reader surface end to end: preference resolution and persistence,
 * the controls sheet, page navigation, input (zones / keybinds / swipe /
 * auto-next), comfort chrome, notes, and page states.
 *
 * Contract: docs/superpowers/specs/2026-09-20-reader2-design-contract.md
 *
 * Interfaces produced (later work depends on these exact names):
 *   window.MangaReader = { prefs, save, openSheet, closeSheet, goToPage, currentPage }
 *
 * Interfaces consumed:
 *   - the Task 4 DOM in website/src/routes/reader.rs
 *   - window.MangaReadState  (app.js) — the single progress write path
 *   - window.MangaSync       (auth.js) — pushProgress / loggedIn
 *   - GET/POST /api/reader-prefs, GET/POST /api/notes
 *
 * Reader-facing surfaces expose no scraping concepts.
 */
(function () {
  'use strict';

  // ============================================================
  //  CONSTANTS
  // ============================================================

  var LS_PREFS = 'mangaread_reader_prefs';
  var LS_SERIES_PREFIX = 'mangaread_series_prefs_';
  var LS_LOCAL = 'mangaread_reader_local';
  var SS_DRAFT_PREFIX = 'mangaread_note_draft_';
  var SS_AUTONEXT_PREFIX = 'mangaread_autonext_cancel_';
  var SS_OPEN_LAST = 'mangaread_reader_open_last';

  var LAYOUTS = ['single', 'double', 'strip', 'vertical'];
  var DIRECTIONS = ['ltr', 'rtl'];
  var FITS = ['', 'height', 'screen'];
  var BGS = ['black', 'white', 'sepia', 'custom'];
  var DIM_DELAYS = [0, 15, 30, 60];

  /**
   * Curated reading styles. Each preset only adjusts the choices it owns;
   * page order, tone, notes, and progress remain independent so existing
   * stored preferences are not lost when someone changes pace.
   */
  var PRESETS = {
    flow: { layout: 'strip' },
    page: { layout: 'single' },
    spread: { layout: 'double' }
  };

  var ACTION_IDS = [
    'page.next', 'page.prev', 'chapter.next', 'chapter.prev',
    'ui.toggle', 'ui.focus', 'ui.fitCycle', 'layout.cycle', 'note.add'
  ];

  var DEFAULT_KEYBINDS = {
    'page.next': ['ArrowRight', 'ArrowDown', 'Space', 'PageDown'],
    'page.prev': ['ArrowLeft', 'ArrowUp', 'PageUp'],
    'chapter.next': ['Shift+ArrowRight', 'KeyN'],
    'chapter.prev': ['Shift+ArrowLeft', 'KeyP'],
    'ui.toggle': ['KeyS', 'Escape', '?'],
    'ui.focus': ['KeyF'],
    'ui.fitCycle': ['KeyW'],
    'layout.cycle': ['KeyL'],
    'note.add': ['KeyM']
  };

  // ============================================================
  //  SMALL UTILITIES
  // ============================================================

  /** @returns {Element|null} */
  function $(sel, root) {
    return (root || document).querySelector(sel);
  }

  /** @returns {Element[]} a real Array (NodeList has no `.map` in old engines) */
  function $$(sel, root) {
    return Array.prototype.slice.call((root || document).querySelectorAll(sel));
  }

  function clampInt(v, lo, hi, fallback) {
    var n = parseInt(v, 10);
    if (!isFinite(n)) return fallback;
    return n < lo ? lo : (n > hi ? hi : n);
  }

  function clampNum(v, lo, hi, fallback) {
    var n = parseFloat(v);
    if (!isFinite(n)) return fallback;
    return n < lo ? lo : (n > hi ? hi : n);
  }

  /** Keep `value` when it is in `allowed`, otherwise use `fallback`. */
  function allowed(value, list, fallback) {
    return list.indexOf(value) >= 0 ? value : fallback;
  }

  function isHexColor(v) {
    return typeof v === 'string' && /^#[0-9a-fA-F]{6}$/.test(v);
  }

  function customTone(color) {
    var hex = String(color || '').replace('#', '');
    if (hex.length !== 6) return null;
    var channels = [0, 2, 4].map(function (i) {
      var value = parseInt(hex.slice(i, i + 2), 16) / 255;
      return value <= 0.04045 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4);
    });
    var luminance = channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
    return luminance > 0.42 ? {
      chromeBg: 'rgba(255, 255, 255, 0.96)',
      chromeFg: '#111111',
      chromeBorder: 'rgba(17, 17, 17, 0.14)',
      surface: 'rgba(255, 255, 255, 0.96)',
      surfaceStrong: '#f3f4f6',
      accent: '#92400e',
      accentFg: '#ffffff',
      error: '#b91c1c'
    } : {
      chromeBg: 'rgba(10, 10, 10, 0.96)',
      chromeFg: '#ffffff',
      chromeBorder: 'rgba(255, 255, 255, 0.14)',
      surface: 'rgba(18, 18, 18, 0.96)',
      surfaceStrong: '#1f1f1f',
      accent: '#f59e0b',
      accentFg: '#111111',
      error: '#fca5a5'
    };
  }

  function readerTone(p) {
    if (p.bg === 'white') return {
      chromeBg: 'rgba(255, 255, 255, 0.96)', chromeFg: '#111111',
      chromeBorder: 'rgba(17, 17, 17, 0.14)', surface: 'rgba(255, 255, 255, 0.96)',
      surfaceStrong: '#f3f4f6', accent: '#92400e', accentFg: '#ffffff', error: '#b91c1c'
    };
    if (p.bg === 'sepia') return {
      chromeBg: 'rgba(244, 236, 216, 0.97)', chromeFg: '#1f1a12',
      chromeBorder: 'rgba(31, 26, 18, 0.16)', surface: 'rgba(244, 236, 216, 0.97)',
      surfaceStrong: '#e6d7b8', accent: '#7c2d12', accentFg: '#ffffff', error: '#991b1b'
    };
    if (p.bg === 'custom') return customTone(p.bgCustom) || readerTone({ bg: 'black' });
    return {
      chromeBg: 'rgba(10, 10, 10, 0.96)', chromeFg: '#ffffff',
      chromeBorder: 'rgba(255, 255, 255, 0.14)', surface: 'rgba(18, 18, 18, 0.96)',
      surfaceStrong: '#1f1f1f', accent: '#f59e0b', accentFg: '#111111', error: '#fca5a5'
    };
  }

  function applyReaderTone(p) {
    var body = document.body;
    if (!body) return;
    var tone = readerTone(p);
    body.style.setProperty('--r-chrome-bg', tone.chromeBg);
    body.style.setProperty('--r-chrome-fg', tone.chromeFg);
    body.style.setProperty('--r-chrome-border', tone.chromeBorder);
    body.style.setProperty('--r-surface', tone.surface);
    body.style.setProperty('--r-surface-strong', tone.surfaceStrong);
    body.style.setProperty('--r-accent', tone.accent);
    body.style.setProperty('--r-accent-fg', tone.accentFg);
    body.style.setProperty('--r-error', tone.error);
  }

  function readJSON(storage, key, fallback) {
    try {
      var raw = storage.getItem(key);
      if (raw) return JSON.parse(raw);
    } catch (_) { /* ignore malformed / unavailable storage */ }
    return fallback;
  }

  function writeJSON(storage, key, value) {
    try {
      storage.setItem(key, JSON.stringify(value));
      return true;
    } catch (_) {
      return false;
    }
  }

  function readString(storage, key) {
    try { return storage.getItem(key); } catch (_) { return null; }
  }

  function writeString(storage, key, value) {
    try { storage.setItem(key, value); } catch (_) { /* ignore */ }
  }

  function removeKey(storage, key) {
    try { storage.removeItem(key); } catch (_) { /* ignore */ }
  }

  // ============================================================
  //  PREFS SHAPE (client mirror of ReaderPrefs::normalized)
  // ============================================================

  /** Two physical boundaries from the left, 10..90, at least 10 apart. */
  function normalizeZones(zones) {
    var a = clampInt(zones && zones[0], 10, 80, 30);
    var b = clampInt(zones && zones[1], a + 10, 90, 70);
    if (b < a + 10) b = a + 10;
    if (b > 90) { b = 90; a = Math.min(a, 80); if (b < a + 10) a = b - 10; }
    return [a, b];
  }

  function normalizeKeybinds(kb) {
    var out = {};
    for (var i = 0; i < ACTION_IDS.length; i++) {
      var action = ACTION_IDS[i];
      var raw = kb && kb[action];
      var keys = [];
      if (Object.prototype.toString.call(raw) === '[object Array]') {
        for (var j = 0; j < raw.length; j++) {
          var k = raw[j];
          if (typeof k !== 'string') continue;
          k = k.trim();
          if (!k || k.length > 32) continue;
          if (keys.indexOf(k) >= 0) continue;
          keys.push(k);
          if (keys.length >= 4) break;
        }
      }
      if (keys.length === 0) keys = DEFAULT_KEYBINDS[action].slice();
      out[action] = keys;
    }
    return out;
  }

  function defaultPrefs() {
    return normalizePrefs({});
  }

  function normalizePrefs(p) {
    p = p || {};
    var o = {};
    o.layout = allowed(p.layout, LAYOUTS, 'strip');
    o.direction = allowed(p.direction, DIRECTIONS, 'ltr');
    o.fit = allowed(p.fit, FITS, '');
    o.maxW = clampInt(p.maxW, 400, 1400, 700);
    o.gap = clampInt(p.gap, 0, 64, 0);
    o.bg = allowed(p.bg, BGS, 'black');
    o.bgCustom = isHexColor(p.bgCustom) ? p.bgCustom.toLowerCase() : '#0a0a0a';
    o.frame = !!p.frame;
    o.doubleCover = !!p.doubleCover;
    o.autoNext = !!p.autoNext;
    o.autoPrev = !!p.autoPrev;
    o.hideDelay = clampInt(p.hideDelay, 0, 10, 3);
    o.dimDelay = DIM_DELAYS.indexOf(Number(p.dimDelay)) >= 0 ? Number(p.dimDelay) : 0;
    o.dimOpacity = clampNum(p.dimOpacity, 0.2, 0.8, 0.5);
    o.swipeThreshold = clampInt(p.swipeThreshold, 40, 160, 70);
    o.clickZones = normalizeZones(p.clickZones);
    o.keybinds = normalizeKeybinds(p.keybinds);
    o.showNotes = p.showNotes === undefined ? true : !!p.showNotes;
    return o;
  }

  function clonePrefs(p) {
    return normalizePrefs(JSON.parse(JSON.stringify(p)));
  }

  // ============================================================
  //  STATE
  // ============================================================

  var state = {
    isReader: false,
    track: null,
    seriesId: 0,
    chapterId: 0,
    totalPages: 0,
    currentPage: 1,
    primeIdx: 0,
    loggedIn: false,
    basePrefs: null,       // server blob (logged in) or local blob (anon)
    seriesPrefs: null,     // full per-series override, or null
    local: null,           // device-local state (mangaread_reader_local)
    notes: [],
    sheetOpen: false,
    keymap: null,          // combo -> action
    hideTimer: null,
    dimTimer: null,
    dimmed: false,
    progressTimer: null,
    progressHydrationComplete: false,
    serverProgressSeen: false,
    prefsSaveTimer: null,
    pendingServerPrefs: null,
    serverWriteId: 0,
    writeFailures: 0,
    offlineHooked: false,
    finished: false,
    autoNextTimer: null,
    countdownValue: 3,
    stripAutoNextTimer: null,
    indicatorAnnouncementTimer: null,
    pendingIndicatorText: '',
    lastIndicatorAt: 0,
    resumePage: 0,
    serverResumePage: 0,
    scrollTimer: null,
    longPressTimer: null,
    retried: {},
    navigated: false
  };

  function isLoggedIn() {
    if (state.loggedIn) return true;
    try {
      if (window.MangaSync && typeof window.MangaSync.loggedIn === 'function') {
        return window.MangaSync.loggedIn();
      }
    } catch (_) { /* ignore */ }
    return !!(document.body && document.body.dataset && document.body.dataset.userId);
  }

  function seriesKey() {
    return LS_SERIES_PREFIX + state.seriesId;
  }

  /** Effective prefs = series override (full replacement) or the base blob. */
  function effectivePrefs() {
    return state.seriesPrefs || state.basePrefs || defaultPrefs();
  }

  function loadDeviceLocal() {
    var d = readJSON(localStorage, LS_LOCAL, {}) || {};
    if (!d || typeof d !== 'object') d = {};
    if (typeof d.scrollPositions !== 'object' || !d.scrollPositions) d.scrollPositions = {};
    if (typeof d.zoom !== 'number' || !isFinite(d.zoom) || d.zoom <= 0) d.zoom = 1;
    d.resumeBadgeSeen = !!d.resumeBadgeSeen;
    d.hintSeen = !!d.hintSeen;
    d.lastUsedLayout = typeof d.lastUsedLayout === 'string' ? d.lastUsedLayout : '';
    state.local = d;
    return d;
  }

  function saveDeviceLocal() {
    if (state.local) writeJSON(localStorage, LS_LOCAL, state.local);
  }

  // ============================================================
  //  PREF RESOLUTION (contract §2.3 / §3.4)
  // ============================================================

  /** The `#reader-prefs` JSON island is the pre-first-paint server blob. */
  function serverIslandPrefs() {
    var el = document.getElementById('reader-prefs');
    if (!el) return null;
    var parsed = null;
    try { parsed = JSON.parse(el.textContent || '{}'); } catch (_) { parsed = null; }
    return parsed && typeof parsed === 'object' ? parsed : null;
  }

  /**
   * Resolution order: series override (local) > server blob (logged in) >
   * local (anon) > defaults. The island seeds the first paint synchronously;
   * a logged-in session then re-reads the server blob so a change made on
   * another device is not masked by a stale render.
   */
  function resolvePrefs() {
    var island = serverIslandPrefs();
    if (isLoggedIn()) {
      state.basePrefs = normalizePrefs(island || {});
    } else {
      var localBlob = readJSON(localStorage, LS_PREFS, null);
      // Anon: local blob wins over the island (the island is defaults for anon).
      state.basePrefs = normalizePrefs(localBlob || island || {});
    }

    var override = readJSON(localStorage, seriesKey(), null);
    if (override && typeof override === 'object') {
      state.seriesPrefs = normalizePrefs(override);
    } else {
      state.seriesPrefs = null;
    }

    var remember = $('#reader-remember-series');
    if (remember) remember.checked = !!state.seriesPrefs;

    // The anon→account merge is sequenced AFTER the server read so a late
    // GET response cannot clobber the merged blob in memory (finding 1).
    if (isLoggedIn()) fetchServerPrefs(maybeMergeLocalPrefs);
  }

  /**
   * Logged-in only: the server blob is authoritative. `after` receives the
   * server prefs object once known, or `null` when the read failed.
   */
  function fetchServerPrefs(after) {
    try {
      fetch('/api/reader-prefs', { headers: { 'Accept': '*/*' }, credentials: 'same-origin' })
        .then(function (r) { return r.ok ? r.json() : null; })
        .then(function (data) {
          if (data && data.prefs) {
            state.basePrefs = normalizePrefs(data.prefs);
            applyPrefs();
          }
          if (after) after(data && data.prefs ? data.prefs : null);
        })
        .catch(function () {
          if (after) after(null);
        });
    } catch (_) {
      if (after) after(null);
    }
  }

  function progressTime(value) {
    if (!value) return 0;
    var raw = String(value);
    if (raw.indexOf(' ') >= 0 && raw.indexOf('T') < 0) raw = raw.replace(' ', 'T') + 'Z';
    var parsed = Date.parse(raw);
    return isFinite(parsed) ? parsed : 0;
  }

  /**
   * Hydrate every progress row in the series response without a server write.
   * The local read-state map remains the resume source; finished is monotonic so
   * an older started row can never downgrade a newer finished chapter locally.
   */
  function hydrateServerProgress() {
    return new Promise(function (resolve) {
      if (!isLoggedIn() || !state.seriesId || !window.MangaReadState ||
          typeof window.MangaReadState.getReadState !== 'function' ||
          typeof window.MangaReadState.saveReadState !== 'function') {
        resolve();
        return;
      }
      try {
        fetch('/api/progress?series_id=' + encodeURIComponent(state.seriesId), {
          headers: { 'Accept': '*/*' },
          credentials: 'same-origin'
        })
          .then(function (r) { return r.ok ? r.json() : null; })
          .then(function (data) {
            var chapters = data && data.chapters;
            if (!chapters || Object.prototype.toString.call(chapters) !== '[object Object]') {
              resolve();
              return;
            }

            var readState = window.MangaReadState.getReadState(state.seriesId) || {};
            var changed = false;
            var currentLocal = window.MangaReadState.getChapterStatus
              ? window.MangaReadState.getChapterStatus(state.seriesId, state.chapterId)
              : null;
            state.finished = !!(currentLocal && currentLocal.status === 'finished');

            Object.keys(chapters).forEach(function (chapterKey) {
              var row = chapters[chapterKey];
              if (!row || typeof row !== 'object') return;
              var chapterId = parseInt(chapterKey, 10);
              var page = parseInt(row.page, 10);
              var status = row.status === 'finished' || row.status === 'started' || row.status === 'unread'
                ? row.status : 'started';
              if (!isFinite(chapterId) || chapterId < 1 || !isFinite(page) || page < 1) return;

              var storageKey = 'ch_' + chapterId;
              var local = readState[storageKey] || null;
              var serverAt = progressTime(row.updated_at);
              var localAt = progressTime(local && local.updated);
              var isCurrent = chapterId === state.chapterId;
              if (isCurrent) state.serverProgressSeen = true;
              if (isCurrent && status === 'finished') state.finished = true;

              // Ties follow the server's last-write-wins rule, except that a
              // finished local row is never replaced by a started server row.
              var wouldDowngrade = !!(local && local.status === 'finished' && status !== 'finished');
              if (!local || (serverAt >= localAt && !wouldDowngrade)) {
                readState[storageKey] = {
                  status: status,
                  page: page,
                  updated: row.updated_at || (local && local.updated) || ''
                };
                changed = true;
                if (isCurrent) {
                  state.serverResumePage = page;
                  var target = document.getElementById('resume-target');
                  if (target) target.setAttribute('data-page', String(page));
                }
              }
            });

            if (changed) window.MangaReadState.saveReadState(state.seriesId, readState);
            state.progressHydrationComplete = true;
            resolve();
          })
          .catch(function () { resolve(); });
      } catch (_) { resolve(); }
    });
  }
  // ============================================================
  //  ANON → ACCOUNT PREF MERGE (contract §3.4 / spec §16, finding 1)
  // ============================================================

  /** One-shot flag, mirroring `mangaread_migrated` in auth.js. */
  function prefsMigratedKey() {
    var uid = document.body && document.body.dataset ? document.body.dataset.userId : '';
    return 'mangaread_reader_prefs_migrated' + (uid ? '_' + uid : '');
  }

  /** True when a blob carries nothing the user chose (ReaderPrefs defaults). */
  function isDefaultBlob(raw) {
    if (!raw || typeof raw !== 'object') return true;
    var p = normalizePrefs(raw);
    var d = defaultPrefs();
    return p.layout === d.layout &&
      p.direction === d.direction &&
      p.fit === d.fit &&
      p.maxW === d.maxW &&
      p.gap === d.gap &&
      p.bg === d.bg &&
      p.bgCustom === d.bgCustom &&
      p.frame === d.frame &&
      p.doubleCover === d.doubleCover &&
      p.autoNext === d.autoNext &&
      p.autoPrev === d.autoPrev &&
      p.hideDelay === d.hideDelay &&
      p.dimDelay === d.dimDelay &&
      p.dimOpacity === d.dimOpacity &&
      p.swipeThreshold === d.swipeThreshold &&
      p.clickZones.join(',') === d.clickZones.join(',') &&
      JSON.stringify(p.keybinds) === JSON.stringify(d.keybinds) &&
      p.showNotes === d.showNotes;
  }

  /**
   * Mirror of auth.js's one-shot `mangaread_migrated` pattern, for reader
   * prefs. An anonymous reader's local blob is pushed to the account once, but
   * only when the account has no real preferences of its own; a non-default
   * server blob always wins and is never overwritten.
   *
   * `serverBlob` is null when the server read failed — in that case the merge
   * is skipped and retried on the next load, so a network blip can never
   * overwrite an account's settings.
   *
   * @param {Object|null} serverBlob prefs as returned by GET /api/reader-prefs
   */
  function maybeMergeLocalPrefs(serverBlob) {
    if (!isLoggedIn() || !serverBlob) return;
    var flagKey = prefsMigratedKey();
    try {
      if (localStorage.getItem(flagKey)) return;
    } catch (_) {
      return;
    }

    // The account already has real preferences: it wins, do not overwrite.
    if (!isDefaultBlob(serverBlob)) {
      try { localStorage.setItem(flagKey, '1'); } catch (_) { /* ignore */ }
      return;
    }

    var localBlob = readJSON(localStorage, LS_PREFS, null);
    if (!localBlob || typeof localBlob !== 'object') {
      try { localStorage.setItem(flagKey, '1'); } catch (_) { /* ignore */ }
      return;
    }

    var merged = normalizePrefs(localBlob);
    try {
      fetch('/api/reader-prefs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Accept': '*/*' },
        credentials: 'same-origin',
        body: JSON.stringify(merged)
      })
        .then(function (r) { return r.ok ? r.json() : null; })
        .then(function (data) {
          if (!data || !data.prefs) return; // leave the flag unset: retry later
          if (!state.seriesPrefs) {
            state.basePrefs = normalizePrefs(data.prefs);
            applyPrefs();
          }
          try { localStorage.setItem(flagKey, '1'); } catch (_) { /* ignore */ }
        })
        .catch(function () { /* local blob stays intact; retry on next load */ });
    } catch (_) { /* never block page load */ }
  }

  // ============================================================
  //  APPLYING PREFS TO THE DOM
  // ============================================================

  /** Map the existing layout contract to the public, three-mode surface. */
  function activePreset(p) {
    if (p.layout === 'double') return 'spread';
    if (p.layout === 'single' || p.layout === 'vertical') return 'page';
    return 'flow';
  }

  /**
   * A saved Spread remains stored, but phones render the safer Page surface.
   * Persistence still uses effectivePrefs(), so resizing back to a wide
   * viewport restores the saved Spread without rewriting the user's choice.
   */
  function layoutPrefs() {
    var p = effectivePrefs();
    if (window.innerWidth < 768 && p.layout === 'double') {
      var safe = clonePrefs(p);
      safe.layout = 'single';
      safe.fit = 'height';
      safe.maxW = 900;
      safe.doubleCover = false;
      return safe;
    }
    return p;
  }

  function applyPrefs() {
    var p = layoutPrefs();
    var b = document.body;
    applyReaderTone(p);
    // Controller ruling: fit modes do not apply to `strip`. The stored fit
    // pref is preserved but never reaches the body on that layout.
    var activeFit = p.layout === 'strip' ? '' : p.fit;

    b.setAttribute('data-r-layout', p.layout);
    b.setAttribute('data-r-preset', activePreset(p));
    b.setAttribute('data-r-direction', p.direction);
    if (activeFit) b.setAttribute('data-r-fit', activeFit);
    else b.removeAttribute('data-r-fit');
    b.setAttribute('data-r-bg', p.bg);
    if (p.bg === 'custom') b.style.setProperty('--r-bg-custom', p.bgCustom);
    if (p.frame) b.setAttribute('data-r-frame', 'on');
    else b.removeAttribute('data-r-frame');
    b.setAttribute('data-r-notes', p.showNotes ? 'on' : 'off');

    b.style.setProperty('--r-gap', p.gap + 'px');
    b.style.setProperty('--r-zoom', String((state.local && state.local.zoom) || 1));
    b.style.setProperty('--r-dim', String(state.dimmed ? p.dimOpacity : 0));
    // Fit wins over width but the stored width is never cleared (contract §2.2).
    b.style.setProperty('--r-max-w', activeFit ? '100%' : p.maxW + 'px');
    if (state.track) {
      if (p.autoNext) state.track.setAttribute('data-auto-next', '1');
      else state.track.removeAttribute('data-auto-next');
      if (p.autoPrev) state.track.setAttribute('data-auto-prev', '1');
      else state.track.removeAttribute('data-auto-prev');
    }

    syncControls();
    applyZoneWidths();
    rebuildKeymap();
    renderLayout();
    maybeAutoNextStrip();
  }

  /** Two-way bind the compact preference surface to the effective prefs. */
  function syncControls() {
    var p = layoutPrefs();
    setSegment('direction', p.direction);
    setSegment('bg', p.bg);

    var selected = activePreset(p);
    var presetBtns = $$('.reader-preset-btn[data-r-preset]');
    for (var i = 0; i < presetBtns.length; i++) {
      var btn = presetBtns[i];
      var id = btn.getAttribute('data-r-preset');
      btn.setAttribute('aria-pressed', id === selected ? 'true' : 'false');

      // A book spread on a phone halves already-small artwork, so it remains
      // a saved value but cannot be selected there.
      var spreadTooNarrow = id === 'spread' && window.innerWidth < 768;
      btn.disabled = spreadTooNarrow;
      btn.setAttribute('aria-disabled', spreadTooNarrow ? 'true' : 'false');
      if (spreadTooNarrow) btn.setAttribute('title', 'Spread needs a wider screen');
      else btn.removeAttribute('title');
    }
    var spreadNote = document.getElementById('reader-spread-note');
    if (spreadNote) spreadNote.hidden = window.innerWidth >= 768;

    var focusOn = document.body && document.body.getAttribute('data-r-focus') === '1';
    var focusControls = $$('[data-focus-toggle]');
    for (var f = 0; f < focusControls.length; f++) {
      focusControls[f].setAttribute('aria-pressed', focusOn ? 'true' : 'false');
    }
  }

  function setSegment(pref, value) {
    var btns = $$('.reader-seg-btn[data-pref="' + pref + '"]');
    for (var i = 0; i < btns.length; i++) {
      var pressed = btns[i].getAttribute('data-value') === value;
      btns[i].setAttribute('aria-pressed', pressed ? 'true' : 'false');
    }
  }

  /** Map the stored physical boundaries onto the three zone buttons. */
  function applyZoneWidths() {
    var zones = document.getElementById('reader-zones');
    if (!zones) return;
    var z = effectivePrefs().clickZones;
    var prev = $('.reader-zone[data-zone="prev"]', zones);
    var menu = $('.reader-zone[data-zone="menu"]', zones);
    var next = $('.reader-zone[data-zone="next"]', zones);
    if (prev) { prev.style.left = '0%'; prev.style.width = z[0] + '%'; }
    if (menu) { menu.style.left = z[0] + '%'; menu.style.width = (z[1] - z[0]) + '%'; }
    if (next) { next.style.left = z[1] + '%'; next.style.width = (100 - z[1]) + '%'; }
  }

  // ============================================================
  //  PREF WRITE PATH
  // ============================================================

  /**
   * Apply a change to the effective prefs object and persist it.
   *
   * With "Remember for this series" checked the change lands in the series
   * override (a full replacement); otherwise it lands in the base blob.
   * Logged-in users additionally write through to `POST /api/reader-prefs`.
   *
   * @param {Object} patch one or more prefs keys in wire (camelCase) shape
   */
  function applyPrefPatch(patch, immediate) {
    var prefs = clonePrefs(effectivePrefs());
    for (var key in patch) {
      if (Object.prototype.hasOwnProperty.call(patch, key)) prefs[key] = patch[key];
    }
    prefs = normalizePrefs(prefs);

    if (state.seriesPrefs) {
      // The per-series override is device-local for everyone (§3.4); it has no
      // server key, so only localStorage carries it.
      state.seriesPrefs = prefs;
      writeJSON(localStorage, seriesKey(), prefs);
    } else {
      state.basePrefs = prefs;
      if (isLoggedIn()) {
        // Contract §9: `mangaread_reader_prefs` is the anonymous blob; when
        // logged in the server blob is authoritative, so write through only.
        queueServerSave(prefs, immediate);
      } else {
        writeJSON(localStorage, LS_PREFS, prefs);
      }
    }

    applyPrefs();
    updateNavigationHint();
    if (state.local && state.local.lastUsedLayout !== prefs.layout) {
      state.local.lastUsedLayout = prefs.layout;
      saveDeviceLocal();
    }
    // A pref change is user input: re-arm the idle timers against the new
    // delays and reveal the chrome (contract §5.1/§5.2).
    resetDimTimer();
    showChrome();
  }

  function sendServerPrefs(prefs) {
    var writeId = ++state.serverWriteId;
    try {
      fetch('/api/reader-prefs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Accept': '*/*' },
        credentials: 'same-origin',
        body: JSON.stringify(prefs)
      })
        .then(function (r) { return r.ok ? r.json() : null; })
        .then(function (data) {
          if (!data || !data.prefs) return;
          // The server echoes the normalized blob; adopt it only when no
          // series override is active, so a local override still wins.
          if (!state.seriesPrefs && writeId === state.serverWriteId) {
            state.basePrefs = normalizePrefs(data.prefs);
            applyPrefs();
          }
        })
        .catch(function () { /* localStorage stays authoritative */ });
    } catch (_) { /* ignore */ }
  }

  function queueServerSave(prefs, immediate) {
    state.pendingServerPrefs = prefs;
    if (state.prefsSaveTimer) clearTimeout(state.prefsSaveTimer);
    if (immediate) {
      state.prefsSaveTimer = null;
      var pending = state.pendingServerPrefs;
      state.pendingServerPrefs = null;
      sendServerPrefs(pending);
      return;
    }
    state.prefsSaveTimer = setTimeout(function () {
      state.prefsSaveTimer = null;
      var pending = state.pendingServerPrefs;
      state.pendingServerPrefs = null;
      if (pending) sendServerPrefs(pending);
    }, 400);
  }

  function flushPendingPrefs() {
    if (state.prefsSaveTimer) {
      clearTimeout(state.prefsSaveTimer);
      state.prefsSaveTimer = null;
    }
    var pending = state.pendingServerPrefs;
    state.pendingServerPrefs = null;
    if (pending) sendServerPrefs(pending);
  }

  /** Public `save` alias; merges when called with no argument. */
  function savePrefs(patch) {
    if (patch && typeof patch === 'object') applyPrefPatch(patch);
    else applyPrefPatch({});
  }

  function announceSetting(message) {
    var status = document.getElementById('reader-settings-status');
    if (!status) return;
    status.textContent = message;
  }

  function applyPreset(id) {
    var preset = PRESETS[id];
    if (!preset || (id === 'spread' && window.innerWidth < 768)) return;
    var preservedPage = clampInt(state.currentPage, 1, state.totalPages || 1, 1);
    var patch = {};
    for (var key in preset) {
      if (Object.prototype.hasOwnProperty.call(preset, key)) patch[key] = preset[key];
    }
    applyPrefPatch(patch, true);
    var nextPrefs = layoutPrefs();
    var nextPage = nextPrefs.layout === 'double' ? spreadStart(preservedPage, nextPrefs) : preservedPage;
    goToPage(nextPage);
    var label = id === 'flow' ? 'Flow' : (id === 'page' ? 'Page' : 'Spread');
    announceSetting(label + ' selected.');
  }

  function restoreDefaults() {
    // Restore only curated fields. Fit, width, gap, cover pairing, zones,
    // keybinds, auto-next, dim, and every other advanced value remain intact.
    var current = effectivePrefs();
    var recommendedTone = current.bg === 'custom' ? 'custom' : 'black';
    applyPrefPatch({
      layout: PRESETS.flow.layout,
      direction: 'ltr',
      bg: recommendedTone
    }, true);
    announceSetting('Recommended settings restored.');
  }

  /** `#reader-remember-series` writes or deletes the per-series override. */
  function setRememberSeries(on) {
    var remember = $('#reader-remember-series');
    if (on) {
      var snapshot = clonePrefs(effectivePrefs());
      state.seriesPrefs = snapshot;
      writeJSON(localStorage, seriesKey(), snapshot);
      saveDeviceLocal();
    } else {
      state.seriesPrefs = null;
      removeKey(localStorage, seriesKey());
    }
    if (remember) remember.checked = !!on;
    revealRememberNote(!!on);
    applyPrefs();
  }

  /**
   * Contract §3.4: the "Remembering for <title>" note shows only while the
   * checkbox is checked. The server renders it unconditionally. `hidden`
   * cannot be used here because reader.css sets `display: block`, which
   * outranks the UA `[hidden] { display: none }` rule, so the inline
   * `display` is toggled instead.
   */
  function revealRememberNote(on) {
    var note = $('.reader-series-remember-note');
    if (note) note.style.display = on ? '' : 'none';
  }

  // ============================================================
  //  CONTROLS SHEET (contract §3)
  // ============================================================

  var FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]),' +
    ' select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

  function openSheet() {
    state.sheetOpen = true;
    showChrome();
    var sheet = document.getElementById('reader-sheet');
    if (sheet) sheet.hidden = false;
    var btn = document.getElementById('reader-settings-btn');
    if (btn) btn.setAttribute('aria-expanded', 'true');
    // Move focus into the dialog so the trap is engaged and the reader lands
    // on a control, not on the page behind it.
    var close = $('.reader-sheet-close');
    if (close) close.focus();
    syncControls();
  }

  function closeSheet() {
    state.sheetOpen = false;
    var sheet = document.getElementById('reader-sheet');
    if (sheet) sheet.hidden = true;
    var focusOn = document.body && document.body.getAttribute('data-r-focus') === '1';
    if (!focusOn) showChrome();
    var focusTarget = focusOn
      ? document.getElementById('reader-focus-exit')
      : document.getElementById('reader-settings-btn');
    var btn = document.getElementById('reader-settings-btn');
    if (btn) btn.setAttribute('aria-expanded', 'false');
    if (focusTarget) focusTarget.focus();
    // The idle timers were suppressed while the sheet was open; re-arm them.
    resetDimTimer();
  }

  function toggleSheet() {
    if (state.sheetOpen) closeSheet();
    else openSheet();
  }

  /** Keep Tab inside `.reader-sheet-panel` while the sheet is open (§8.2). */
  function trapFocus(e) {
    if (!state.sheetOpen) return;
    var panel = $('.reader-sheet-panel');
    if (!panel) return;
    var items = $$(FOCUSABLE, panel).filter(function (el) {
      return el.offsetParent !== null || el === document.activeElement;
    });
    if (items.length === 0) return;
    var first = items[0];
    var last = items[items.length - 1];
    if (e.shiftKey && (document.activeElement === first || !panel.contains(document.activeElement))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }

  function selectSheetGroup() {
    var group = $('.reader-sheet-group[data-group="reading"]');
    if (group) group.hidden = false;
  }

  /**
   * Wire one `[data-pref]` control to the prefs write path.
   * Every control carrying `data-pref` is handled here; that is the test.
   */
  function bindPrefControl(el) {
    var pref = el.getAttribute('data-pref');
    if (!pref) return;

    if (el.classList.contains('reader-seg-btn')) {
      el.addEventListener('click', function () {
        if (el.disabled) return;
        if (el.getAttribute('aria-disabled') === 'true') return;
        var value = el.getAttribute('data-value');
        var patch = {};
        patch[pref] = value;
        if (el.getAttribute('aria-pressed') === 'true') return;
        applyPrefPatch(patch, pref === 'direction' || pref === 'bg');
      });
      return;
    }

  }

  function bindPrefControls() {
    var controls = $$('[data-pref]');
    for (var i = 0; i < controls.length; i++) {
      bindPrefControl(controls[i]);
    }

    var presets = $$('.reader-preset-btn[data-r-preset]');
    for (var p = 0; p < presets.length; p++) {
      presets[p].addEventListener('click', function (e) {
        e.stopPropagation();
        applyPreset(e.currentTarget.getAttribute('data-r-preset'));
      });
    }
  }

  function initSheet() {
    var btn = document.getElementById('reader-settings-btn');
    if (btn) {
      btn.addEventListener('click', function (e) {
        e.stopPropagation();
        toggleSheet();
      });
    }

    var focusBtn = document.getElementById('reader-focus-btn');
    if (focusBtn) {
      focusBtn.addEventListener('click', function (e) {
        e.stopPropagation();
        toggleFocus();
      });
    }
    var focusExit = document.getElementById('reader-focus-exit');
    if (focusExit) {
      focusExit.addEventListener('click', function (e) {
        e.stopPropagation();
        toggleFocus();
      });
    }

    // Backdrop, close button, and any `[data-r-close]` close the surface it
    // belongs to (sheet by default; the notes panel/composer handle their own).
    var closers = $$('[data-r-close]');
    for (var i = 0; i < closers.length; i++) {
      closers[i].addEventListener('click', function (e) {
        e.stopPropagation();
        var host = e.currentTarget.closest('#reader-notes-panel');
        if (host) { closeNotesPanel(); return; }
        if (e.currentTarget.closest('#reader-note-composer')) { closeComposer(false); return; }
        closeSheet();
      });
    }

    selectSheetGroup();

    var remember = $('#reader-remember-series');
    if (remember) {
      remember.addEventListener('change', function () { setRememberSeries(remember.checked); });
    }

    // Trap Tab inside the dialog and route the sheet's own `[data-action]`
    // affordances through the action dispatch.
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Tab') trapFocus(e);
    });
    var actionEls = $$('.reader-sheet-panel [data-action]');
    for (var ai = 0; ai < actionEls.length; ai++) {
      actionEls[ai].addEventListener('click', function (e) {
        e.stopPropagation();
        runAction(e.currentTarget.getAttribute('data-action'));
      });
    }

    revealRememberNote(!!state.seriesPrefs);
    bindPrefControls();
  }

  /**
   * Strip replaces the center tap zone with the HUD (contract §4.1, finding 6):
   * a long-press (≥400ms, matching the note long-press) opens the sheet, and
   * Enter/Space activate it for keyboard users. A short tap is deliberately
   * inert so it does not fight the hide-chrome behavior.
   */
  function initHudTrigger() {
    var hud = document.getElementById('reader-hud');
    if (!hud) return;
    var pressTimer = null;
    var longPressed = false;

    function beginPress() {
      longPressed = false;
      if (pressTimer) clearTimeout(pressTimer);
      pressTimer = setTimeout(function () {
        pressTimer = null;
        longPressed = true;
        openSheet();
      }, 400);
    }

    function endPress() {
      if (pressTimer) {
        clearTimeout(pressTimer);
        pressTimer = null;
      }
    }

    hud.addEventListener('pointerdown', beginPress);
    ['pointerup', 'pointercancel', 'pointerleave'].forEach(function (evt) {
      hud.addEventListener(evt, endPress);
    });
    // A short tap must not open the sheet; only suppress the click after a
    // long-press so no other handler double-fires.
    hud.addEventListener('click', function (e) {
      if (longPressed) {
        longPressed = false;
        e.stopPropagation();
      }
    });

    hud.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar') {
        e.preventDefault();
        e.stopPropagation();
        openSheet();
      }
    });
  }

  // ============================================================
  //  NAVIGATOR (contract §1)
  // ============================================================

  state.pageEls = [];
  state.pageByIndex = {};

  function indexPages() {
    state.pageEls = $$('.reader-page[data-page-idx]');
    state.pageEls.sort(function (a, b) {
      return parseInt(a.getAttribute('data-page-idx'), 10) - parseInt(b.getAttribute('data-page-idx'), 10);
    });
    state.pageByIndex = {};
    for (var i = 0; i < state.pageEls.length; i++) {
      state.pageByIndex[state.pageEls[i].getAttribute('data-page-idx')] = state.pageEls[i];
    }
    if (!state.totalPages) state.totalPages = state.pageEls.length;
  }

  function pageElByIndex(idx) {
    return state.pageByIndex[String(idx)] || null;
  }

  /** First page index of the spread containing `idx` (contract §1.1). */
  function spreadStart(idx, p) {
    if (p.layout !== 'double') return idx;
    if (p.doubleCover) {
      if (idx <= 1) return 1;
      return idx - ((idx - 2) % 2);
    }
    return idx - ((idx - 1) % 2);
  }

  function lastSpreadIndex(p) {
    var last = state.totalPages;
    if (p.layout !== 'double' || last <= 1) return last;
    return spreadStart(last, p);
  }

  function visibleIndexes(p) {
    if (p.layout === 'single') return [state.currentPage];
    if (p.layout === 'double') {
      var start = spreadStart(state.currentPage, p);
      // The cover (page 1 when `doubleCover` is set) renders alone (§1.1).
      if (p.doubleCover && start === 1) return [1];
      var out = [start];
      if (start + 1 <= state.totalPages) out.push(start + 1);
      return out;
    }
    return null; // strip/vertical: visibility is owned by the scroll container
  }

  function renderLayout() {
    if (!state.isReader) return;
    var p = layoutPrefs();
    syncZoneMode(p.layout);

    // `strip` and `vertical` are both scroll-driven; their pages must all stay
    // in the document so the container can scroll (contract §1.1: `vertical`
    // is "stacked in a scroll-snap container" and differs from `single` only
    // by "the scroll container and snap"). Hiding pages there collapses the
    // scroller to one screen and loses the scroll position (finding 2).
    if (p.layout === 'strip' || p.layout === 'vertical') {
      for (var s = 0; s < state.pageEls.length; s++) state.pageEls[s].hidden = false;
      updateIndicator();
      return;
    }

    var visible = visibleIndexes(p);
    for (var i = 0; i < state.pageEls.length; i++) {
      var idx = parseInt(state.pageEls[i].getAttribute('data-page-idx'), 10);
      state.pageEls[i].hidden = visible.indexOf(idx) < 0;
    }
    updateIndicator();
  }

  /** `#reader-zones[data-r-zone-mode]` — strip has no tap zones (§4.1). */
  function syncZoneMode(layout) {
    var zones = document.getElementById('reader-zones');
    if (!zones) return;
    zones.setAttribute('data-r-zone-mode', layout === 'strip' ? 'off' : 'three');
    var prev = $('.reader-zone[data-zone="prev"]', zones);
    var next = $('.reader-zone[data-zone="next"]', zones);
    var rtl = effectivePrefs().direction === 'rtl';
    if (prev) prev.setAttribute('aria-label', rtl ? 'Next page' : 'Previous page');
    if (next) next.setAttribute('aria-label', rtl ? 'Previous page' : 'Next page');
  }

  function scrollPageIntoView(el, block) {
    if (!el) return;
    var p = layoutPrefs();
    if (p.layout === 'vertical') {
      var container = document.getElementById('reader-images');
      if (container && container.scrollTo) {
        // Snap container: scroll within `#reader-images`, not the document.
        container.scrollTo({ top: el.offsetTop, behavior: 'auto' });
        return;
      }
    }
    try {
      el.scrollIntoView({ behavior: 'auto', block: block || 'start' });
    } catch (_) {
      el.scrollIntoView();
    }
  }

  function updateIndicator() {
    var text = state.currentPage + ' / ' + state.totalPages;
    var indicator = document.getElementById('reader-page-indicator');
    var hud = document.getElementById('reader-hud');
    if (hud) hud.textContent = text;
    if (!indicator) return;

    var now = Date.now();
    if (!state.lastIndicatorAt || now - state.lastIndicatorAt >= 500) {
      indicator.textContent = text;
      state.lastIndicatorAt = now;
      state.pendingIndicatorText = '';
      return;
    }
    state.pendingIndicatorText = text;
    if (state.indicatorAnnouncementTimer) return;
    state.indicatorAnnouncementTimer = setTimeout(function () {
      state.indicatorAnnouncementTimer = null;
      if (!state.pendingIndicatorText) return;
      indicator.textContent = state.pendingIndicatorText;
      state.pendingIndicatorText = '';
      state.lastIndicatorAt = Date.now();
    }, Math.max(1, 500 - (now - state.lastIndicatorAt)));
  }

  /**
   * Move to a page. In `strip` this only scrolls; in the paged layouts it also
   * swaps which pages are unhidden (contract §1.1).
   */
  function goToPage(idx) {
    if (!state.isReader) return;
    var total = state.totalPages || state.pageEls.length;
    if (total < 1) return;
    idx = clampInt(idx, 1, total, state.currentPage);
    // Any `goToPage` call is an explicit position: a resume target or a user
    // step, either of which is a legitimate place to persist on unload.
    state.navigated = true;
    state.currentPage = idx;

    var p = layoutPrefs();
    if (p.layout !== 'strip') renderLayout();
    scrollPageIntoView(pageElByIndex(idx) || pageElByIndex(spreadStart(idx, p)), 'start');
    updateIndicator();
    rememberScrollPosition();
    maybeEndOfChapter();
    prefetchAround(idx);
    // A page step is a progress change; the debounced write path keeps
    // `MangaSync.pushProgress` firing from exactly one place.
    scheduleProgress();
  }

  /** Contract §1.1 "current page" for the scroll-driven strip. */
  function computeStripPage() {
    var vh = window.innerHeight || 1;
    var best = state.currentPage;
    var bestTop = Infinity;
    for (var i = 0; i < state.pageEls.length; i++) {
      var rect = state.pageEls[i].getBoundingClientRect();
      var visible = Math.min(rect.bottom, vh) - Math.max(rect.top, 0);
      if (visible >= vh * 0.5 && rect.top < bestTop) {
        bestTop = rect.top;
        best = parseInt(state.pageEls[i].getAttribute('data-page-idx'), 10);
      }
    }
    return best;
  }

  function syncStripPosition() {
    state.currentPage = computeStripPage();
    state.navigated = true;
    updateIndicator();
    maybeAutoNextStrip();
  }

  /** Advance/retreat by one page-step, honouring the chapter boundaries. */
  function step(delta) {
    var p = layoutPrefs();
    var size = p.layout === 'double' ? 2 : 1;
    var target = state.currentPage + (delta > 0 ? size : -size);
    if (p.layout === 'double') target = spreadStart(target, p);
    navigateTo(target, delta > 0);
  }

  function navigateTo(target, forward) {
    var p = layoutPrefs();
    var total = state.totalPages;

    if (target < 1) {
      // Prev at page 1 → previous chapter at its LAST page, auto-prev only.
      // The cross-page handoff rides sessionStorage because the destination is
      // a server-rendered document; `location.assign` never mutates history.
      if (p.autoPrev && state.prevUrl) {
        writeString(sessionStorage, SS_OPEN_LAST, '1');
        location.assign(state.prevUrl);
        return;
      }
      nudge('prev');
      return;
    }

    if (target > total || (p.layout === 'double' && target > lastSpreadIndex(p))) {
      if (forward) {
        // Contract §1.1: the end-of-chapter behaviour is the nav overlay
        // (§5.4), and it is revealed the moment the last page is reached.
        // Chapter advance is "automatic only when auto-next is on" (§1.2);
        // the explicit path is the overlay's next button or `chapter.next`.
        if (state.nextUrl && p.autoNext) startCountdown();
        revealChapterNav();
      } else {
        nudge();
      }
      return;
    }

    goToPage(target);
  }

  /** Contract §1.2 boundary feedback, suppressed when motion is reduced. */
  function nudge(direction) {
    var reduced = false;
    try {
      reduced = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    } catch (_) { /* keep the boundary safe by default */ }
    if (reduced) return;
    var zoneName = direction === 'prev' ? 'prev' : 'next';
    var target = $('.reader-zone[data-zone="' + zoneName + '"]') ||
      document.getElementById('reader-hud');
    if (!target || typeof target.animate !== 'function') return;
    var sign = direction === 'prev' ? -1 : 1;
    target.animate([
      { transform: 'translate3d(0, 0, 0)' },
      { transform: 'translate3d(' + (4 * sign) + 'px, 0, 0)' },
      { transform: 'translate3d(0, 0, 0)' }
    ], { duration: 180, easing: 'ease-out' });
  }

  function prefetchAround(idx) {
    var images = [];
    for (var i = idx; i < idx + 3 && i <= state.totalPages; i++) {
      var el = pageElByIndex(i);
      var img = el ? $('img', el) : null;
      if (img && img.getAttribute('src')) images.push(img.getAttribute('src'));
    }
    for (var j = 0; j < images.length; j++) {
      try { fetch(images[j], { mode: 'no-cors' }); } catch (_) { /* warm-up only */ }
    }
  }

  // ============================================================
  //  TAP ZONES (contract §4.1)
  // ============================================================

  function initZones() {
    var zones = document.getElementById('reader-zones');
    if (!zones) return;
    var btns = $$('.reader-zone', zones);
    for (var i = 0; i < btns.length; i++) {
      btns[i].addEventListener('click', function (e) {
        e.stopPropagation();
        // A long-press that opened the composer must not also toggle the sheet.
        if (state.longPressFired) { state.longPressFired = false; return; }
        var zone = e.currentTarget.getAttribute('data-zone');
        if (zone === 'menu') { toggleSheet(); return; }
        var rtl = effectivePrefs().direction === 'rtl';
        // Physical zone → semantic action; `data-zone` is a side, the action
        // is derived from direction so RTL mirrors automatically (§1.2).
        var forward = rtl ? (zone === 'prev') : (zone === 'next');
        step(forward ? 1 : -1);
        showChrome();
      });
    }
  }

  // ============================================================
  //  KEYBINDS + REMAP (contract §4.3)
  // ============================================================

  function comboFromEvent(e) {
    var parts = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Meta');
    parts.push(e.code || e.key);
    return parts.join('+');
  }

  function rebuildKeymap() {
    var kb = effectivePrefs().keybinds;
    var map = {};
    for (var a = 0; a < ACTION_IDS.length; a++) {
      var action = ACTION_IDS[a];
      var keys = kb[action] || [];
      for (var k = 0; k < keys.length; k++) map[keys[k]] = action;
    }
    state.keymap = map;
  }

  function isEditable(el) {
    if (!el) return false;
    var tag = el.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable === true;
  }

  function isNativeControl(el) {
    if (!el) return false;
    return el.tagName === 'BUTTON' || (el.tagName === 'A' && el.hasAttribute('href'));
  }

  function directionalAction(action, event) {
    if (effectivePrefs().direction !== 'rtl') return action;
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return action;
    if (action === 'page.next') return 'page.prev';
    if (action === 'page.prev') return 'page.next';
    if (action === 'chapter.next') return 'chapter.prev';
    if (action === 'chapter.prev') return 'chapter.next';
    return action;
  }

  // ============================================================
  //  INPUT (keyboard handler, swipe, auto-next) — contract §4
  // ============================================================

  function runAction(action) {
    switch (action) {
      case 'page.next': step(1); break;
      case 'page.prev': step(-1); break;
      case 'chapter.next': if (state.nextUrl) location.assign(state.nextUrl); break;
      case 'chapter.prev': if (state.prevUrl) location.assign(state.prevUrl); break;
      case 'ui.toggle': toggleSheet(); break;
      case 'ui.focus': toggleFocus(); break;
      case 'ui.fitCycle': cycleFit(); break;
      case 'layout.cycle': cycleLayout(); break;
      case 'ui.chapterNav': openChapterNavigation(); break;
      case 'ui.restoreDefaults': restoreDefaults(); break;
      case 'note.add': openComposer(); break;
      default: break;
    }
  }

  /** Fit shortcuts now cycle the public preset model, never a hidden fit mode. */
  function cycleFit() {
    cycleLayout();
  }

  function cycleLayout() {
    var ids = ['flow', 'page', 'spread'];
    var current = activePreset(layoutPrefs());
    var index = ids.indexOf(current);
    var next = ids[(index + 1) % ids.length];
    if (next === 'spread' && window.innerWidth < 768) next = 'flow';
    applyPreset(next);
  }

  function onKeydown(e) {
    if (isEditable(e.target)) return;
    if (isNativeControl(e.target) && (e.key === 'Enter' || e.key === ' ' || e.key === 'Spacebar')) return;

    if (e.key === 'Escape') {
      if (state.composerOpen) { e.preventDefault(); closeComposer(false); return; }
      if (notesPanelOpen()) { e.preventDefault(); closeNotesPanel(); return; }
      if (state.sheetOpen) { e.preventDefault(); closeSheet(); return; }
      return;
    }

    // The keymap lookup is by `code` (contract §4.3): layout-independent.
    var combo = comboFromEvent(e);
    var action = state.keymap && state.keymap[combo];
    // A few legacy bindings are stored by `key` (e.g. "Space" parses as
    // `code: "Space"`, but "PageUp"/"PageDown" also work either way); fall back
    // to the raw key so a remap typed as a key name still fires.
    if (!action && state.keymap && state.keymap[e.key]) action = state.keymap[e.key];
    if (!action) return;
    e.preventDefault();
    runAction(directionalAction(action, e));
    showChrome();
  }

  function initSwipe() {
    var startX = 0;
    var startY = 0;
    var tracking = false;

    document.addEventListener('touchstart', function (e) {
      if (e.touches.length !== 1) { tracking = false; return; }
      if (state.sheetOpen || state.composerOpen || notesPanelOpen()) { tracking = false; return; }
      if (isEditable(e.target)) { tracking = false; return; }
      // Vertical layouts are scroll-driven; a horizontal swipe has no page to
      // step (§4.4).
      var layout = layoutPrefs().layout;
      if (layout === 'strip' || layout === 'vertical') { tracking = false; return; }
      startX = e.touches[0].clientX;
      startY = e.touches[0].clientY;
      tracking = true;
    }, { passive: true });

    document.addEventListener('touchend', function (e) {
      if (!tracking || !e.changedTouches.length) return;
      tracking = false;
      var dx = e.changedTouches[0].clientX - startX;
      var dy = e.changedTouches[0].clientY - startY;
      var threshold = effectivePrefs().swipeThreshold;
      // Distance only, never velocity (§4.4), and never on a vertical gesture.
      if (Math.abs(dx) < threshold || Math.abs(dx) < Math.abs(dy)) return;
      var rtl = effectivePrefs().direction === 'rtl';
      var forward = rtl ? dx > 0 : dx < 0;
      step(forward ? 1 : -1);
      showChrome();
    }, { passive: true });
  }

  function autoNextCancelled() {
    return readString(sessionStorage, SS_AUTONEXT_PREFIX + state.chapterId) === '1';
  }

  function startCountdown() {
    if (state.autoNextTimer || autoNextCancelled()) return;
    var el = document.getElementById('reader-countdown');
    if (!el || !state.nextUrl) return;
    el.hidden = false;
    state.countdownValue = 3;
    var secs = $('.reader-countdown-secs', el);
    if (secs) secs.textContent = '3';
    state.autoNextTimer = setInterval(function () {
      state.countdownValue -= 1;
      if (state.countdownValue <= 0) {
        clearInterval(state.autoNextTimer);
        state.autoNextTimer = null;
        location.assign(state.nextUrl);
        return;
      }
      if (secs) secs.textContent = String(state.countdownValue);
    }, 1000);
  }

  function cancelAutoNext() {
    if (state.autoNextTimer) {
      clearInterval(state.autoNextTimer);
      state.autoNextTimer = null;
    }
    var el = document.getElementById('reader-countdown');
    if (el) el.hidden = true;
    writeString(sessionStorage, SS_AUTONEXT_PREFIX + state.chapterId, '1');
  }

  function stripLastPageFullyVisible() {
    var last = pageElByIndex(state.totalPages);
    if (!last) return false;
    var rect = last.getBoundingClientRect();
    var vh = window.innerHeight || 1;
    return rect.top >= 0 && rect.bottom <= vh + 1;
  }

  function clearStripAutoNext() {
    if (!state.stripAutoNextTimer) return;
    clearTimeout(state.stripAutoNextTimer);
    state.stripAutoNextTimer = null;
  }

  function maybeAutoNextStrip() {
    var p = layoutPrefs();
    if (p.layout !== 'strip' || !p.autoNext || !state.nextUrl || autoNextCancelled() ||
        !stripLastPageFullyVisible()) {
      clearStripAutoNext();
      return;
    }
    if (state.stripAutoNextTimer) return;
    state.stripAutoNextTimer = setTimeout(function () {
      state.stripAutoNextTimer = null;
      var current = layoutPrefs();
      // Re-check after the dwell: scrolling away during the 1500ms boundary
      // delay cancels the countdown before it can start.
      if (current.layout === 'strip' && current.autoNext && !autoNextCancelled() &&
          stripLastPageFullyVisible()) {
        startCountdown();
      }
    }, 1500);
  }

  function maybeEndOfChapter() {
    var p = layoutPrefs();
    if (p.layout === 'strip') return;
    var boundary = p.layout === 'double' ? lastSpreadIndex(p) : state.totalPages;
    if (state.currentPage >= boundary) {
      revealChapterNav();
      if (p.autoNext && state.nextUrl) startCountdown();
    } else {
      hideChapterNav();
    }
  }

  function openChapterNavigation() {
    if (layoutPrefs().layout === 'strip') {
      var bottom = document.getElementById('reader-bottom-nav');
      if (bottom && bottom.scrollIntoView) bottom.scrollIntoView({ behavior: 'auto', block: 'end' });
      return;
    }
    closeSheet();
    revealChapterNav();
  }

  function revealChapterNav() {
    var nav = document.getElementById('reader-chapter-nav');
    if (nav) nav.hidden = false;
  }

  function hideChapterNav() {
    var nav = document.getElementById('reader-chapter-nav');
    if (nav) nav.hidden = true;
  }

  function initChapterNav() {
    var nav = document.getElementById('reader-chapter-nav');
    if (!nav) return;
    var nextBtn = $('.reader-nav-btn[data-action="next"]', nav);
    if (nextBtn) {
      nextBtn.addEventListener('click', function () {
        if (state.nextUrl) location.assign(state.nextUrl);
      });
    }
  }

  /** `#reader-nextup-pages` carries the first 3 page URLs per upcoming chapter. */
  function nextupPages() {
    var el = document.getElementById('reader-nextup-pages');
    if (!el) return {};
    var parsed = null;
    try { parsed = JSON.parse(el.textContent || '{}'); } catch (_) { parsed = null; }
    return parsed && typeof parsed === 'object' ? parsed : {};
  }

  function initPrefetch() {
    var pages = nextupPages();
    var items = $$('.reader-nextup-prefetch');
    for (var i = 0; i < items.length; i++) {
      (function (btn) {
        var item = btn.closest && btn.closest('.reader-nextup-item');
        var chapterId = item ? item.getAttribute('data-chapter-id') : null;
        var urls = chapterId && pages[String(chapterId)] ? pages[String(chapterId)] : [];

        // No resolvable URLs means the warm-up cannot happen. Say so instead of
        // flipping to "ready" for work that was never attempted.
        if (!urls.length) {
          btn.disabled = true;
          btn.setAttribute('data-prefetch-state', 'unavailable');
          btn.setAttribute('aria-label', 'No pages to warm up');
          btn.setAttribute('title', 'No pages available to warm up yet');
          return;
        }

        btn.addEventListener('click', function () {
          if (btn.getAttribute('data-prefetch-state') === 'prefetching') return;
          if (btn.getAttribute('data-prefetch-state') === 'ready') return;
          btn.setAttribute('data-prefetch-state', 'prefetching');
          // Warm the first 3 page URLs; a failed fetch still counts as
          // "attempted" (contract §5.5), so `ready` does not depend on success.
          var pending = [];
          for (var u = 0; u < urls.length; u++) {
            try {
              pending.push(fetch(urls[u], { mode: 'no-cors' }).catch(function () { return null; }));
            } catch (_) { /* a synchronous throw still counts as attempted */ }
          }
          var settle = pending.length
            ? Promise.all(pending)
            : Promise.resolve([]);
          settle.then(function () {
            btn.setAttribute('data-prefetch-state', 'ready');
          });
        });
      })(items[i]);
    }
  }

  // ============================================================
  //  COMFORT (contract §5)
  // ============================================================

  function showChrome() {
    var topbar = document.getElementById('reader-topbar');
    var hud = document.getElementById('reader-hud');
    if (topbar) topbar.classList.add('visible');
    if (hud) hud.classList.add('visible');
    if (state.hideTimer) clearTimeout(state.hideTimer);
    var delay = effectivePrefs().hideDelay;
    if (delay === 0) return; // 0 = keep the chrome permanently visible
    state.hideTimer = setTimeout(hideChrome, delay * 1000);
  }

  function hideChrome() {
    var topbar = document.getElementById('reader-topbar');
    var hud = document.getElementById('reader-hud');
    if (topbar) topbar.classList.remove('visible');
    if (hud) hud.classList.remove('visible');
  }

  /**
   * Apply the idle dim state. `--r-dim` is the overlay's opacity target
   * (contract §2.1) and the stylesheet reads it unconditionally, so it is
   * zeroed on restore or the overlay would stay tinted.
   */
  function setDim(on) {
    var body = document.body;
    if (!body) return;
    state.dimmed = !!on;
    if (on) {
      body.style.setProperty('--r-dim', String(effectivePrefs().dimOpacity));
      body.setAttribute('data-r-dim', '1');
    } else {
      body.style.setProperty('--r-dim', '0');
      body.removeAttribute('data-r-dim');
    }
  }

  function resetDimTimer() {
    if (state.dimTimer) clearTimeout(state.dimTimer);
    if (state.dimmed) setDim(false);
    var p = effectivePrefs();
    if (!p.dimDelay) return; // 0 = off
    if (state.sheetOpen || state.composerOpen || notesPanelOpen()) return;
    state.dimTimer = setTimeout(function () {
      setDim(true);
    }, p.dimDelay * 1000);
  }

  function toggleFocus() {
    if (!document.body) return;
    var on = document.body.getAttribute('data-r-focus') !== '1';
    if (on) document.body.setAttribute('data-r-focus', '1');
    else document.body.removeAttribute('data-r-focus');
    syncControls();
    showChrome();
  }

  /**
   * Resume badge (§5.5). The server renders it hidden; JS reveals it when the
   * resume page is past page 1. The page comes from `data-prime-idx` / the
   * server `#resume-target`, falling back to the local read state — the store
   * `app.js`'s former scroll-to-resume read, so resume survives a reload.
   *
   * `data-prime-idx` (0 = none) is an explicit jump-on-load; the badge is the
   * affordance for the implicit local resume.
   */
  function initResumeBadge() {
    var badge = document.getElementById('reader-resume-badge');
    if (!badge) return;

    var resumeTarget = document.getElementById('resume-target');
    var serverPage = resumeTarget ? parseInt(resumeTarget.getAttribute('data-page') || '0', 10) : 0;
    if (resumeTarget && serverPage > 0) resumeTarget.setAttribute('data-page', String(serverPage));

    // Explicit prime wins and jumps immediately; no badge is shown because the
    // reader is already at the requested page.
    if (state.primeIdx > 1 && state.primeIdx <= state.totalPages) {
      state.resumePage = state.primeIdx;
      goToPage(state.primeIdx);
      return;
    }

    var saved = serverPage > 1 ? serverPage : 0;
    if (!saved && window.MangaReadState && state.seriesId) {
      var savedState = window.MangaReadState.getChapterStatus(state.seriesId, state.chapterId);
      if (savedState && savedState.page > 1) saved = parseInt(savedState.page, 10) || 0;
    }
    if (saved > state.totalPages) saved = state.totalPages;
    if (saved <= 1) { hideResumeBadge(); return; }

    state.resumePage = saved;
    badge.style.display = '';
    badge.hidden = false;
    badge.setAttribute('data-page', String(saved));
    badge.textContent = 'Resume p. ' + saved + suffixForLastLayout();
    badge.addEventListener('click', function (e) {
      e.stopPropagation();
      badge.hidden = true;
      // `.reader-resume-badge` sets `display: inline-flex` (reader.css:1012),
      // which outranks the UA `[hidden]` rule, so hide it explicitly.
      badge.style.display = 'none';
      if (state.local) { state.local.resumeBadgeSeen = true; saveDeviceLocal(); }
      goToPage(saved);
    });
  }

  /** Hide the server-rendered resume badge when there is nothing to resume. */
  function hideResumeBadge() {
    var badge = document.getElementById('reader-resume-badge');
    if (badge) badge.style.display = 'none';
  }

  /**
   * A one-shot flag set before an auto-prev chapter hop. It lands the reader
   * on the destination chapter's last page, which is the whole point of
   * "prev at page 1" (contract §1.2).
   */
  function consumeOpenLast() {
    if (readString(sessionStorage, SS_OPEN_LAST) !== '1') return;
    removeKey(sessionStorage, SS_OPEN_LAST);
    goToPage(state.totalPages);
  }

  /** The anon → logged-in merge hint from contract §5.5. */
  function suffixForLastLayout() {
    var last = state.local && state.local.lastUsedLayout;
    if (last && LAYOUTS.indexOf(last) >= 0 && !state.seriesPrefs && last !== effectivePrefs().layout) {
      return ' · last read in ' + last;
    }
    return '';
  }

  function updateNavigationHint() {
    var hint = document.getElementById('reader-hint');
    if (!hint) return;
    var layout = layoutPrefs().layout;
    hint.textContent = layout === 'strip' ? 'Scroll to read' : 'Tap sides to navigate pages';
  }

  function initHint() {
    var hint = document.getElementById('reader-hint');
    if (!hint) return;
    updateNavigationHint();
    if (state.local && state.local.hintSeen) {
      hint.classList.add('hidden');
      return;
    }
    function dismiss() {
      hint.classList.add('hidden');
      if (state.local) { state.local.hintSeen = true; saveDeviceLocal(); }
      document.removeEventListener('click', dismiss);
      document.removeEventListener('keydown', dismiss);
    }
    document.addEventListener('click', dismiss);
    document.addEventListener('keydown', dismiss);
  }

  function bindComfortKeys() {
    // Any interaction resets the hide/dim timers and reveals chrome.
    document.addEventListener('click', function () { showChrome(); resetDimTimer(); });
    document.addEventListener('keydown', function () { showChrome(); resetDimTimer(); });
    window.addEventListener('mousemove', function () { showChrome(); resetDimTimer(); });
  }

  function initCountdownCancel() {
    var el = document.getElementById('reader-countdown');
    if (!el) return;
    var btn = $('button', el);
    if (btn) btn.addEventListener('click', function (e) { e.stopPropagation(); cancelAutoNext(); });
  }

  // ============================================================
  //  NOTES (contract §6)
  // ============================================================

  state.composerOpen = false;

  function notesPanelOpen() {
    var panel = document.getElementById('reader-notes-panel');
    return !!panel && !panel.hidden;
  }

  function openNotesPanel() {
    var panel = document.getElementById('reader-notes-panel');
    if (!panel) return;
    panel.hidden = false;
    renderNotesPanel();
    showChrome();
    var close = $('.reader-notes-close', panel);
    if (close) close.focus();
  }

  function closeNotesPanel() {
    var panel = document.getElementById('reader-notes-panel');
    if (panel) panel.hidden = true;
    resetDimTimer();
  }

  function notesForPage(idx) {
    var out = [];
    for (var i = 0; i < state.notes.length; i++) {
      if (state.notes[i].page === idx) out.push(state.notes[i]);
    }
    return out;
  }

  function renderNotePins() {
    for (var i = 0; i < state.pageEls.length; i++) {
      var el = state.pageEls[i];
      var idx = parseInt(el.getAttribute('data-page-idx'), 10);
      var list = notesForPage(idx);
      var pin = $('.reader-note-pin', el);
      if (list.length === 0) {
        if (pin) pin.parentNode.removeChild(pin);
        continue;
      }
      if (!pin) {
        pin = document.createElement('button');
        pin.type = 'button';
        pin.className = 'reader-note-pin';
        el.appendChild(pin);
      }
      pin.setAttribute('data-page-idx', String(idx));
      pin.setAttribute('data-count', String(list.length));
      pin.setAttribute('data-note-id', String(list[0].id));
      pin.setAttribute('aria-label', 'Note on page ' + idx);
      pin.textContent = String(list.length);
    }
  }

  function renderNotesPanel() {
    var panel = document.getElementById('reader-notes-panel');
    if (!panel) return;
    var count = $('.reader-notes-count', panel);
    if (count) count.textContent = String(state.notes.length);

    var list = $('.reader-notes-list', panel);
    var empty = $('.reader-notes-empty', panel);

    if (state.notes.length === 0) {
      if (list) list.hidden = true;
      if (empty) empty.hidden = false;
      return;
    }
    if (empty) empty.hidden = true;
    if (!list) {
      list = document.createElement('ul');
      list.className = 'reader-notes-list';
      panel.appendChild(list);
    }
    list.hidden = false;
    list.innerHTML = '';
    for (var i = 0; i < state.notes.length; i++) {
      list.appendChild(noteListItem(state.notes[i]));
    }
  }

  function noteListItem(note) {
    var li = document.createElement('li');
    li.className = 'reader-note-item';
    li.setAttribute('data-note-id', String(note.id));
    li.setAttribute('data-page-idx', String(note.page));

    var jump = document.createElement('button');
    jump.type = 'button';
    jump.className = 'reader-note-jump';
    jump.textContent = 'p. ' + note.page;
    jump.addEventListener('click', function (e) {
      e.stopPropagation();
      goToPage(note.page);
      closeNotesPanel();
    });

    var time = document.createElement('time');
    time.className = 'reader-note-time';
    if (note.created_at) time.setAttribute('datetime', String(note.created_at).replace(' ', 'T'));
    time.textContent = note.created_at || '';

    var text = document.createElement('p');
    text.className = 'reader-note-text';
    text.textContent = note.body;
    text.addEventListener('click', function () {
      // Contract §6.3: clamped to four lines, expands on click.
      var clamped = text.style.webkitLineClamp;
      text.style.webkitLineClamp = clamped === 'unset' ? '4' : 'unset';
      text.style.overflow = text.style.webkitLineClamp === 'unset' ? 'visible' : '';
    });

    li.appendChild(jump);
    li.appendChild(time);
    li.appendChild(text);
    return li;
  }

  function fetchNotes() {
    if (!isLoggedIn()) { state.notes = []; return; }
    try {
      fetch('/api/notes?chapter_id=' + state.chapterId, {
        headers: { 'Accept': '*/*' },
        credentials: 'same-origin'
      })
        .then(function (r) { return r.ok ? r.json() : null; })
        .then(function (data) {
          state.notes = data && Object.prototype.toString.call(data.notes) === '[object Array]'
            ? data.notes : [];
          renderNotePins();
          if (notesPanelOpen()) renderNotesPanel();
        })
        .catch(function () { /* pins simply do not appear */ });
    } catch (_) { /* ignore */ }
  }

  function upsertNote(note) {
    if (!note) return;
    for (var i = 0; i < state.notes.length; i++) {
      if (state.notes[i].id === note.id) return;
    }
    state.notes.push({ id: note.id, page: note.page, body: note.body, created_at: note.created_at || '' });
  }

  function draftKey() {
    return SS_DRAFT_PREFIX + state.chapterId;
  }

  function openComposer() {
    var composer = document.getElementById('reader-note-composer');
    if (!composer) return;
    state.composerOpen = true;
    var pageInput = document.getElementById('reader-note-page');
    var body = document.getElementById('reader-note-body');
    var error = $('.reader-note-error', composer);
    if (error) { error.hidden = true; error.textContent = ''; }
    if (pageInput) pageInput.value = String(state.currentPage);
    if (body) {
      if (!body.value) {
        var draft = readString(sessionStorage, draftKey());
        if (draft) body.value = draft;
      }
      body.focus();
    }
    composer.hidden = false;
    showChrome();
  }

  function closeComposer(force) {
    var composer = document.getElementById('reader-note-composer');
    if (!composer) return;
    var body = document.getElementById('reader-note-body');
    var text = body ? body.value : '';
    if (!force && text.trim() && !window.confirm('Discard note?')) return;
    composer.hidden = true;
    state.composerOpen = false;
    if (force && text.trim()) writeString(sessionStorage, draftKey(), text);
    resetDimTimer();
    var settings = document.getElementById('reader-settings-btn');
    if (settings) settings.focus();
  }

  function saveNote(e) {
    e.preventDefault();
    var composer = document.getElementById('reader-note-composer');
    var body = document.getElementById('reader-note-body');
    var pageInput = document.getElementById('reader-note-page');
    var error = composer ? $('.reader-note-error', composer) : null;
    var text = body ? body.value.trim() : '';
    var page = pageInput ? parseInt(pageInput.value, 10) : state.currentPage;

    function fail(message) {
      if (!error) return;
      error.textContent = message;
      error.hidden = false;
    }

    if (!text) { fail('Write something first.'); return; }
    if (text.length > 2000) { fail('Note must be 1-2000 characters.'); return; }
    if (!(page >= 1)) page = state.currentPage;

    if (!isLoggedIn()) {
      // No anonymous note writes (contract §6.1): keep the text and ask the
      // reader to sign in, then offer the draft back after login.
      writeString(sessionStorage, draftKey(), text);
      fail('Sign in to save notes — your draft is kept.');
      return;
    }

    try {
      fetch('/api/notes', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Accept': '*/*' },
        credentials: 'same-origin',
        body: JSON.stringify({ chapter_id: state.chapterId, page: page, body: text })
      })
        .then(function (r) {
          if (r.ok) return r.json();
          // axum's Json extractor rejects before the handler: 415 for a bad
          // Content-Type, 400 for a syntax error, 422 for a missing field or
          // wrong type. Treat 400/415/422 alike.
          if (r.status === 400 || r.status === 415 || r.status === 422) {
            fail('Could not save the note. Check the text and try again.');
          } else {
            fail('Could not save the note.');
          }
          return null;
        })
        .then(function (data) {
          if (!data || !data.note) return;
          upsertNote(data.note);
          removeKey(sessionStorage, draftKey());
          if (body) body.value = '';
          renderNotePins();
          if (notesPanelOpen()) renderNotesPanel();
          closeComposer(true);
        })
        .catch(function () { fail('Could not save the note.'); });
    } catch (_) {
      fail('Could not save the note.');
    }
  }

  function initNotes() {
    var form = document.getElementById('reader-note-form');
    if (form) form.addEventListener('submit', saveNote);

    // Pins are delegated from `#reader-images`: the server renders pins for
    // existing notes before this module runs, so a per-pin listener would miss
    // them; delegation also covers pins this module creates later.
    var images = document.getElementById('reader-images');
    if (images) {
      images.addEventListener('click', function (e) {
        var pin = e.target.closest && e.target.closest('.reader-note-pin');
        if (!pin) return;
        e.stopPropagation();
        openNotesPanel();
      });
    }

    var body = document.getElementById('reader-note-body');
    if (body) {
      body.addEventListener('input', function () {
        if (body.value.trim()) writeString(sessionStorage, draftKey(), body.value);
        else removeKey(sessionStorage, draftKey());
      });
    }

    var composer = document.getElementById('reader-note-composer');
    if (composer) {
      composer.addEventListener('click', function (e) { e.stopPropagation(); });
    }

    // Long-press (400ms) opens the composer anchored to the current page
    // (§6.1). In the paged layouts the fixed tap zones sit over the pages, so
    // the center zone is the surface a long-press actually lands on; the
    // `.reader-page` branch covers strip, where the zones are hidden.
    document.addEventListener('pointerdown', function (e) {
      if (state.sheetOpen || state.composerOpen) return;
      var target = e.target;
      var onPage = target.closest && target.closest('.reader-page');
      var onMenuZone = target.closest && target.closest('.reader-zone[data-zone="menu"]');
      if (!onPage && !onMenuZone) return;
      state.longPressFired = false;
      state.longPressTimer = setTimeout(function () {
        // Suppress the click that follows the release; this was a press.
        state.longPressFired = true;
        if (onPage) {
          state.currentPage = parseInt(onPage.getAttribute('data-page-idx'), 10) || state.currentPage;
        }
        openComposer();
      }, 400);
    });
    ['pointerup', 'pointercancel', 'pointermove', 'scroll'].forEach(function (evt) {
      document.addEventListener(evt, function () {
        if (state.longPressTimer) { clearTimeout(state.longPressTimer); state.longPressTimer = null; }
      }, { passive: true });
    });

  }

  // ============================================================
  //  PROGRESS PERSISTENCE (single write path)
  // ============================================================

  function rememberScrollPosition() {
    if (!state.local) return;
    var p = layoutPrefs();
    if (p.layout === 'vertical') {
      // `vertical` scrolls inside `#reader-images` (a y-snap container), not
      // the window, so `window.scrollY` stays ~0 (finding 2). Read the
      // container, matching `scrollPageIntoView`.
      var container = document.getElementById('reader-images');
      state.local.scrollPositions[String(state.chapterId)] = container ? (container.scrollTop || 0) : 0;
    } else if (p.layout === 'strip') {
      state.local.scrollPositions[String(state.chapterId)] = window.scrollY || 0;
    } else {
      state.local.scrollPositions[String(state.chapterId)] = state.currentPage;
    }
    saveDeviceLocal();
  }

  /**
   * Progress flows exclusively through `MangaReadState.setChapterStatus`,
   * which owns the `MangaSync.pushProgress` write-through. There is no second
   * write path (the review focus for this task).
   *
   * `page` defaults to the current page; callers that merely mark the chapter
   * as started pass the resume page so a load does not clobber a saved
   * position before the reader navigates.
   */
  function saveProgress(status, page) {
    if (!state.seriesId || !state.chapterId) return;
    if (!window.MangaReadState) return;
    if (state.finished) status = 'finished';
    var p = page === undefined ? state.currentPage : page;
    window.MangaReadState.setChapterStatus(
      state.seriesId,
      state.chapterId,
      status,
      p
    );
  }

  function scheduleProgress() {
    if (state.progressTimer) return;
    state.progressTimer = setTimeout(function () {
      state.progressTimer = null;
      saveProgress(state.finished ? 'finished' : 'started');
    }, 400);
  }

  function initProgress() {
    if (!window.MangaReadState) return;
    // A plain page load marks a chapter started without moving its saved
    // position. When hydration found the current row, that row was already
    // merged into local state and an initial write would only fight it.
    if (!state.progressHydrationComplete || !state.serverProgressSeen) {
      saveProgress('started', state.resumePage > 0 ? state.resumePage : state.currentPage);
    }
    if (state.finished) markReadBadge();

    var last = state.pageEls.length ? state.pageEls[state.pageEls.length - 1] : null;
    if (last && window.IntersectionObserver) {
      var observer = new IntersectionObserver(function (entries) {
        for (var i = 0; i < entries.length; i++) {
          if (entries[i].isIntersecting && !state.finished) {
            state.finished = true;
            // Do NOT move `currentPage`: in `double` the current page is the
            // first index of the visible spread (contract §1.1), and the last
            // page becoming visible must not shift that anchor.
            saveProgress('finished');
            markReadBadge();
            observer.disconnect();
          }
        }
      }, { threshold: 0.5 });
      observer.observe(last);
    }
  }

  function markReadBadge() {
    var target = $('.reader-topbar-chapter');
    if (!target || $('.completed-badge', target)) return;
    var badge = document.createElement('span');
    badge.className = 'completed-badge';
    badge.textContent = '\u2713 Read';
    target.appendChild(badge);
  }

  function initWriteThroughFailures() {
    // Observe the fire-and-forget progress write to raise the offline notice
    // after repeated failures (contract §7). The wrapper delegates to the
    // original `MangaSync.pushProgress`, so the transport stays auth.js's and
    // there is still exactly one write per progress change.
    //
    // `pushProgress` returns the request promise (auth.js). A resolved `null`
    // is a network failure (fetch rejected), and a non-ok `Response` is a
    // server rejection — both are write-through failures. A resolved ok
    // response clears the count. This is the only way to detect a failure in
    // an online browser that cannot reach the server, which `navigator.onLine`
    // cannot see.
    if (state.offlineHooked || !window.MangaSync) return;
    state.offlineHooked = true;
    var original = window.MangaSync.pushProgress;
    if (typeof original !== 'function') return;
    window.MangaSync.pushProgress = function (chapterId, page, status) {
      var result = original.call(window.MangaSync, chapterId, page, status);
      if (result && typeof result.then === 'function') {
        result.then(function (res) {
          if (res && res.ok) state.writeFailures = 0;
          else noteWriteFailure();
        });
      } else if (navigator.onLine === false) {
        // No promise (no session / older auth.js): fall back to the browser's
        // connectivity signal, as before.
        noteWriteFailure();
      } else {
        state.writeFailures = 0;
      }
      // Forward the original return value so the wrapper preserves auth.js's
      // signature for any other observer.
      return result;
    };
    window.addEventListener('online', function () { state.writeFailures = 0; });
  }

  function noteWriteFailure() {
    state.writeFailures += 1;
    if (state.writeFailures >= 3) {
      var banner = document.getElementById('reader-offline');
      if (banner) banner.hidden = false;
    }
  }

  // ============================================================
  //  PAGE STATES (contract §7)
  // ============================================================

  function setPageState(page, value) {
    page.setAttribute('data-page-state', value);
    var error = $('.reader-page-error', page);
    if (error) error.hidden = value !== 'error';
  }

  function retryImage(page) {
    var img = $('img', page);
    if (!img) return;
    setPageState(page, 'loading');
    // Re-assign the same src to force a refetch (contract §7).
    var src = img.getAttribute('src');
    img.setAttribute('src', '');
    img.setAttribute('src', src);
  }

  function initPageStates() {
    for (var i = 0; i < state.pageEls.length; i++) {
      (function (page) {
        var img = $('img', page);
        if (!img) return;
        setPageState(page, img.complete && img.naturalWidth > 0 ? 'ready' : 'loading');

        img.addEventListener('load', function () {
          setPageState(page, 'ready');
        });
        img.addEventListener('error', function () {
          setPageState(page, 'error');
          // One auto-retry after 2000ms (contract §7).
          if (!state.retried[page.getAttribute('data-page-idx')]) {
            state.retried[page.getAttribute('data-page-idx')] = true;
            setTimeout(function () { retryImage(page); }, 2000);
          }
        });

        var retry = $('.reader-retry-btn', page);
        if (retry) retry.addEventListener('click', function (e) { e.stopPropagation(); retryImage(page); });
      })(state.pageEls[i]);
    }
  }

  function initResumeFromScroll() {
    var chapterId = state.chapterId;
    var p = layoutPrefs();
    var saved = state.local && state.local.scrollPositions ? state.local.scrollPositions[String(chapterId)] : 0;
    if (!saved || typeof saved !== 'number') return;

    // A server hydration is newer than the local scroll offset.
    if (state.serverResumePage > 1) return;

    // An explicit server prime (`data-prime-idx > 1`) is an intentional jump.
    // `initResumeBadge` already performed it, so never override it here.
    if (state.primeIdx > 1) return;

    // The scroll-driven layouts store a byte offset, which is a more precise
    // device-local resume than the page the read-state happens to name — and
    // in `vertical` that page is only ever a rounded snapshot of the offset.
    // Restore the container's own offset first, before the page guard below
    // (which exists for the paged layouts).
    if (p.layout === 'vertical') {
      var container = document.getElementById('reader-images');
      if (container && saved > 0 && container.scrollTo) {
        container.scrollTo({ top: saved, behavior: 'auto' });
      }
      return;
    }
    if (p.layout === 'strip') {
      if (saved > 0) window.scrollTo(0, saved);
      return;
    }

    // Prime index (server-provided resume) wins for the paged layouts.
    if (state.resumePage > 1) return;
    if (saved > 1) {
      goToPage(saved);
    }
  }

  // ============================================================
  //  INIT
  // ============================================================

  function buildApi() {
    window.MangaReader = {
      get prefs() { return clonePrefs(effectivePrefs()); },
      save: savePrefs,
      openSheet: openSheet,
      closeSheet: closeSheet,
      goToPage: goToPage,
      get currentPage() { return state.currentPage; }
    };
  }

  function init() {
    state.track = document.getElementById('reader-track');
    if (!state.track) return; // not the reader page

    state.isReader = true;
    state.loggedIn = isLoggedIn();
    state.seriesId = state.track.getAttribute('data-series-id') || 0;
    state.chapterId = parseInt(state.track.getAttribute('data-chapter-id') || '0', 10) || 0;
    state.totalPages = parseInt(state.track.getAttribute('data-total-pages') || '0', 10) || 0;
    state.primeIdx = parseInt(state.track.getAttribute('data-prime-idx') || '0', 10) || 0;
    state.prevUrl = state.track.getAttribute('data-prev-chapter') || '';
    state.nextUrl = state.track.getAttribute('data-next-chapter') || '';

    loadDeviceLocal();
    resolvePrefs();
    indexPages();
    applyPrefs();
    initSheet();
    initHudTrigger();
    initZones();
    initChapterNav();
    initPrefetch();
    initNotes();
    initCountdownCancel();
    initPageStates();
    initHint();
    bindComfortKeys();

    // Navigation and input.
    window.addEventListener('scroll', function () {
      if (layoutPrefs().layout === 'strip') {
        if (state.scrollTimer) return;
        state.scrollTimer = setTimeout(function () {
          state.scrollTimer = null;
          syncStripPosition();
        }, 120);
      }
      scheduleProgress();
      if (!state.dimmed) showChrome();
      resetDimTimer();
    }, { passive: true });

    // `vertical` scrolls inside `#reader-images` (a y-snap container), not the
    // window, so its position is tracked on that element. The listener is
    // attached unconditionally and guards on the layout, because a sheet
    // change can switch to `vertical` after init.
    var snapContainer = document.getElementById('reader-images');
    if (snapContainer) {
      snapContainer.addEventListener('scroll', function () {
        if (layoutPrefs().layout !== 'vertical') return;
        state.currentPage = computeStripPage() || state.currentPage;
        state.navigated = true;
        updateIndicator();
        maybeEndOfChapter();
        scheduleProgress();
        showChrome();
        resetDimTimer();
      }, { passive: true });
    }

    document.addEventListener('keydown', onKeydown);
    initSwipe();
    window.addEventListener('resize', function () {
      applyPrefs();
      selectSheetGroup();
    });
    window.addEventListener('pagehide', flushPendingPrefs);
    window.addEventListener('beforeunload', function () {
      flushPendingPrefs();
      // Preserve a resume page the reader never navigated away from; only a
      // real page step updates the stored position.
      var page = state.navigated ? state.currentPage : (state.resumePage || state.currentPage);
      saveProgress(state.finished ? 'finished' : 'started', page);
      rememberScrollPosition();
    });

    // Resume must be resolved AFTER the read-only server hydration and BEFORE
    // `initProgress` writes page 1 as the started marker.
    hydrateServerProgress().then(function () {
      initResumeBadge();
      initResumeFromScroll();
      consumeOpenLast();
      initProgress();
      renderLayout();
      resetDimTimer();
      showChrome();
      fetchNotes();
      initWriteThroughFailures();
    });

    // An anonymous reader's layout choice becomes the merge hint if they later
    // sign in (contract §5.5); the `/api/migrate` path itself is untouched.
    if (state.local && state.local.lastUsedLayout !== effectivePrefs().layout) {
      state.local.lastUsedLayout = effectivePrefs().layout;
      saveDeviceLocal();
    }
  }

  buildApi();

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
