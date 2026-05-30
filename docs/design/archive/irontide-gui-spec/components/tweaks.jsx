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
          <div>
            <div style={{fontSize: 10.5, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 6}}>Skin</div>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 1fr', gap: 6}}>
              {['tide','forge','abyss'].map(s => (
                <button key={s} onClick={()=>set('skin', s)} style={{
                  padding: '8px 6px',
                  background: tweaks.skin===s ? 'var(--bg-selected)' : 'var(--bg-2)',
                  border: '1px solid ' + (tweaks.skin===s ? 'var(--accent)' : 'var(--border-1)'),
                  borderRadius:'var(--r-md)', cursor:'pointer',
                  color:'var(--fg-0)', fontSize: 12, textTransform:'capitalize',
                }}>{s}</button>
              ))}
            </div>
          </div>

          <div>
            <div style={{fontSize: 10.5, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 6}}>Theme</div>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 6}}>
              {['light','dark'].map(s => (
                <button key={s} onClick={()=>set('theme', s)} style={{
                  padding: '6px', textTransform:'capitalize',
                  background: tweaks.theme===s ? 'var(--bg-selected)' : 'var(--bg-2)',
                  border: '1px solid ' + (tweaks.theme===s ? 'var(--accent)' : 'var(--border-1)'),
                  borderRadius:'var(--r-md)', cursor:'pointer', color:'var(--fg-0)', fontSize: 12,
                }}>{s}</button>
              ))}
            </div>
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

          <div>
            <div style={{fontSize: 10.5, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 6}}>Layout variant</div>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 1fr', gap: 6}}>
              {[
                {id:'L1',label:'3-pane'},{id:'L2',label:'Drawer'},{id:'L3',label:'Command'},
              ].map(s => (
                <button key={s.id} onClick={()=>set('layoutVariant', s.id)} style={{
                  padding: '6px 4px',
                  background: tweaks.layoutVariant===s.id ? 'var(--bg-selected)' : 'var(--bg-2)',
                  border: '1px solid ' + (tweaks.layoutVariant===s.id ? 'var(--accent)' : 'var(--border-1)'),
                  borderRadius:'var(--r-md)', cursor:'pointer', color:'var(--fg-0)', fontSize: 11.5,
                }}>{s.label}</button>
              ))}
            </div>
          </div>

          <div>
            <div style={{fontSize: 10.5, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 6}}>Radius</div>
            <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 1fr', gap: 6}}>
              {['sharp','balanced','rounded'].map(s => (
                <button key={s} onClick={()=>set('radius', s)} style={{
                  padding: '6px 4px', textTransform:'capitalize',
                  background: tweaks.radius===s ? 'var(--bg-selected)' : 'var(--bg-2)',
                  border: '1px solid ' + (tweaks.radius===s ? 'var(--accent)' : 'var(--border-1)'),
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
            <span>Platform chrome</span>
            <IT.Select value={tweaks.platform} onChange={v=>set('platform', v)} options={['mac','windows','linux']} width={100}/>
          </div>
          <div style={{display:'flex', alignItems:'center', justifyContent:'space-between'}}>
            <span>Font (UI)</span>
            <IT.Select value={tweaks.font} onChange={v=>set('font', v)} options={['Inter','System','Helvetica','IBM Plex Sans']} width={130}/>
          </div>
        </div>
      </div>
    );
  }

  window.TweaksPanel = TweaksPanel;
})();
