// IronTide — custom 16px line glyphs. 1.5px stroke, round caps.
// Each is a small React component returning an <svg>.

(() => {
  const base = (children, props = {}) => (
    <svg width={props.size || 16} height={props.size || 16} viewBox="0 0 16 16" fill="none"
      stroke="currentColor" strokeWidth={props.stroke || 1.5}
      strokeLinecap="round" strokeLinejoin="round" {...props}>
      {children}
    </svg>
  );

  const Icon = {
    // nav
    list:      (p) => base(<><path d="M3 4h10M3 8h10M3 12h10"/><circle cx="1.5" cy="4" r=".5"/><circle cx="1.5" cy="8" r=".5"/><circle cx="1.5" cy="12" r=".5"/></>, p),
    search:    (p) => base(<><circle cx="7" cy="7" r="4.5"/><path d="m10.5 10.5 3 3"/></>, p),
    rss:       (p) => base(<><circle cx="4" cy="12" r="1"/><path d="M3 7a6 6 0 0 1 6 6M3 3a10 10 0 0 1 10 10"/></>, p),
    scheduler: (p) => base(<><circle cx="8" cy="8" r="5.5"/><path d="M8 5v3l2 1.5"/></>, p),
    stats:     (p) => base(<><path d="M2 13V8M6 13V4M10 13v-6M14 13V2"/></>, p),
    logs:      (p) => base(<><path d="M3 2h7l3 3v9H3z"/><path d="M10 2v3h3M5 8h6M5 11h4"/></>, p),
    ipfilter:  (p) => base(<><path d="M8 1.5 2 4v4c0 3.5 2.5 5.8 6 6.5 3.5-.7 6-3 6-6.5V4z"/><path d="m6 8 1.5 1.5L10.5 6.5"/></>, p),
    torrentCreate: (p) => base(<><path d="M8 2v8M5 7l3 3 3-3M2.5 13.5h11"/></>, p),
    webui:     (p) => base(<><circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c2 2.5 2 9.5 0 12M8 2c-2 2.5-2 9.5 0 12"/></>, p),
    // actions
    add:       (p) => base(<><path d="M8 3v10M3 8h10"/></>, p),
    magnet:    (p) => base(<><path d="M3 3v5a5 5 0 0 0 10 0V3M3 3h3v5M13 3h-3v5"/></>, p),
    play:      (p) => base(<><path d="M4 3v10l8-5z"/></>, p),
    pause:     (p) => base(<><path d="M5 3v10M11 3v10"/></>, p),
    stop:      (p) => base(<><rect x="3.5" y="3.5" width="9" height="9" rx="1"/></>, p),
    remove:    (p) => base(<><path d="M3 5h10M6 5V3h4v2M5 5l1 9h4l1-9"/></>, p),
    up:        (p) => base(<><path d="M4 10l4-4 4 4"/></>, p),
    down:      (p) => base(<><path d="M4 6l4 4 4-4"/></>, p),
    folder:    (p) => base(<><path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.2L7 4.5h5.5A1.5 1.5 0 0 1 14 6v6a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12z"/></>, p),
    file:      (p) => base(<><path d="M4 2h5l3 3v9H4z"/><path d="M9 2v3h3"/></>, p),
    gear:      (p) => base(<><circle cx="8" cy="8" r="2"/><path d="M8 1v2M8 13v2M13 8h2M1 8h2M12.2 3.8l1.4-1.4M2.4 13.6l1.4-1.4M12.2 12.2l1.4 1.4M2.4 2.4l1.4 1.4"/></>, p),
    filter:    (p) => base(<><path d="M2 3h12l-4.5 5.5V13L6.5 11V8.5z"/></>, p),
    chevronR:  (p) => base(<><path d="M6 3l4 5-4 5"/></>, p),
    chevronD:  (p) => base(<><path d="M3 6l5 4 5-4"/></>, p),
    chevronU:  (p) => base(<><path d="M3 10l5-4 5 4"/></>, p),
    check:     (p) => base(<><path d="m3 8 3 3 7-7"/></>, p),
    x:         (p) => base(<><path d="m3 3 10 10M13 3 3 13"/></>, p),
    info:      (p) => base(<><circle cx="8" cy="8" r="6"/><path d="M8 7.5v3.5M8 5v.5"/></>, p),
    warn:      (p) => base(<><path d="M8 1.5 14.5 13H1.5z"/><path d="M8 6v3.5M8 11v.5"/></>, p),
    tag:       (p) => base(<><path d="M2.5 2.5h5l6 6-5 5-6-6z"/><circle cx="5.5" cy="5.5" r=".8"/></>, p),
    globe:     (p) => base(<><circle cx="8" cy="8" r="6"/><path d="M2 8h12M8 2c2 2.5 2 9.5 0 12"/></>, p),
    download:  (p) => base(<><path d="M8 2v8M4 8l4 4 4-4M3 14h10"/></>, p),
    upload:    (p) => base(<><path d="M8 13V5M4 7l4-4 4 4M3 2h10"/></>, p),
    bolt:      (p) => base(<><path d="m9 1-6 9h4l-1 5 6-9h-4z"/></>, p),
    peer:      (p) => base(<><circle cx="8" cy="6" r="2.5"/><path d="M3 14c0-2.5 2.2-4.5 5-4.5s5 2 5 4.5"/></>, p),
    tracker:   (p) => base(<><circle cx="8" cy="8" r="1.5"/><circle cx="8" cy="8" r="4"/><circle cx="8" cy="8" r="6.5"/></>, p),
    sort:      (p) => base(<><path d="M4 3v9M4 12l-1.5-1.5M4 12l1.5-1.5M11 13V4M11 4l-1.5 1.5M11 4l1.5 1.5"/></>, p),
    more:      (p) => base(<><circle cx="3" cy="8" r=".9"/><circle cx="8" cy="8" r=".9"/><circle cx="13" cy="8" r=".9"/></>, p),
    cmd:       (p) => base(<><path d="M4 4h8v8H4z"/><path d="M2 4a2 2 0 1 1 2 2M14 4a2 2 0 1 0-2 2M2 12a2 2 0 1 0 2-2M14 12a2 2 0 1 1-2-2"/></>, p),
    flag:      (p) => base(<><path d="M3 14V2M3 3h8l-2 3 2 3H3"/></>, p),
    link:      (p) => base(<><path d="M6 10 10 6M5.5 9 3 11.5a2 2 0 0 1-2.8-2.8L3 6M10.5 7 13 4.5a2 2 0 0 0-2.8-2.8L7.5 4"/></>, p),
    refresh:   (p) => base(<><path d="M2.5 8a5.5 5.5 0 0 1 10-3M13.5 8a5.5 5.5 0 0 1-10 3M10 4.5h2.5V2M6 11.5H3.5V14"/></>, p),
    pin:       (p) => base(<><path d="M10 2 14 6 10 8 8 14 6 10 2 8z"/></>, p),
    shield:    (p) => base(<><path d="M8 1.5 2 4v4c0 3.5 2.5 5.8 6 6.5 3.5-.7 6-3 6-6.5V4z"/></>, p),
    sun:       (p) => base(<><circle cx="8" cy="8" r="3"/><path d="M8 1v1.5M8 13.5V15M1 8h1.5M13.5 8H15M2.8 2.8l1.1 1.1M12.1 12.1l1.1 1.1M2.8 13.2l1.1-1.1M12.1 3.9l1.1-1.1"/></>, p),
    moon:      (p) => base(<><path d="M13.5 9.5A6 6 0 1 1 6.5 2.5a5 5 0 0 0 7 7z"/></>, p),
    layout1:   (p) => base(<><rect x="2" y="2" width="12" height="12" rx="1"/><path d="M2 6h12M7 6v8"/></>, p),
    layout2:   (p) => base(<><rect x="2" y="2" width="12" height="12" rx="1"/><path d="M6 2v12M11 2v12"/></>, p),
    layout3:   (p) => base(<><rect x="2" y="2" width="12" height="12" rx="1"/><path d="M2 5h12"/></>, p),
  };

  window.Icon = Icon;

  // Small atoms used everywhere
  function Btn({ variant='ghost', size='md', icon, children, active, onClick, title, style }) {
    const styles = {
      display: 'inline-flex', alignItems: 'center', gap: 6,
      whiteSpace: 'nowrap', flexShrink: 0,
      height: size === 'sm' ? 24 : 28,
      padding: size === 'sm' ? '0 8px' : '0 10px',
      fontSize: size === 'sm' ? 12 : 13,
      fontWeight: 500,
      borderRadius: 'var(--r-md)',
      cursor: 'pointer',
      border: '1px solid transparent',
      userSelect: 'none',
      transition: 'background var(--dur-fast) var(--ease-out), border var(--dur-fast)',
      ...style,
    };
    if (variant === 'primary') Object.assign(styles, {
      background: 'var(--accent)', color: 'var(--accent-fg)', borderColor: 'var(--accent)',
    });
    else if (variant === 'solid') Object.assign(styles, {
      background: 'var(--bg-3)', color: 'var(--fg-0)', borderColor: 'var(--border-1)',
    });
    else Object.assign(styles, {
      background: active ? 'var(--bg-hover)' : 'transparent', color: 'var(--fg-0)',
      borderColor: active ? 'var(--border-1)' : 'transparent',
    });
    return (
      <button title={title} onClick={onClick} style={styles}
        onMouseOver={e => { if (variant==='ghost' && !active) e.currentTarget.style.background = 'var(--bg-hover)'; }}
        onMouseOut={e => { if (variant==='ghost' && !active) e.currentTarget.style.background = 'transparent'; }}
      >
        {icon ? <span style={{display:'inline-flex', color: variant==='primary' ? 'var(--accent-fg)' : 'var(--fg-1)'}}>{icon}</span> : null}
        {children}
      </button>
    );
  }

  function IconBtn({ icon, title, onClick, active }) {
    return (
      <button title={title} onClick={onClick}
        style={{
          width: 28, height: 28, display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
          border: '1px solid transparent', borderRadius: 'var(--r-md)', cursor: 'pointer',
          background: active ? 'var(--bg-hover)' : 'transparent',
          color: 'var(--fg-1)', transition: 'background var(--dur-fast)',
        }}
        onMouseOver={e => { if (!active) e.currentTarget.style.background = 'var(--bg-hover)'; }}
        onMouseOut={e => { if (!active) e.currentTarget.style.background = 'transparent'; }}
      >
        {icon}
      </button>
    );
  }

  function Kbd({ children }) {
    return <span style={{
      fontFamily: 'var(--font-mono)', fontSize: 11,
      padding: '1px 5px', borderRadius: 3,
      border: '1px solid var(--border-1)', background: 'var(--bg-2)',
      color: 'var(--fg-1)',
    }}>{children}</span>;
  }

  function Chip({ children, tone, onClick }) {
    const toneColor = {
      downloading: 'var(--st-downloading)',
      seeding: 'var(--st-seeding)',
      paused: 'var(--st-paused)',
      queued: 'var(--st-queued)',
      error: 'var(--st-error)',
      checking: 'var(--st-checking)',
      stalled: 'var(--st-stalled)',
      complete: 'var(--st-complete)',
    }[tone] || 'var(--fg-2)';
    return (
      <span onClick={onClick} style={{
        display: 'inline-flex', alignItems: 'center', gap: 5,
        whiteSpace: 'nowrap',
        fontSize: 11, fontWeight: 500,
        padding: '2px 7px', borderRadius: 999,
        background: 'var(--bg-2)', color: 'var(--fg-1)',
        border: '1px solid var(--border-1)',
        cursor: onClick ? 'pointer' : 'default',
      }}>
        {tone ? <span style={{width: 6, height: 6, borderRadius:3, background: toneColor}}/> : null}
        {children}
      </span>
    );
  }

  function ProgressBar({ value=0, tone, height=6, showLabel=false }) {
    const color = {
      downloading: 'var(--st-downloading)',
      seeding: 'var(--st-seeding)',
      paused: 'var(--st-paused)',
      queued: 'var(--st-queued)',
      error: 'var(--st-error)',
      checking: 'var(--st-checking)',
      stalled: 'var(--st-stalled)',
      complete: 'var(--st-complete)',
    }[tone] || 'var(--accent)';
    return (
      <div style={{display:'flex', alignItems:'center', gap: 8, minWidth: 0, flex: 1}}>
        <div style={{
          flex: 1, height, background: 'var(--bg-inset)',
          borderRadius: height/2, overflow: 'hidden',
          border: '1px solid var(--border-1)',
        }}>
          <div style={{
            width: `${value*100}%`, height: '100%', background: color,
            transition: 'width var(--dur) var(--ease-out)',
          }}/>
        </div>
        {showLabel ? <span className="num" style={{fontSize: 11, color:'var(--fg-2)', minWidth: 36, textAlign:'right'}}>{(value*100).toFixed(1)}%</span> : null}
      </div>
    );
  }

  function SectionLabel({ children, right }) {
    return (
      <div style={{
        display:'flex', alignItems:'center', justifyContent:'space-between',
        padding: '10px 12px 4px', fontSize: 10, fontWeight: 600,
        color:'var(--fg-3)', textTransform: 'uppercase', letterSpacing: '.08em',
      }}>
        <span>{children}</span>
        {right}
      </div>
    );
  }

  function Divider({ v, style }) {
    return <div style={{
      [v ? 'width' : 'height']: 1,
      [v ? 'height' : 'width']: '100%',
      background: 'var(--divider)',
      ...style,
    }}/>;
  }

  function StatusDot({ tone }) {
    const c = {
      downloading: 'var(--st-downloading)',
      seeding: 'var(--st-seeding)',
      paused: 'var(--st-paused)',
      queued: 'var(--st-queued)',
      error: 'var(--st-error)',
      checking: 'var(--st-checking)',
      stalled: 'var(--st-stalled)',
    }[tone] || 'var(--fg-3)';
    const isActive = tone === 'downloading' || tone === 'checking';
    return (
      <span style={{
        position:'relative', display:'inline-flex', width: 8, height: 8,
        borderRadius: 4, background: c, flexShrink: 0,
      }}>
        {isActive ? <span style={{
          position:'absolute', inset: -2, borderRadius: 6,
          border: `1.5px solid ${c}`, opacity: 0.35,
          animation: 'it-pulse 1.8s var(--ease-out) infinite',
        }}/> : null}
      </span>
    );
  }

  function Toggle({ on, onChange, label }) {
    return (
      <label style={{display:'flex', alignItems:'flex-start', gap:8, cursor:'pointer', userSelect:'none', minWidth: 0}}>
        <span onClick={() => onChange && onChange(!on)} style={{
          width: 30, height: 18, borderRadius: 9, flexShrink: 0, marginTop: 1,
          background: on ? 'var(--accent)' : 'var(--bg-3)',
          border: `1px solid ${on ? 'var(--accent)' : 'var(--border-1)'}`,
          position:'relative', transition: 'background var(--dur-fast)',
        }}>
          <span style={{
            position:'absolute', top: 1, left: on ? 13 : 1,
            width: 14, height: 14, borderRadius: 7,
            background: 'white',
            transition: 'left var(--dur-fast) var(--ease-out)',
            boxShadow: '0 1px 2px rgba(0,0,0,.2)',
          }}/>
        </span>
        {label ? <span style={{flex: 1, minWidth: 0, lineHeight: 1.3}}>{label}</span> : null}
      </label>
    );
  }

  function TextInput({ value, onChange, placeholder, mono, width, right, type='text', suffix }) {
    return (
      <div style={{
        display:'inline-flex', alignItems:'center',
        height: 28, width: width || 'auto',
        padding: '0 8px',
        background: 'var(--bg-2)',
        border: '1px solid var(--border-1)',
        borderRadius: 'var(--r-md)',
      }}>
        <input type={type} value={value} placeholder={placeholder}
          onChange={e => onChange && onChange(e.target.value)}
          style={{
            border:'none', outline:'none', background:'transparent',
            color:'var(--fg-0)', fontSize: 13, flex: 1,
            fontFamily: mono ? 'var(--font-mono)' : 'var(--font-ui)',
            textAlign: right ? 'right' : 'left',
            width: '100%',
          }}
        />
        {suffix ? <span style={{color:'var(--fg-3)', fontSize: 12, marginLeft: 6}}>{suffix}</span> : null}
      </div>
    );
  }

  function Select({ value, options, onChange, width }) {
    return (
      <div style={{
        display:'inline-flex', alignItems:'center',
        height: 28, minWidth: width || 120,
        padding: '0 8px', gap: 8,
        background: 'var(--bg-2)',
        border: '1px solid var(--border-1)',
        borderRadius: 'var(--r-md)',
        cursor:'pointer', position: 'relative',
      }}>
        <select value={value} onChange={e => onChange && onChange(e.target.value)}
          style={{
            border:'none', outline:'none', background:'transparent',
            color:'var(--fg-0)', fontSize: 13, flex: 1,
            appearance: 'none', cursor:'pointer',
            paddingRight: 14,
          }}>
          {options.map(o => <option key={o.value||o} value={o.value||o}>{o.label||o}</option>)}
        </select>
        <span style={{position:'absolute', right: 6, color:'var(--fg-2)', pointerEvents:'none'}}>{Icon.chevronD({size:12})}</span>
      </div>
    );
  }

  window.IT = { Btn, IconBtn, Kbd, Chip, ProgressBar, SectionLabel, Divider, StatusDot, Toggle, TextInput, Select };
})();
