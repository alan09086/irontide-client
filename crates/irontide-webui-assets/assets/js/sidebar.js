// M231 — WebUI sidebar filter state + persistence.
//
// Responsibilities:
//   1. Track active filters (state / category / tag / tracker) in memory.
//   2. Propagate filters to the torrent-list HTMX poll via `hx-vals`
//      (HTMX 2.x supports flat-object encoding only — no nested objects).
//   3. Persist section collapse state to localStorage so reload re-applies it.
//   4. Restore aria-pressed state on every OOB sidebar refresh so chips
//      survive the 2 s poll (server is stateless; client re-applies state).
//
// Filter axes are flat string arrays. Each axis is sent to the server as
// a comma-separated list (e.g. `state=downloading,seeding`). The empty
// list means "no filter on this axis".

(function () {
  'use strict';

  // ----- Filter state (in-memory) -----
  var filters = {
    state: [],
    category: [],
    tag: [],
    tracker: []
  };

  // ----- LocalStorage keys -----
  var LS_COLLAPSE = 'irontide.sidebar.collapsed';

  // ----- Collapse state (persisted) -----
  function loadCollapsed() {
    try {
      var raw = localStorage.getItem(LS_COLLAPSE);
      if (!raw) return {};
      var parsed = JSON.parse(raw);
      return (parsed && typeof parsed === 'object') ? parsed : {};
    } catch (e) {
      return {};
    }
  }

  function saveCollapsed(map) {
    try {
      localStorage.setItem(LS_COLLAPSE, JSON.stringify(map));
    } catch (e) {
      // localStorage may be disabled (incognito, quota); ignore.
    }
  }

  // Apply persisted collapse state to all currently-rendered sections.
  function applyCollapsedState() {
    var map = loadCollapsed();
    var sections = document.querySelectorAll('.sidebar-section[data-axis]');
    sections.forEach(function (section) {
      var axis = section.getAttribute('data-axis');
      var collapsed = !!map[axis];
      section.setAttribute('data-collapsed', collapsed ? 'true' : 'false');
      var toggle = section.querySelector('.sidebar-section-toggle');
      if (toggle) {
        toggle.setAttribute('aria-expanded', collapsed ? 'false' : 'true');
      }
    });
  }

  // Toggle a section's collapse state; persist + reapply DOM.
  window.toggleSidebarSection = function (axis) {
    if (!axis) return;
    var map = loadCollapsed();
    map[axis] = !map[axis];
    saveCollapsed(map);
    applyCollapsedState();
  };

  // ----- Filter state propagation -----

  // Build the hx-vals JSON for #torrent-list based on current filters.
  // Empty axes are omitted to keep the URL slim.
  function buildHxVals() {
    var vals = {};
    Object.keys(filters).forEach(function (axis) {
      if (filters[axis].length > 0) {
        vals[axis] = filters[axis].join(',');
      }
    });
    return vals;
  }

  // Push current filters to #torrent-list's hx-vals attribute. HTMX reads
  // this attribute on every poll, so the next request picks up the new
  // filter set automatically (no manual trigger needed).
  function pushFilterStateToHtmx() {
    var list = document.getElementById('torrent-list');
    if (!list) return;
    var vals = buildHxVals();
    list.setAttribute('hx-vals', JSON.stringify(vals));
    // Re-process the element so HTMX picks up the updated attribute.
    if (window.htmx && typeof window.htmx.process === 'function') {
      window.htmx.process(list);
    }
  }

  // Restore aria-pressed on every chip based on current filter state.
  // Called after each OOB sidebar refresh (server doesn't know which
  // chips are active — that lives entirely in this client).
  function applyChipState() {
    var chips = document.querySelectorAll('.sidebar-chip[data-axis]');
    chips.forEach(function (chip) {
      var axis = chip.getAttribute('data-axis');
      var value = chip.getAttribute('data-value');
      if (!axis || value === null) return;
      var active = (filters[axis] || []).indexOf(value) !== -1;
      chip.setAttribute('aria-pressed', active ? 'true' : 'false');
    });
  }

  // Toggle a single filter chip; refresh chip state + push to HTMX +
  // trigger an immediate refresh so the user sees the result without
  // waiting up to 2 s for the next poll.
  window.toggleFilter = function (axis, value) {
    if (!axis || value === null || value === undefined) return;
    if (!filters[axis]) return;
    var idx = filters[axis].indexOf(value);
    if (idx === -1) {
      filters[axis].push(value);
    } else {
      filters[axis].splice(idx, 1);
    }
    applyChipState();
    pushFilterStateToHtmx();
    triggerListRefresh();
  };

  // Clear every active filter. Called by the "Clear" button.
  window.clearAllFilters = function () {
    Object.keys(filters).forEach(function (axis) {
      filters[axis] = [];
    });
    applyChipState();
    pushFilterStateToHtmx();
    triggerListRefresh();
  };

  // Force an immediate poll. HTMX exposes this on body via the standard
  // `htmx.trigger` API — we trigger the same `refreshList` event the
  // post-action handlers fire so the torrent list refreshes on the next
  // microtask rather than waiting for the next 2 s tick.
  function triggerListRefresh() {
    if (window.htmx && typeof window.htmx.trigger === 'function') {
      window.htmx.trigger(document.body, 'refreshList');
    }
  }

  // ----- HTMX lifecycle hooks -----

  // After every HTMX swap (including OOB sidebar refresh), re-apply
  // collapse state + chip aria-pressed state. The server returns a
  // brand-new #sidebar-sections every poll, so client-side state must
  // be re-applied each time.
  document.body.addEventListener('htmx:afterSwap', function (evt) {
    applyCollapsedState();
    applyChipState();
  });

  // On initial DOM ready, push current filters (empty) to hx-vals so
  // the first poll carries the right attribute shape.
  document.addEventListener('DOMContentLoaded', function () {
    pushFilterStateToHtmx();
    applyCollapsedState();
    applyChipState();
  });
})();
