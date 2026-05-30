// IronTide — window chrome (cross-platform, custom).
// A neutral top bar with traffic-light-style close buttons but no OS-specific cues.

(() => {
  function WindowChrome({ title, subtitle, right, onMenu }) {
    return (
      <div style={{
        height: 40, flexShrink: 0,
        background: 'var(--bg-1)',
        borderBottom: '1px solid var(--border-1)',
        display: 'flex', alignItems: 'center', gap: 14,
        padding: '0 12px',
        userSelect: 'none',
      }}>
        {/* Window controls — platform-agnostic: three dots in neutral grays */}
        <div style={{display:'flex', gap: 6}}>
          {['close','min','max'].map((k, i) => (
            <span key={k} style={{
              width: 11, height: 11, borderRadius: 6,
              background: ['oklch(0.65 0.14 25)','oklch(0.78 0.11 80)','oklch(0.72 0.13 150)'][i],
              border: '1px solid rgba(0,0,0,.12)',
            }}/>
          ))}
        </div>
        <div style={{width: 1, height: 18, background: 'var(--divider)'}}/>
        {/* App mark — IronTide wordmark */}
        <div style={{display:'flex', alignItems:'center', gap: 8}}>
          <svg width="18" height="18" viewBox="0 0 18 18">
            <path d="M2 12c2-2 3-1 5 0s3 2 5 0 3-2 4 0" fill="none" stroke="var(--accent)" strokeWidth="1.5" strokeLinecap="round"/>
            <path d="M2 8c2-2 3-1 5 0s3 2 5 0 3-2 4 0" fill="none" stroke="var(--fg-1)" strokeWidth="1.2" strokeLinecap="round" opacity=".6"/>
            <circle cx="9" cy="4" r="1.4" fill="var(--accent)"/>
          </svg>
          <span style={{fontSize: 13, fontWeight: 600, letterSpacing: '-0.01em'}}>IronTide</span>
          {subtitle ? <span style={{fontSize: 12, color:'var(--fg-2)', marginLeft: 4}}>— {subtitle}</span> : null}
        </div>
        <div style={{flex:1}}/>
        {right}
      </div>
    );
  }

  function MenuBar({ onAction }) {
    const menus = ['File','Edit','View','Tools','Window','Help'];
    return (
      <div style={{
        height: 30, flexShrink: 0,
        background: 'var(--bg-1)',
        borderBottom: '1px solid var(--border-1)',
        display:'flex', alignItems:'center', padding:'0 6px',
        fontSize: 12,
      }}>
        {menus.map(m => (
          <button key={m} onClick={() => onAction && onAction(m)}
            style={{
              padding:'4px 10px', background:'transparent', border:'none',
              color:'var(--fg-1)', cursor:'pointer', borderRadius:'var(--r-sm)',
              fontSize: 12,
            }}
            onMouseOver={e=>e.currentTarget.style.background='var(--bg-hover)'}
            onMouseOut={e=>e.currentTarget.style.background='transparent'}
          >{m}</button>
        ))}
      </div>
    );
  }

  window.Chrome = { WindowChrome, MenuBar };
})();
