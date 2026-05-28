/*
 * IronTide WebUI — runtime light/dark theme switch (M234).
 *
 * Stores the user preference in localStorage under
 * `irontide.webui.theme`. Three valid values:
 *
 *   - "auto"  — match `prefers-color-scheme` (default if unset)
 *   - "dark"  — force dark
 *   - "light" — force light
 *
 * The FOUC-prevention boot snippet (inline in <head> on every page)
 * applies the resolved theme to `<html data-theme>` BEFORE stylesheets
 * resolve, so first paint matches the user's preference instead of
 * flashing the wrong theme.
 */

(function () {
  'use strict';

  var STORAGE_KEY = 'irontide.webui.theme';
  var DEFAULT_MODE = 'auto';
  var VALID_MODES = ['auto', 'dark', 'light'];

  function readMode() {
    try {
      var v = window.localStorage.getItem(STORAGE_KEY);
      if (VALID_MODES.indexOf(v) !== -1) return v;
    } catch (_e) {
      // Private mode or storage disabled — fall back to default.
    }
    return DEFAULT_MODE;
  }

  function writeMode(mode) {
    try {
      window.localStorage.setItem(STORAGE_KEY, mode);
    } catch (_e) {
      // Storage disabled — apply will still work for this page-view.
    }
  }

  function systemPrefersDark() {
    if (!window.matchMedia) return true;
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }

  function resolveTheme(mode) {
    if (mode === 'dark') return 'dark';
    if (mode === 'light') return 'light';
    return systemPrefersDark() ? 'dark' : 'light';
  }

  function applyMode(mode) {
    var theme = resolveTheme(mode);
    document.documentElement.setAttribute('data-theme', theme);
  }

  function setMode(mode) {
    if (VALID_MODES.indexOf(mode) === -1) {
      mode = DEFAULT_MODE;
    }
    writeMode(mode);
    applyMode(mode);
    document.dispatchEvent(new CustomEvent('irontide:theme-changed', {
      detail: { mode: mode, theme: resolveTheme(mode) }
    }));
  }

  function toggle() {
    // Toggle flips between dark and light; "auto" snaps to the opposite
    // of the currently-resolved theme so a single click always gives an
    // observable change.
    var current = resolveTheme(readMode());
    setMode(current === 'dark' ? 'light' : 'dark');
  }

  // Respond to system theme changes while in "auto" mode.
  if (window.matchMedia) {
    var mql = window.matchMedia('(prefers-color-scheme: dark)');
    var listener = function () {
      if (readMode() === 'auto') applyMode('auto');
    };
    if (typeof mql.addEventListener === 'function') {
      mql.addEventListener('change', listener);
    } else if (typeof mql.addListener === 'function') {
      mql.addListener(listener); // Safari < 14
    }
  }

  window.irontideTheme = {
    get: readMode,
    set: setMode,
    toggle: toggle,
    resolve: function () { return resolveTheme(readMode()); }
  };

  // Re-apply on script load too, in case the inline boot snippet did
  // not run (e.g. CSP blocked inline scripts). Idempotent.
  applyMode(readMode());
})();
