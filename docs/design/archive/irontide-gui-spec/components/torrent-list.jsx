// IronTide — main torrent list table.
(() => {
  const { Icon, IT } = window;

  const COLUMNS = [
    { id:'sel',     label:'',         w: 28, fixed: true },
    { id:'status',  label:'',         w: 24 },
    { id:'name',    label:'Name',     w: 360, flex: 1, minW: 200 },
    { id:'size',    label:'Size',     w: 78,  align:'right', mono: true },
    { id:'progress',label:'Progress', w: 130 },
    { id:'status_l',label:'Status',   w: 90 },
    { id:'seeds',   label:'Seeds',    w: 82,  align:'right', mono: true },
    { id:'peers',   label:'Peers',    w: 82,  align:'right', mono: true },
    { id:'dl',      label:'Down',     w: 84,  align:'right', mono: true },
    { id:'ul',      label:'Up',       w: 84,  align:'right', mono: true },
    { id:'ratio',   label:'Ratio',    w: 60,  align:'right', mono: true },
    { id:'eta',     label:'ETA',      w: 72,  align:'right', mono: true },
    { id:'category',label:'Category', w: 110 },
    { id:'tags',    label:'Tags',     w: 130 },
    { id:'added',   label:'Added',    w: 140, mono: true },
    { id:'tracker', label:'Tracker',  w: 180 },
  ];

  function TorrentList({ selected, setSelected, rows, striping }) {
    return (
      <div style={{flex:1, display:'flex', flexDirection:'column', minHeight: 0, background: 'var(--bg-0)'}}>
        {/* Header */}
        <div style={{
          display:'flex',
          background: 'var(--bg-1)',
          borderBottom: '1px solid var(--border-1)',
          height: 30, flexShrink: 0,
          fontSize: 11, fontWeight: 600,
          color:'var(--fg-2)',
          textTransform: 'uppercase',
          letterSpacing: '.04em',
          overflow: 'hidden',
        }}>
          {COLUMNS.map(c => (
            <div key={c.id} style={{
              width: c.flex ? undefined : c.w,
              flex: c.flex ? `${c.flex} 0 ${c.minW||0}px` : '0 0 auto',
              padding: '0 10px',
              display:'flex', alignItems:'center',
              justifyContent: c.align==='right' ? 'flex-end' : 'flex-start',
              borderRight: '1px solid var(--divider)',
              gap: 4,
            }}>
              {c.label}
              {c.id === 'name' ? <span style={{color:'var(--fg-3)'}}>{Icon.chevronD({size:10})}</span> : null}
            </div>
          ))}
        </div>

        {/* Rows */}
        <div style={{flex:1, overflowY:'auto'}}>
          {rows.map((t, i) => {
            const isSel = selected === t.id;
            return (
              <div key={t.id} onClick={() => setSelected(t.id)}
                style={{
                  display:'flex', height: 'var(--row-h)',
                  background: isSel ? 'var(--bg-selected)'
                            : (striping && i%2===1) ? 'var(--bg-1)' : 'transparent',
                  borderBottom: '1px solid var(--divider)',
                  cursor:'pointer',
                  fontSize: 12.5,
                }}
                onMouseOver={e=>{ if(!isSel) e.currentTarget.style.background='var(--bg-hover)'; }}
                onMouseOut={e=>{ if(!isSel) e.currentTarget.style.background = (striping && i%2===1) ? 'var(--bg-1)' : 'transparent'; }}
              >
                {/* Checkbox */}
                <div style={{width: 28, display:'flex', alignItems:'center', justifyContent:'center'}}>
                  <span style={{
                    width: 13, height: 13,
                    border: '1px solid var(--border-2)',
                    borderRadius: 3,
                    background: isSel ? 'var(--accent)' : 'var(--bg-2)',
                    color:'var(--accent-fg)',
                    display:'inline-flex', alignItems:'center', justifyContent:'center',
                  }}>{isSel ? Icon.check({size:10}) : null}</span>
                </div>
                {/* Status dot */}
                <div style={{width: 24, display:'flex', alignItems:'center', justifyContent:'center'}}>
                  <IT.StatusDot tone={t.status}/>
                </div>
                {/* Name */}
                <div style={{flex:'1 0 200px', padding:'0 10px', display:'flex', alignItems:'center', gap:8, minWidth: 0}}>
                  <span style={{whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{t.name}</span>
                </div>
                {/* Size */}
                <div className="mono" style={{width: 78, padding:'0 10px', textAlign:'right', display:'flex', alignItems:'center', justifyContent:'flex-end'}}>{t.size}</div>
                {/* Progress */}
                <div style={{width: 130, padding:'0 10px', display:'flex', alignItems:'center'}}>
                  <IT.ProgressBar value={t.progress} tone={t.status} showLabel/>
                </div>
                {/* Status label */}
                <div style={{width: 90, padding:'0 10px', display:'flex', alignItems:'center', textTransform:'capitalize', color:'var(--fg-1)'}}>
                  {t.status}
                </div>
                {/* Seeds */}
                <div className="mono" style={{width: 82, padding:'0 10px', textAlign:'right', display:'flex', alignItems:'center', justifyContent:'flex-end', color:'var(--fg-1)'}}>{t.seeds}</div>
                <div className="mono" style={{width: 82, padding:'0 10px', textAlign:'right', display:'flex', alignItems:'center', justifyContent:'flex-end', color:'var(--fg-1)'}}>{t.peers}</div>
                <div className="mono" style={{width: 84, padding:'0 10px', textAlign:'right', display:'flex', alignItems:'center', justifyContent:'flex-end', color: t.dl!=='0 B/s' ? 'var(--st-downloading)' : 'var(--fg-2)'}}>{t.dl}</div>
                <div className="mono" style={{width: 84, padding:'0 10px', textAlign:'right', display:'flex', alignItems:'center', justifyContent:'flex-end', color: t.ul!=='0 B/s' ? 'var(--st-seeding)' : 'var(--fg-2)'}}>{t.ul}</div>
                <div className="mono" style={{width: 60, padding:'0 10px', textAlign:'right', display:'flex', alignItems:'center', justifyContent:'flex-end', color: t.ratio >= 1 ? 'var(--st-seeding)' : 'var(--fg-1)'}}>{t.ratio.toFixed(2)}</div>
                <div className="mono" style={{width: 72, padding:'0 10px', textAlign:'right', display:'flex', alignItems:'center', justifyContent:'flex-end', color:'var(--fg-2)'}}>{t.eta}</div>
                <div style={{width: 110, padding:'0 10px', display:'flex', alignItems:'center'}}>
                  {t.category ? <IT.Chip>{t.category}</IT.Chip> : null}
                </div>
                <div style={{width: 130, padding:'0 10px', display:'flex', alignItems:'center', gap: 4, overflow:'hidden'}}>
                  {t.tags.slice(0,2).map(tg => <IT.Chip key={tg}>{tg}</IT.Chip>)}
                </div>
                <div className="mono" style={{width: 140, padding:'0 10px', display:'flex', alignItems:'center', color:'var(--fg-2)', fontSize: 11.5}}>{t.added}</div>
                <div style={{width: 180, padding:'0 10px', display:'flex', alignItems:'center', color:'var(--fg-2)', fontSize: 11.5, whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{t.tracker}</div>
              </div>
            );
          })}
        </div>

        {/* Status bar */}
        <div style={{
          height: 24, flexShrink: 0,
          borderTop: '1px solid var(--border-1)',
          background: 'var(--bg-1)',
          display:'flex', alignItems:'center', padding:'0 12px',
          fontSize: 11, color:'var(--fg-2)', gap: 16,
        }}>
          <span>{rows.length} torrents</span>
          <span>{rows.filter(t=>t.status==='downloading').length} active</span>
          <span style={{flex:1}}/>
          <span className="mono">⇣ 13.7 MB/s · ⇡ 3.2 MB/s · 128 peers · DHT: 1,024 nodes</span>
        </div>
      </div>
    );
  }

  window.TorrentList = TorrentList;
})();
