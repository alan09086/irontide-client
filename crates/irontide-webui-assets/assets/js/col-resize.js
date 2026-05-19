// Column resize for the torrent list table.
//
// Drag the right edge of any <th> to resize. Widths persist to localStorage.
// Restored on page load and after each HTMX swap of the torrent list.

(function () {
  'use strict';

  var STORAGE_KEY = 'irontide-col-widths';
  var HANDLE_WIDTH = 6;
  var MIN_COL_WIDTH = 40;

  function saveWidths(widths) {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(widths));
    } catch (_) { /* quota exceeded — ignore */ }
  }

  function loadWidths() {
    try {
      var raw = localStorage.getItem(STORAGE_KEY);
      if (raw) return JSON.parse(raw);
    } catch (_) { /* parse error — ignore */ }
    return null;
  }

  function applyWidths(table, widths) {
    if (!table || !widths) return;
    var ths = table.querySelectorAll('thead th');
    for (var i = 0; i < ths.length && i < widths.length; i += 1) {
      if (widths[i] > 0) {
        ths[i].style.width = widths[i] + 'px';
      }
    }
  }

  function getWidths(table) {
    if (!table) return [];
    var ths = table.querySelectorAll('thead th');
    var widths = [];
    for (var i = 0; i < ths.length; i += 1) {
      widths.push(ths[i].offsetWidth);
    }
    return widths;
  }

  function initResize(table) {
    if (!table) return;
    var ths = table.querySelectorAll('thead th');

    for (var i = 0; i < ths.length; i += 1) {
      ths[i].style.position = 'relative';

      var handle = document.createElement('div');
      handle.className = 'col-resize-handle';
      handle.style.position = 'absolute';
      handle.style.right = '0';
      handle.style.top = '0';
      handle.style.bottom = '0';
      handle.style.width = HANDLE_WIDTH + 'px';
      handle.style.cursor = 'col-resize';
      handle.style.userSelect = 'none';
      handle.setAttribute('data-col-idx', String(i));
      ths[i].appendChild(handle);
    }

    table.addEventListener('mousedown', function (e) {
      var target = e.target;
      if (!target.classList || !target.classList.contains('col-resize-handle')) return;

      var colIdx = parseInt(target.getAttribute('data-col-idx'), 10);
      var th = ths[colIdx];
      if (!th) return;

      var startX = e.pageX;
      var startW = th.offsetWidth;

      e.preventDefault();

      function onMove(ev) {
        var newW = Math.max(MIN_COL_WIDTH, startW + (ev.pageX - startX));
        th.style.width = newW + 'px';
      }

      function onUp() {
        document.removeEventListener('mousemove', onMove);
        document.removeEventListener('mouseup', onUp);
        saveWidths(getWidths(table));
      }

      document.addEventListener('mousemove', onMove);
      document.addEventListener('mouseup', onUp);
    });

    var saved = loadWidths();
    if (saved) applyWidths(table, saved);
  }

  function findTable() {
    return document.querySelector('#torrent-list table');
  }

  function setup() {
    var table = findTable();
    if (table) initResize(table);
  }

  // Run on initial load.
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', setup);
  } else {
    setup();
  }

  // Re-apply after HTMX swaps the torrent list (the table is replaced).
  document.body.addEventListener('htmx:afterSwap', function (e) {
    if (e.detail && e.detail.target && e.detail.target.id === 'torrent-list') {
      var table = findTable();
      if (table) {
        var saved = loadWidths();
        if (saved) applyWidths(table, saved);
        initResize(table);
      }
    }
  });
})();
