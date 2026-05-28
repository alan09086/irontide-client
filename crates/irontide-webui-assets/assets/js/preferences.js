/*
 * IronTide WebUI — Preferences page brain (M232).
 *
 * Three responsibilities:
 *   1. URL-hash-driven tab switching (deep-linkable, survives reload).
 *   2. Restart-banner persistence across page reloads (until user dismisses
 *      or the daemon restarts and clears the flag).
 *   3. HTMX `restartPending` event → render banner inline on the next save.
 *
 * No client-side validation lives here — D6 (server-side validation only).
 */

(function () {
  'use strict';

  var STORAGE_KEY = 'irontide.preferences.restartPending';

  // --- Tab routing ----------------------------------------------------------

  function activateTab(name) {
    var safe = String(name || 'behaviour').toLowerCase();
    var tabs = document.querySelectorAll('.preferences-tab');
    var panels = document.querySelectorAll('.preferences-tab-panel');
    var found = false;
    for (var i = 0; i < panels.length; i += 1) {
      var p = panels[i];
      var matches = p.getAttribute('data-tab') === safe;
      p.toggleAttribute('hidden', !matches);
      if (matches) found = true;
    }
    if (!found) {
      // Unknown hash — fall back to first panel.
      if (panels[0]) {
        panels[0].removeAttribute('hidden');
        safe = panels[0].getAttribute('data-tab') || 'behaviour';
      }
    }
    for (var j = 0; j < tabs.length; j += 1) {
      var t = tabs[j];
      var match = t.getAttribute('data-tab') === safe;
      t.classList.toggle('preferences-tab-active', match);
      t.setAttribute('aria-selected', match ? 'true' : 'false');
    }
  }

  function tabFromHash() {
    var h = (window.location.hash || '').replace(/^#tab-/, '');
    return h || 'behaviour';
  }

  // --- Restart banner ------------------------------------------------------

  function readPersistedRestartFields() {
    try {
      var raw = window.localStorage.getItem(STORAGE_KEY);
      if (!raw) return [];
      var parsed = JSON.parse(raw);
      return Array.isArray(parsed) ? parsed : [];
    } catch (e) {
      return [];
    }
  }

  function writePersistedRestartFields(fields) {
    try {
      if (fields && fields.length > 0) {
        window.localStorage.setItem(STORAGE_KEY, JSON.stringify(fields));
      } else {
        window.localStorage.removeItem(STORAGE_KEY);
      }
    } catch (e) {
      // localStorage unavailable — banner only lasts the current page view.
    }
  }

  function renderBanner(fields) {
    var host = document.getElementById('preferences-restart-banner');
    if (!host) return;
    if (!fields || fields.length === 0) {
      host.innerHTML = '';
      host.className = '';
      return;
    }
    host.className = 'preferences-restart-banner';
    var card = document.createElement('div');
    card.className = 'preferences-restart-banner-card';
    var heading = document.createElement('strong');
    heading.textContent = 'Restart pending';
    var p = document.createElement('p');
    p.textContent =
      'The following setting' +
      (fields.length === 1 ? '' : 's') +
      ' will take effect after the next session restart:';
    var ul = document.createElement('ul');
    ul.className = 'preferences-restart-fields';
    for (var i = 0; i < fields.length; i += 1) {
      var li = document.createElement('li');
      var code = document.createElement('code');
      code.textContent = fields[i];
      li.appendChild(code);
      ul.appendChild(li);
    }
    var btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'preferences-restart-dismiss';
    btn.textContent = 'Dismiss';
    btn.setAttribute('aria-label', 'Dismiss');
    btn.addEventListener('click', dismissRestartBanner);
    card.appendChild(heading);
    card.appendChild(p);
    card.appendChild(ul);
    card.appendChild(btn);
    host.innerHTML = '';
    host.appendChild(card);
  }

  window.dismissRestartBanner = function () {
    writePersistedRestartFields([]);
    renderBanner([]);
  };

  // --- Wire it up ----------------------------------------------------------

  function init() {
    activateTab(tabFromHash());
    window.addEventListener('hashchange', function () {
      activateTab(tabFromHash());
    });

    // Persist restart-required fields from saves.
    document.body.addEventListener('settingsSaved', function (ev) {
      var fields = [];
      if (ev && ev.detail && Array.isArray(ev.detail.restartPending)) {
        fields = ev.detail.restartPending;
      }
      // Server response is authoritative — replace the persisted list.
      writePersistedRestartFields(fields);
      // The HTMX outerHTML swap of the form re-rendered the banner host
      // inline, so we only need to re-render here when the server omitted
      // the inline render (e.g., the response was empty for some reason).
    });

    // On initial page load, replay any persisted restart-required state so
    // the banner survives a refresh until the user dismisses or the daemon
    // restarts.
    var persisted = readPersistedRestartFields();
    if (persisted.length > 0) {
      var host = document.getElementById('preferences-restart-banner');
      // Only paint from JS when the server didn't inline a banner (e.g.,
      // this is a fresh GET). If the server already painted a banner,
      // trust the server-rendered list.
      if (host && host.children.length === 0) {
        renderBanner(persisted);
      }
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
