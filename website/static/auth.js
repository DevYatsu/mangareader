/**
 * MangaRead session-aware sync adapter.
 *
 * - Exposes `window.MangaSync` with fire-and-forget write-through helpers.
 * - One-shot silent auto-migration: on load, if logged in (body
 *   `data-user-id`) and `localStorage.mangaread_migrated` does not match the
 *   current user id, local bookmarks + read progress are POSTed once to
 *   `/api/migrate`. No import/export UI is provided anywhere.
 *
 * localStorage stays the source of truth offline; every network failure is
 * swallowed silently.
 */

(function () {
  'use strict';

  var MIGRATED_KEY = 'mangaread_migrated';
  var BOOKMARKS_KEY = 'mangaread_bookmarks';
  var READ_PREFIX = 'mangaread_read_';
  var EPOCH = '1970-01-01 00:00:00';

  /** @returns {string|null} logged-in user id from body dataset, if any */
  function currentUserId() {
    if (document.body && document.body.dataset && document.body.dataset.userId) {
      return document.body.dataset.userId;
    }
    return null;
  }

  /**
   * Normalize an ISO8601 timestamp to server `updated_at` format
   * (`%Y-%m-%d %H:%M:%S` UTC). Falls back to the epoch on invalid input.
   * @param {*} value
   * @returns {string}
   */
  function normalizeUpdatedAt(value) {
    var d, iso;
    try {
      d = new Date(value);
      iso = d.toISOString();
      return iso.slice(0, 19).replace('T', ' ');
    } catch (_) {
      return EPOCH;
    }
  }

  /**
   * Collect local bookmarks + read progress into the `/api/migrate` shape.
   * @returns {{bookmarks: number[], progress: Array<{chapter_id:number,page:number,status:string,updated_at:string}>}}
   */
  function collectLocal() {
    var out = { bookmarks: [], progress: [] };
    var i, key, state, chKey, chapterId, e;
    try {
      out.bookmarks = JSON.parse(localStorage.getItem(BOOKMARKS_KEY) || '[]');
      if (!Array.isArray(out.bookmarks)) out.bookmarks = [];
    } catch (_) {
      out.bookmarks = [];
    }
    for (i = 0; i < localStorage.length; i++) {
      key = localStorage.key(i);
      if (!key || key.indexOf(READ_PREFIX) !== 0) continue;
      try {
        state = JSON.parse(localStorage.getItem(key));
        for (chKey in state) {
          if (!Object.prototype.hasOwnProperty.call(state, chKey)) continue;
          if (chKey.indexOf('ch_') !== 0) continue;
          chapterId = parseInt(chKey.slice(3), 10);
          if (isNaN(chapterId)) continue;
          e = state[chKey] || {};
          out.progress.push({
            chapter_id: chapterId,
            page: e.page || 1,
            status: e.status || 'started',
            updated_at: normalizeUpdatedAt(e.updated)
          });
        }
      } catch (_) {
        /* ignore malformed entries */
      }
    }
    return out;
  }

  /**
   * Fire-and-forget JSON POST. Non-ok responses and network errors are
   * swallowed so localStorage stays authoritative offline. `Accept: * / *`
   * ensures unauthenticated calls get a JSON 401, never a redirect to parse.
   * @param {string} url
   * @param {Object} body
   * @returns {Promise<Response|null>}
   */
  function postJson(url, body) {
    try {
      return fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Accept': '*/*' },
        body: JSON.stringify(body)
      }).catch(function () {
        return null;
      });
    } catch (_) {
      return Promise.resolve(null);
    }
  }

  window.MangaSync = {
    /** @returns {boolean} true when a logged-in session is present */
    loggedIn: function () {
      return currentUserId() !== null;
    },
    /**
     * Returns the request promise so a caller can observe the outcome.
     * The request shape and the error-swallowing contract are unchanged:
     * callers that ignore the return value behave exactly as before.
     * @param {number} seriesId
     * @param {boolean} bookmarked
     * @returns {Promise<Response|null>|undefined} `undefined` when no write is sent
     */
    pushBookmark: function (seriesId, bookmarked) {
      if (!currentUserId()) return;
      return postJson('/api/bookmarks', { series_id: seriesId, bookmarked: !!bookmarked });
    },
    /**
     * Returns the request promise so a caller can observe the outcome.
     * On a network failure the promise resolves `null` (never rejects); a
     * delivered-but-non-ok response resolves as the `Response`.
     * @param {number|string} chapterId
     * @param {number} page
     * @param {string} status
     * @returns {Promise<Response|null>|undefined} `undefined` when no write is sent
     */
    pushProgress: function (chapterId, page, status) {
      var id;
      if (!currentUserId()) return;
      id = parseInt(chapterId, 10);
      if (!isFinite(id)) return;
      return postJson('/api/progress', {
        chapter_id: id,
        page: page || 1,
        status: status || 'started'
      });
    }
  };

  // One-shot auto-migration on load. Retry next page load on failure
  // (flag is only set on success).
  document.addEventListener('DOMContentLoaded', function () {
    var userId, payload;
    userId = currentUserId();
    if (!userId) return;
    try {
      if (localStorage.getItem(MIGRATED_KEY) === userId) return;
    } catch (_) {
      return;
    }
    payload = collectLocal();
    if (payload.bookmarks.length === 0 && payload.progress.length === 0) {
      try {
        localStorage.setItem(MIGRATED_KEY, userId);
      } catch (_) {}
      return;
    }
    postJson('/api/migrate', payload).then(function (res) {
      if (res && res.ok) {
        try {
          localStorage.setItem(MIGRATED_KEY, userId);
        } catch (_) {}
      }
    });
  });
})();
