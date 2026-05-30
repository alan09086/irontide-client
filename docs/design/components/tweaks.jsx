// IronTide — Tweaks panel (toolbar-toggled).
(() => {
  const { Icon, IT } = window;

  function TweaksPanel({ tweaks, setTweaks, onClose }) {
    const set = (k, v) => setTweaks({...tweaks, [k]: v});
    return (
      <div style={{
        position:'absolute', right: 16, bottom: 40,
        width: 280, maxHeight: 'calc(100% - 80px)',
        background:'var(--bg-0)', border:'1px solid var(--border-2)',
        borderRadius:'var(--r-lg)', boxShadow:'var(--shadow-lg)',
        display:'flex', flexDirection:'column', overflow:'hidden',
        zIndex: 40, fontSize: 12.5,
      }}>
        <div style={{padding:'10px 12px', borderBottom:'1px solid var(--border-1)', display:'flex', alignItems:'center', gap: 8}}>
          <span style={{color:'var(--accent)'}}>{Icon.gear({size:14})}</span>
          <span style={{fontWeight: 600, flex:1}}>Tweaks</span>
          <IT.IconBtn icon={Icon.x({size:12})} onClick={onClose}/>
        </div>
        <div style={{overflowY:'auto', padding: 12, display:'flex', flexDirection:'column', gap: 14}}>
          <div style={{fontSize: 11, color:'var(--fg-2)', lineHeight: 1.45}}>
            Visual direction is locked to the IronTide system. These are the only runtime knobs.
          </div>

          <div>
            <div style={{fontSize: 10.5, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 6}}>Density</div>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 1fr', gap: 6}}>
              {['compact','balanced','spacious'].map(s => (
                <button key={s} onClick={()=>set('density', s)} style={{
                  padding: '6px 4px', textTransform:'capitalize',
                  background: tweaks.density===s ? 'var(--bg-selected)' : 'var(--bg-2)',
                  border: '1px solid ' + (tweaks.density===s ? 'var(--accent)' : 'var(--border-1)'),
                  borderRadius:'var(--r-md)', cursor:'pointer', color:'var(--fg-0)', fontSize: 11.5,
                }}>{s}</button>
              ))}
            </div>
          </div>

          <div>
            <div style={{fontSize: 10.5, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 6}}>Sidebar</div>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 1fr', gap: 6}}>
              {['full','icons','hidden'].map(s => (
                <button key={s} onClick={()=>set('sidebar', s)} style={{
                  padding: '6px 4px', textTransform:'capitalize',
                  background: tweaks.sidebar===s ? 'var(--bg-selected)' : 'var(--bg-2)',
                  border: '1px solid ' + (tweaks.sidebar===s ? 'var(--accent)' : 'var(--border-1)'),
                  borderRadius:'var(--r-md)', cursor:'pointer', color:'var(--fg-0)', fontSize: 11.5,
                }}>{s}</button>
              ))}
            </div>
          </div>

          <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
            <span>Row striping</span>
            <IT.Toggle on={tweaks.striping} onChange={v=>set('striping', v)}/>
          </div>
          <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
            <span>Emerald glow</span>
            <IT.Toggle on={tweaks.accentGlow} onChange={v=>set('accentGlow', v)}/>
          </div>
        </div>
      </div>
    );
  }

  window.TweaksPanel = TweaksPanel;
})();
