// Chapter comments: lazy list, composer, replies, edit/delete, report, and
// moderator hide/restore. Talks to /api/comments (see routes/comments.rs).
//
// Every user-supplied string goes through textContent; nothing here builds
// HTML from strings, so comment bodies cannot inject markup.
(function () {
  'use strict';

  var root = document.getElementById('chapter-comments');
  if (!root) return;

  var seriesId = Number(root.getAttribute('data-series-id'));
  var chapterNumber = Number(root.getAttribute('data-chapter-number'));
  var loggedIn = root.getAttribute('data-logged-in') === '1';
  var list = root.querySelector('.reader-comments-list');
  var status = root.querySelector('.reader-comments-status');
  var moreBtn = root.querySelector('.reader-comments-more');
  var form = root.querySelector('[data-comment-form]');
  var viewer = { logged_in: loggedIn, can_moderate: false, muted_until: null };
  var nextBefore = null;
  var loaded = false;
  var loading = false;

  var REPORT_REASONS = ['Spam', 'Spoiler without warning', 'Harassment', 'Other'];

  function el(tag, cls, text) {
    var node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text != null) node.textContent = text;
    return node;
  }

  function say(text, isError) {
    status.textContent = text || '';
    status.classList.toggle('has-text-danger', !!isError);
  }

  function setCount(n) {
    var nodes = document.querySelectorAll('[data-comments-count]');
    for (var i = 0; i < nodes.length; i++) nodes[i].textContent = String(n);
  }

  // SQLite timestamps are UTC "YYYY-MM-DD HH:MM:SS".
  function ago(ts) {
    var t = Date.parse(String(ts).replace(' ', 'T') + 'Z');
    if (isNaN(t)) return ts;
    var s = Math.max(0, Math.floor((Date.now() - t) / 1000));
    if (s < 60) return 'just now';
    if (s < 3600) return Math.floor(s / 60) + 'm ago';
    if (s < 86400) return Math.floor(s / 3600) + 'h ago';
    if (s < 2592000) return Math.floor(s / 86400) + 'd ago';
    return new Date(t).toLocaleDateString();
  }

  function api(path, body) {
    var opts = { credentials: 'same-origin', headers: { Accept: 'application/json' } };
    if (body !== undefined) {
      opts.method = 'POST';
      opts.headers['Content-Type'] = 'application/json';
      opts.body = JSON.stringify(body);
    }
    return fetch(path, opts).then(function (r) {
      return r.json().catch(function () { return {}; }).then(function (data) {
        if (!r.ok || data.ok === false) {
          var msg = data.message || (r.status === 401 ? 'Please log in again.' : 'Something went wrong.');
          throw new Error(msg);
        }
        return data;
      });
    });
  }

  // ── Rendering ──

  function renderBody(c) {
    var body = el('div', 'reader-comment-body');
    var text = el('p', null, c.body);
    if (c.spoiler) {
      body.classList.add('is-spoiler');
      var reveal = el('button', 'button is-small is-light reader-comment-reveal', 'Show spoiler');
      reveal.type = 'button';
      reveal.addEventListener('click', function () {
        body.classList.remove('is-spoiler');
        reveal.remove();
      });
      body.appendChild(reveal);
    }
    body.appendChild(text);
    return body;
  }

  function actionButton(label, onClick, extraCls) {
    var b = el('button', 'button is-small is-ghost reader-comment-action' + (extraCls ? ' ' + extraCls : ''), label);
    b.type = 'button';
    b.addEventListener('click', onClick);
    return b;
  }

  function renderComment(c, threadEl) {
    var item = el('li', 'reader-comment');
    item.setAttribute('data-comment-id', String(c.id));

    if (c.placeholder) {
      item.appendChild(el('p', 'reader-comment-placeholder m-text-muted',
        c.placeholder === 'deleted' ? '[deleted]' : '[removed by a moderator]'));
      return item;
    }

    var meta = el('p', 'reader-comment-meta is-size-7');
    meta.appendChild(el('strong', null, c.author));
    meta.appendChild(document.createTextNode(' · ' + ago(c.created_at)));
    if (c.edited) meta.appendChild(el('span', 'm-text-muted', ' · edited'));
    if (c.hidden) meta.appendChild(el('span', 'tag is-warning is-light ml-2', 'Hidden'));
    item.appendChild(meta);
    item.appendChild(renderBody(c));

    var actions = el('div', 'reader-comment-actions');
    if (viewer.logged_in && !viewer.muted_until) {
      actions.appendChild(actionButton('Reply', function () {
        openReply(threadEl, c.parent_id || c.id, c.author);
      }));
    }
    if (c.mine) {
      actions.appendChild(actionButton('Edit', function () { openEdit(item, c); }));
      actions.appendChild(actionButton('Delete', function () {
        if (!window.confirm('Delete this comment?')) return;
        api('/api/comments/' + c.id + '/delete', {}).then(reload).catch(function (e) { say(e.message, true); });
      }));
    } else if (viewer.logged_in) {
      actions.appendChild(actionButton('Report', function () { openReport(item, c); }));
    }
    if (viewer.can_moderate) {
      var verb = c.hidden ? 'restore' : 'hide';
      actions.appendChild(actionButton(c.hidden ? 'Restore' : 'Hide', function () {
        api('/api/comments/' + c.id + '/' + verb, {}).then(reload).catch(function (e) { say(e.message, true); });
      }, 'has-text-warning'));
    }
    item.appendChild(actions);
    return item;
  }

  function renderThread(t) {
    var thread = el('li', 'reader-comment-thread');
    var head = el('ol', 'reader-comment-root');
    head.appendChild(renderComment(t, thread));
    thread.appendChild(head);
    var replies = el('ol', 'reader-comment-replies');
    (t.replies || []).forEach(function (r) { replies.appendChild(renderComment(r, thread)); });
    thread.appendChild(replies);
    return thread;
  }

  // ── Inline forms ──

  function miniForm(placeholder, initial, submitLabel, onSubmit) {
    var f = el('form', 'reader-comment-inline');
    var ta = el('textarea', 'textarea is-small');
    ta.rows = 2;
    ta.maxLength = 2000;
    ta.placeholder = placeholder;
    ta.value = initial || '';
    var row = el('div', 'buttons are-small mt-1');
    var ok = el('button', 'button is-small is-primary', submitLabel);
    ok.type = 'submit';
    var cancel = el('button', 'button is-small is-light', 'Cancel');
    cancel.type = 'button';
    cancel.addEventListener('click', function () { f.remove(); });
    row.appendChild(ok);
    row.appendChild(cancel);
    f.appendChild(ta);
    f.appendChild(row);
    f.addEventListener('submit', function (e) {
      e.preventDefault();
      ok.disabled = true;
      onSubmit(ta.value).catch(function (err) {
        ok.disabled = false;
        say(err.message, true);
      });
    });
    setTimeout(function () { ta.focus(); }, 0);
    return f;
  }

  function closeInlineForms() {
    var open = root.querySelectorAll('.reader-comment-inline');
    for (var i = 0; i < open.length; i++) open[i].remove();
  }

  function openReply(threadEl, parentId, author) {
    closeInlineForms();
    var f = miniForm('Reply to ' + author, '', 'Reply', function (body) {
      return api('/api/comments', {
        series_id: seriesId, chapter_number: chapterNumber, body: body, parent_id: parentId
      }).then(reload);
    });
    threadEl.appendChild(f);
  }

  function openEdit(item, c) {
    closeInlineForms();
    var f = miniForm('Edit your comment', c.body, 'Save', function (body) {
      return api('/api/comments/' + c.id + '/edit', { body: body }).then(reload);
    });
    item.appendChild(f);
  }

  function openReport(item, c) {
    closeInlineForms();
    var f = el('form', 'reader-comment-inline');
    var sel = el('div', 'select is-small');
    var select = el('select');
    select.setAttribute('aria-label', 'Reason for report');
    REPORT_REASONS.forEach(function (r) { select.appendChild(el('option', null, r)); });
    sel.appendChild(select);
    var send = el('button', 'button is-small is-danger is-light ml-2', 'Send report');
    send.type = 'submit';
    var cancel = el('button', 'button is-small is-light ml-1', 'Cancel');
    cancel.type = 'button';
    cancel.addEventListener('click', function () { f.remove(); });
    f.appendChild(sel);
    f.appendChild(send);
    f.appendChild(cancel);
    f.addEventListener('submit', function (e) {
      e.preventDefault();
      api('/api/comments/' + c.id + '/report', { reason: select.value })
        .then(function () { f.remove(); say('Thanks — a moderator will take a look.'); })
        .catch(function (err) { say(err.message, true); });
    });
    item.appendChild(f);
  }

  // ── Loading ──

  function load(append) {
    if (loading) return;
    loading = true;
    say(append ? 'Loading…' : (loaded ? '' : 'Loading comments…'));
    var url = '/api/comments?series=' + seriesId + '&chapter=' + chapterNumber +
      (append && nextBefore ? '&before=' + nextBefore : '');
    api(url).then(function (data) {
      viewer = data.viewer || viewer;
      if (!append) list.textContent = '';
      data.comments.forEach(function (t) { list.appendChild(renderThread(t)); });
      nextBefore = data.next_before;
      moreBtn.hidden = !nextBefore;
      setCount(data.count);
      applyMute();
      say(!append && data.comments.length === 0 ? 'No comments yet. Start the discussion!' : '');
      loaded = true;
    }).catch(function (e) {
      say(e.message + ' ', true);
      var retry = el('button', 'button is-small is-light', 'Retry');
      retry.type = 'button';
      retry.addEventListener('click', function () { load(append); });
      status.appendChild(retry);
    }).then(function () { loading = false; });
  }

  function reload() {
    nextBefore = null;
    loaded = false;
    load(false);
  }

  function applyMute() {
    if (!form || !viewer.muted_until) return;
    form.hidden = true;
    var note = root.querySelector('.reader-comments-muted');
    if (!note) {
      note = el('p', 'reader-comments-muted notification is-warning is-light is-size-7');
      form.parentNode.insertBefore(note, form);
    }
    note.textContent = 'A moderator paused your commenting until ' + viewer.muted_until + ' UTC.';
  }

  moreBtn.addEventListener('click', function () { load(true); });

  if (form) {
    form.addEventListener('submit', function (e) {
      e.preventDefault();
      var ta = form.querySelector('textarea');
      var spoiler = form.querySelector('input[name="spoiler"]');
      var btn = form.querySelector('button[type="submit"]');
      btn.disabled = true;
      api('/api/comments', {
        series_id: seriesId,
        chapter_number: chapterNumber,
        body: ta.value,
        spoiler: !!(spoiler && spoiler.checked)
      }).then(function () {
        ta.value = '';
        if (spoiler) spoiler.checked = false;
        reload();
      }).catch(function (err) {
        say(err.message, true);
      }).then(function () { btn.disabled = false; });
    });
  }

  // ── Open / close (bottom sheet in paged layouts) ──

  function isStrip() {
    return document.body.getAttribute('data-r-layout') === 'strip';
  }

  function open() {
    root.classList.add('is-open');
    if (!loaded) load(false);
    if (isStrip()) {
      root.scrollIntoView({ behavior: 'smooth', block: 'start' });
    } else {
      var first = root.querySelector('textarea, a, button');
      if (first) first.focus();
    }
  }

  function close() {
    root.classList.remove('is-open');
  }

  var openers = document.querySelectorAll('[data-comments-open]');
  for (var i = 0; i < openers.length; i++) openers[i].addEventListener('click', open);
  root.querySelector('.reader-comments-close').addEventListener('click', close);
  root.addEventListener('keydown', function (e) {
    if (e.key === 'Escape' && !isStrip()) { e.stopPropagation(); close(); }
  });

  // Deep link from the moderation queue: /read/m/c#chapter-comments
  if (location.hash === '#chapter-comments') open();

  // Strip layout: load when the section nears the viewport, never before,
  // so comments never compete with page images.
  if ('IntersectionObserver' in window) {
    var io = new IntersectionObserver(function (entries) {
      if (entries[0].isIntersecting && isStrip() && !loaded) {
        load(false);
        io.disconnect();
      }
    }, { rootMargin: '400px 0px' });
    io.observe(root);
  }
})();
