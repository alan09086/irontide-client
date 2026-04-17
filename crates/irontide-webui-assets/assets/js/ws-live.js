// IronTide live updates via WebSocket.
//
// Connects to /api/v1/events, listens for alert messages (STATUS category
// covers torrent state transitions, peer updates, etc.), and dispatches a
// 'refreshList' CustomEvent on document.body so the HTMX polling div
// re-fetches /webui/fragments/torrent-list.
//
// Design choices from M166 review:
//
//   C3 — Alerts only, never stats. Stats are 500ms heartbeats with no
//        state-transition signal; refreshing twice per second on every
//        heartbeat would be ~2 Hz fragment churn even when nothing
//        visible changes. Alerts fire on real events (add/pause/resume/
//        complete/error) and are bursty, so...
//
//   C3 — Trailing-debounce refreshList to 1 Hz. A burst of alerts (e.g.
//        several TorrentAdded events from an add_magnet retry) collapses
//        to one fragment fetch, capping the sustained refresh rate at
//        one per second while preserving sub-second latency on the
//        first alert.
//
//   Reconnect — Exponential backoff from 500 ms up to 30 s, with the
//        timer reset on successful open so a flaky connection doesn't
//        permanently wedge at the 30 s ceiling.

(function () {
  'use strict';

  var INITIAL_BACKOFF_MS = 500;
  var MAX_BACKOFF_MS = 30000;
  var REFRESH_DEBOUNCE_MS = 1000;

  // Polling cadences for the #torrent-list HTMX poller. While WS is live
  // we push refreshes, so the poll interval is slowed to a once-in-a-while
  // sanity check. When WS is down, polling reverts to a sub-second cadence
  // so users still see fresh data within one heartbeat.
  var FAST_TRIGGER = 'load, every 2s, refreshList from:body';
  var SLOW_TRIGGER = 'load, every 30s, refreshList from:body';

  // Detail-panel cadences mirror the list-view trade-off: fast poll (2s) as
  // a fallback when WS is down, slow poll (30s) as a sanity backstop when
  // push-updates handle the heavy lifting. The `load` trigger is omitted
  // here because panels already fire hx-get on page load from their
  // static `hx-trigger` attribute.
  var DETAIL_FAST_TRIGGER =
    "load, every 2s, refreshDetail[detail.hash=='HASH'] from:body";
  var DETAIL_SLOW_TRIGGER =
    "load, every 30s, refreshDetail[detail.hash=='HASH'] from:body";

  var backoff = INITIAL_BACKOFF_MS;
  var refreshTimer = null;
  var detailRefreshTimer = null;
  var pendingDetailHashes = [];

  function setPollCadence(triggerValue) {
    var el = document.getElementById('torrent-list');
    if (!el) return;
    var current = el.getAttribute('hx-trigger');
    if (current === triggerValue) return;
    el.setAttribute('hx-trigger', triggerValue);
    if (window.htmx && typeof window.htmx.process === 'function') {
      window.htmx.process(el);
    }
  }

  /** Swap every `[data-detail-poll]` panel's hx-trigger between the fast
   *  (WS-down) and slow (WS-up) cadences. Each panel's bracket-filter
   *  embeds its own hash, so we call htmx.process(el) explicitly after
   *  mutating the attribute to force interval timers to reset. */
  function setDetailPollCadence(fast) {
    var hash = document.body.getAttribute('data-detail-hash') || '';
    if (!hash) return;
    var template = fast ? DETAIL_FAST_TRIGGER : DETAIL_SLOW_TRIGGER;
    var trigger = template.replace('HASH', hash);
    var panels = document.querySelectorAll('[data-detail-poll]');
    for (var i = 0; i < panels.length; i += 1) {
      var p = panels[i];
      if (p.getAttribute('hx-trigger') === trigger) continue;
      p.setAttribute('hx-trigger', trigger);
      if (window.htmx && typeof window.htmx.process === 'function') {
        window.htmx.process(p);
      }
    }
  }

  function scheduleRefresh() {
    if (refreshTimer) return;
    refreshTimer = setTimeout(function () {
      refreshTimer = null;
      document.body.dispatchEvent(new CustomEvent('refreshList'));
    }, REFRESH_DEBOUNCE_MS);
  }

  /** Detail dispatch: coalesce a burst of alerts into one refreshDetail
   *  per affected hash. Suppressed when the detail root has been marked
   *  as removed (see swapRemovedBanner in Task 9). */
  function scheduleDetailRefresh(hash) {
    if (!hash) return;
    // Only the hash tagged on document.body is currently being viewed;
    // dispatching for other torrents would churn invisible state.
    var current = document.body.getAttribute('data-detail-hash');
    if (!current || current !== hash) return;
    if (pendingDetailHashes.indexOf(hash) < 0) pendingDetailHashes.push(hash);
    if (detailRefreshTimer) return;
    detailRefreshTimer = setTimeout(function () {
      detailRefreshTimer = null;
      var queued = pendingDetailHashes;
      pendingDetailHashes = [];
      for (var i = 0; i < queued.length; i += 1) {
        document.body.dispatchEvent(
          new CustomEvent('refreshDetail', { detail: { hash: queued[i] } })
        );
      }
    }, REFRESH_DEBOUNCE_MS);
  }

  /** Extract a lowercase info hash from the Alert.kind tagged enum variant.
   *  Returns null when the variant has no info_hash field (session-scoped
   *  alerts like DhtBootstrapComplete). */
  function extractInfoHash(alertKind) {
    if (!alertKind || typeof alertKind !== 'object') return null;
    var keys = Object.keys(alertKind);
    if (keys.length === 0) return null;
    var variant = alertKind[keys[0]];
    if (!variant || typeof variant !== 'object') return null;
    var hash = variant.info_hash;
    if (typeof hash !== 'string' || hash.length === 0) return null;
    return hash.toLowerCase();
  }

  function buildUrl() {
    var loc = window.location;
    var scheme = loc.protocol === 'https:' ? 'wss:' : 'ws:';
    // interval=1000 keeps stats heartbeats coarse — we ignore them, but the
    // server uses them as idle keep-alives so the connection isn't reaped
    // by intermediaries.
    return scheme + '//' + loc.host + '/api/v1/events?interval=1000';
  }

  function connect() {
    var ws;
    try {
      ws = new WebSocket(buildUrl());
    } catch (err) {
      scheduleReconnect();
      return;
    }

    ws.addEventListener('open', function () {
      // Reset backoff so the next disconnect starts at 500 ms.
      backoff = INITIAL_BACKOFF_MS;
      document.body.setAttribute('data-ws-live', 'true');
      // Slow polling — push updates are driving refreshes now.
      setPollCadence(SLOW_TRIGGER);
      setDetailPollCadence(false);
    });

    ws.addEventListener('message', function (event) {
      var msg;
      try {
        msg = JSON.parse(event.data);
      } catch (err) {
        return;
      }
      // Only state-transition alerts trigger a refresh. Stats heartbeats
      // are used purely to detect liveness (see 'close'/'error' handlers).
      if (msg && msg.type === 'alert') {
        scheduleRefresh();
        var hash = extractInfoHash(msg.alert && msg.alert.kind);
        if (hash) scheduleDetailRefresh(hash);
      }
    });

    ws.addEventListener('close', function () {
      document.body.removeAttribute('data-ws-live');
      // Back to fast polling so state changes are still visible within
      // seconds while we wait for the WS to come back up.
      setPollCadence(FAST_TRIGGER);
      setDetailPollCadence(true);
      scheduleReconnect();
    });

    ws.addEventListener('error', function () {
      // 'error' is always followed by 'close', so defer to the close
      // handler to schedule the reconnect — avoids double-scheduling.
    });
  }

  function scheduleReconnect() {
    setTimeout(connect, backoff);
    backoff = Math.min(backoff * 2, MAX_BACKOFF_MS);
  }

  // Kick off the first connection once the DOM is interactive. If the
  // script is loaded synchronously before <body>, defer until DOM ready.
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', connect);
  } else {
    connect();
  }
})();
