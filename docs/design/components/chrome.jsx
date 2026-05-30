// IronTide — window chrome. KDE Plasma / Breeze-centric.
// Server-side-decoration look: app icon left, title centered, window
// controls on the RIGHT (minimize · maximize · close). Below it sits the
// application menu bar (File / Edit / View / Tools / Help), qBittorrent-style.
(() => {
  const { Icon } = window;

  // Breeze-style window control button: flat, symbol shows on hover,
  // close tints coral. 1.75px strokes to match the icon system.
  function WinBtn({ kind }) {
    const [hov, setHov] = React.useState(false);
    const danger = kind === 'close';
    const glyph = {
      min:   <path d="M3 8h8" />,
      max:   <rect x="3" y="3" width="8" height="8" rx="0.5" />,
      close: <path d="M3.5 3.5l7 7M10.5 3.5l-7 7" />,
    }[kind];
    return (
      <button
        onMouseEnter={() => setHov(true)} onMouseLeave={() => setHov(false)}
        style={{
          width: 26, height: 26, display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
          border: 'none', cursor: 'pointer', borderRadius: 'var(--r-sm)',
          background: hov ? (danger ? 'rgba(251,86,91,0.16)' : 'var(--bg-hover)') : 'transparent',
          color: hov && danger ? 'var(--st-error)' : 'var(--fg-2)',
          transition: 'background var(--dur-fast), color var(--dur-fast)',
        }}>
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none"
          stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          {glyph}
        </svg>
      </button>
    );
  }

  // The emerald bolt — the brand signature mark, with the green pulse glow.
  function BoltMark({ size = 16 }) {
    return (
      <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden="true"
        style={{ filter: 'drop-shadow(0 0 3px rgba(0,217,146,0.55))', animation: 'it-glow 3.2s var(--ease-out) infinite' }}>
        <path d="M9 1 3 9h4l-1 6 6-9H8z" fill="var(--accent)" />
      </svg>
    );
  }

  function WindowChrome({ subtitle }) {
    return (
      <div style={{
        height: 34, flexShrink: 0,
        background: 'var(--bg-1)',
        borderBottom: '1px solid var(--border-1)',
        display: 'flex', alignItems: 'center',
        padding: '0 6px 0 10px',
        userSelect: 'none',
      }}>
        {/* Left — app mark + wordmark */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, minWidth: 180 }}>
          <BoltMark size={15} />
          <span style={{ fontSize: 13, fontWeight: 600, letterSpacing: '-0.01em' }}>IronTide</span>
        </div>

        {/* Center — Breeze centers the window title */}
        <div style={{ flex: 1, textAlign: 'center', fontSize: 12, color: 'var(--fg-2)' }}>
          IronTide{subtitle ? ` — ${subtitle}` : ''}
        </div>

        {/* Right — KDE window controls */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 1, minWidth: 180, justifyContent: 'flex-end' }}>
          <WinBtn kind="min" />
          <WinBtn kind="max" />
          <WinBtn kind="close" />
        </div>
      </div>
    );
  }

  // ── Application menu map (qBittorrent-parity, IronTide feature set) ──
  // `k` = action key dispatched to the app; `sep` = divider;
  // `submenu` = nested panel; `check`/`radio` = state-driven marks.
  function MENU_MODEL(s) {
    return [
      { label: 'File', items: [
        { k: 'add',          label: 'Add Torrent File…',          sc: 'Ctrl+O' },
        { k: 'addMagnet',    label: 'Add Torrent Link / Magnet…', sc: 'Ctrl+Shift+O' },
        { k: 'create',       label: 'Create New Torrent…',        sc: 'Ctrl+N' },
        { sep: true },
        { k: 'exportTorrent', label: 'Export .torrent…' },
        { k: 'exportData',    label: 'Export Torrent + Data…' },
        { sep: true },
        { k: 'exit',         label: 'Exit IronTide',              sc: 'Ctrl+Q' },
      ]},
      { label: 'Edit', items: [
        { k: 'resume',       label: 'Resume',         sc: 'Space' },
        { k: 'pause',        label: 'Pause' },
        { k: 'forceResume',  label: 'Force Resume' },
        { sep: true },
        { k: 'recheck',      label: 'Force Recheck',     sc: 'Ctrl+Shift+F' },
        { k: 'reannounce',   label: 'Force Reannounce' },
        { sep: true },
        { k: 'setLocation',  label: 'Set Location…' },
        { k: 'rename',       label: 'Rename…',           sc: 'F2' },
        { label: 'Category', submenu: [
          { k: 'cat:linux',   label: 'Linux ISOs' },
          { k: 'cat:software', label: 'Software' },
          { k: 'cat:none',    label: 'Uncategorized' },
          { sep: true },
          { k: 'cat:new',     label: 'New category…' },
        ]},
        { label: 'Queue', submenu: [
          { k: 'q:top',    label: 'Move to Top',    sc: 'Ctrl+Shift+Up' },
          { k: 'q:up',     label: 'Move Up',        sc: 'Ctrl+Up' },
          { k: 'q:down',   label: 'Move Down',      sc: 'Ctrl+Down' },
          { k: 'q:bottom', label: 'Move to Bottom', sc: 'Ctrl+Shift+Down' },
        ]},
        { sep: true },
        { k: 'remove',       label: 'Remove',                 sc: 'Del' },
        { k: 'removeData',   label: 'Remove + Delete Files…', sc: 'Shift+Del' },
        { sep: true },
        { k: 'selectAll',    label: 'Select All',             sc: 'Ctrl+A' },
      ]},
      { label: 'View', items: [
        { k: 'command',      label: 'Command Palette…',  sc: 'Ctrl+K' },
        { sep: true },
        { label: 'Sidebar', submenu: [
          { k: 'sidebar:full',   label: 'Full',       radio: s.sidebar === 'full' },
          { k: 'sidebar:icons',  label: 'Icons only', radio: s.sidebar === 'icons' },
          { k: 'sidebar:hidden', label: 'Hidden',     radio: s.sidebar === 'hidden' },
        ]},
        { k: 'toggleDetails', label: 'Details Panel', sc: 'Ctrl+I', check: s.detailsPanel !== false },
        { sep: true },
        { label: 'Density', submenu: [
          { k: 'density:compact',  label: 'Compact',  radio: s.density === 'compact' },
          { k: 'density:balanced', label: 'Balanced', radio: s.density === 'balanced' },
          { k: 'density:spacious', label: 'Spacious', radio: s.density === 'spacious' },
        ]},
        { k: 'toggleStriping', label: 'Row Striping',  check: !!s.striping },
        { k: 'toggleGlow',     label: 'Emerald Glow',  check: !!s.accentGlow },
      ]},
      { label: 'Tools', items: [
        { k: 'nav:search',    label: 'Search',               sc: 'Ctrl+F' },
        { k: 'nav:rss',       label: 'RSS Reader' },
        { k: 'nav:scheduler', label: 'Bandwidth Scheduler…' },
        { sep: true },
        { k: 'nav:stats',     label: 'Statistics…' },
        { k: 'nav:logs',      label: 'Logs…' },
        { k: 'nav:ipfilter',  label: 'IP Filter…' },
        { sep: true },
        { k: 'nav:webui',     label: 'Web UI / Remote…' },
        { sep: true },
        { k: 'prefs',         label: 'Preferences…',         sc: 'Ctrl+,' },
      ]},
      { label: 'Help', items: [
        { k: 'firstrun',  label: 'Setup Wizard…' },
        { sep: true },
        { k: 'about',     label: 'About IronTide' },
        { k: 'docs',      label: 'Documentation',     sc: 'F1' },
        { k: 'shortcuts', label: 'Keyboard Shortcuts' },
        { k: 'updates',   label: 'Check for Updates…' },
        { sep: true },
        { k: 'bug',       label: 'Report a Bug…' },
      ]},
    ];
  }

  function Check({ on }) {
    return (
      <span style={{ width: 16, display: 'inline-flex', justifyContent: 'center', color: 'var(--accent)' }}>
        {on ? <svg width="13" height="13" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round"><path d="m3 8 3 3 7-7"/></svg> : null}
      </span>
    );
  }
  function Dot({ on }) {
    return (
      <span style={{ width: 16, display: 'inline-flex', justifyContent: 'center', color: 'var(--accent)' }}>
        {on ? <span style={{ width: 6, height: 6, borderRadius: 3, background: 'var(--accent)' }}/> : null}
      </span>
    );
  }

  function MenuItem({ item, onItem, close }) {
    const [hov, setHov] = React.useState(false);
    if (item.sep) return <div style={{ height: 1, background: 'var(--divider)', margin: '4px 6px' }}/>;
    const hasSub = !!item.submenu;
    const hasMark = item.check !== undefined || item.radio !== undefined;
    return (
      <div
        onMouseEnter={() => setHov(true)} onMouseLeave={() => setHov(false)}
        onClick={(e) => { if (!hasSub) { e.stopPropagation(); onItem && onItem(item.k); close(); } }}
        style={{
          position: 'relative', display: 'flex', alignItems: 'center', gap: 8,
          height: 27, padding: hasMark ? '0 10px 0 4px' : '0 10px',
          borderRadius: 'var(--r-sm)', cursor: 'pointer', whiteSpace: 'nowrap',
          background: hov ? 'var(--bg-hover)' : 'transparent', color: 'var(--fg-0)', fontSize: 12.5,
        }}>
        {item.check !== undefined ? <Check on={item.check}/> : null}
        {item.radio !== undefined ? <Dot on={item.radio}/> : null}
        <span style={{ flex: 1 }}>{item.label}</span>
        {item.sc ? <span className="mono" style={{ color: 'var(--fg-3)', fontSize: 11, marginLeft: 24 }}>{item.sc}</span> : null}
        {hasSub ? <span style={{ color: 'var(--fg-2)', marginLeft: 8, display: 'inline-flex' }}>{Icon.chevronR({ size: 12 })}</span> : null}
        {hasSub && hov ? (
          <div style={{
            position: 'absolute', left: '100%', top: -5, marginLeft: 2,
            minWidth: 190, background: 'var(--bg-2)', border: '1px solid var(--border-1)',
            borderRadius: 'var(--r-md)', boxShadow: 'var(--shadow-lg)', padding: 5, zIndex: 60,
          }}>
            {item.submenu.map((sub, i) => <MenuItem key={i} item={sub} onItem={onItem} close={close}/>)}
          </div>
        ) : null}
      </div>
    );
  }

  // Application menu bar — qBittorrent menu set, Breeze flat hover + dropdowns.
  function MenuBar({ state, onItem }) {
    const [open, setOpen] = React.useState(null);   // index of open top menu
    const model = MENU_MODEL(state || {});
    React.useEffect(() => {
      const onKey = (e) => { if (e.key === 'Escape') setOpen(null); };
      window.addEventListener('keydown', onKey);
      return () => window.removeEventListener('keydown', onKey);
    }, []);
    return (
      <div style={{
        height: 28, flexShrink: 0, position: 'relative', zIndex: 50,
        background: 'var(--bg-1)', borderBottom: '1px solid var(--border-1)',
        display: 'flex', alignItems: 'center', padding: '0 4px', fontSize: 12.5,
      }}>
        {open !== null ? (
          <div onClick={() => setOpen(null)} style={{ position: 'fixed', inset: 0, zIndex: 40 }}/>
        ) : null}
        {model.map((menu, i) => (
          <div key={menu.label} style={{ position: 'relative', zIndex: 50 }}>
            <button
              onClick={() => setOpen(open === i ? null : i)}
              onMouseEnter={() => { if (open !== null) setOpen(i); }}
              style={{
                padding: '4px 9px', background: open === i ? 'var(--bg-hover)' : 'transparent',
                border: 'none', color: 'var(--fg-1)', cursor: 'pointer', borderRadius: 'var(--r-sm)',
                fontSize: 12.5, fontFamily: 'var(--font-ui)',
              }}
              onMouseOver={e => { if (open !== i) e.currentTarget.style.background = 'var(--bg-hover)'; }}
              onMouseOut={e => { if (open !== i) e.currentTarget.style.background = 'transparent'; }}
            >{menu.label}</button>
            {open === i ? (
              <div style={{
                position: 'absolute', top: 'calc(100% + 3px)', left: 0,
                minWidth: 230, background: 'var(--bg-2)', border: '1px solid var(--border-1)',
                borderRadius: 'var(--r-md)', boxShadow: 'var(--shadow-lg)', padding: 5, zIndex: 60,
              }}>
                {menu.items.map((item, j) => <MenuItem key={j} item={item} onItem={onItem} close={() => setOpen(null)}/>)}
              </div>
            ) : null}
          </div>
        ))}
      </div>
    );
  }

  window.Chrome = { WindowChrome, MenuBar, BoltMark };
})();
