// IronTide — top toolbar + global search / command bar.
(() => {
  const { Icon, IT } = window;

  function Toolbar({ onAdd, onAddMagnet, view, setView, layoutVariant, setLayoutVariant, openPrefs, openCommand, dl, ul, connections, theme, setTheme }) {
    return (
      <div style={{
        height: 'var(--chrome-h)', flexShrink: 0,
        background: 'var(--bg-0)',
        borderBottom: '1px solid var(--border-1)',
        display:'flex', alignItems:'center', gap: 10, padding:'0 10px',
      }}>
        <IT.Btn variant="primary" icon={Icon.add({size:14})} onClick={onAdd}>Add</IT.Btn>
        <IT.Btn variant="solid" icon={Icon.magnet({size:14})} onClick={onAddMagnet}>Magnet</IT.Btn>
        <IT.Divider v style={{margin:'0 4px', height: 22}}/>
        <IT.IconBtn icon={Icon.play({size:16})} title="Resume (Space)"/>
        <IT.IconBtn icon={Icon.pause({size:16})} title="Pause"/>
        <IT.IconBtn icon={Icon.up({size:16})} title="Queue up"/>
        <IT.IconBtn icon={Icon.down({size:16})} title="Queue down"/>
        <IT.IconBtn icon={Icon.remove({size:16})} title="Remove"/>
        <IT.Divider v style={{margin:'0 4px', height: 22}}/>

        {/* Global command search */}
        <button onClick={openCommand} style={{
          display:'flex', alignItems:'center', gap: 8,
          height: 28, padding:'0 10px', minWidth: 280, flex:'0 1 440px',
          background:'var(--bg-2)', border:'1px solid var(--border-1)',
          borderRadius:'var(--r-md)', cursor:'pointer',
          color:'var(--fg-2)', fontSize: 13,
        }}>
          <span style={{color:'var(--fg-2)'}}>{Icon.cmd({size:14})}</span>
          <span style={{flex:1, textAlign:'left'}}>Jump to torrent, action, or setting…</span>
          <IT.Kbd>⌘</IT.Kbd><IT.Kbd>K</IT.Kbd>
        </button>

        <div style={{flex:1}}/>

        {/* Live transfer readout */}
        <div style={{
          display:'flex', gap: 14, padding:'0 12px',
          fontFamily:'var(--font-mono)', fontSize: 12,
          color:'var(--fg-1)',
        }}>
          <span style={{display:'inline-flex', alignItems:'center', gap: 5}}>
            <span style={{color:'var(--st-downloading)'}}>{Icon.download({size:12})}</span>
            <span className="num">{dl}</span>
          </span>
          <span style={{display:'inline-flex', alignItems:'center', gap: 5}}>
            <span style={{color:'var(--st-seeding)'}}>{Icon.upload({size:12})}</span>
            <span className="num">{ul}</span>
          </span>
          <span style={{color:'var(--fg-2)'}}>· {connections} peers</span>
        </div>

        <IT.Divider v style={{margin:'0 2px', height: 22}}/>

        {/* Layout variant */}
        <IT.IconBtn icon={Icon.layout1({size:15})} title="Layout: 3-pane"          active={layoutVariant==='L1'} onClick={()=>setLayoutVariant('L1')}/>
        <IT.IconBtn icon={Icon.layout2({size:15})} title="Layout: Inspector drawer" active={layoutVariant==='L2'} onClick={()=>setLayoutVariant('L2')}/>
        <IT.IconBtn icon={Icon.layout3({size:15})} title="Layout: Command workspace" active={layoutVariant==='L3'} onClick={()=>setLayoutVariant('L3')}/>
        <IT.IconBtn icon={theme==='dark' ? Icon.sun({size:15}) : Icon.moon({size:15})} title="Toggle theme" onClick={()=>setTheme(theme==='dark'?'light':'dark')}/>
        <IT.IconBtn icon={Icon.gear({size:16})} title="Preferences (⌘,)" onClick={openPrefs}/>
      </div>
    );
  }

  window.Toolbar = Toolbar;
})();
