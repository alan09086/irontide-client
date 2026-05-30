// IronTide — modals for Add-Torrent, Create-Torrent, Command Palette.
(() => {
  const { Icon, IT } = window;

  function ModalShell({ title, onClose, width=720, height=560, footer, children }) {
    return (
      <div style={{
        position:'absolute', inset: 0,
        background: 'oklch(0.15 0.01 240 / 0.45)',
        display:'flex', alignItems:'center', justifyContent:'center',
        zIndex: 50,
      }} onClick={onClose}>
        <div onClick={e=>e.stopPropagation()} style={{
          width, maxWidth:'94vw', height, maxHeight:'92vh',
          background:'var(--bg-0)', borderRadius:'var(--r-lg)',
          border:'1px solid var(--border-1)', boxShadow:'var(--shadow-lg)',
          display:'flex', flexDirection:'column', overflow:'hidden',
        }}>
          <div style={{
            height: 42, flexShrink:0, padding:'0 14px',
            display:'flex', alignItems:'center',
            borderBottom:'1px solid var(--border-1)', background:'var(--bg-1)',
          }}>
            <span style={{fontSize: 13, fontWeight: 600}}>{title}</span>
            <div style={{flex:1}}/>
            <IT.IconBtn icon={Icon.x({size:14})} onClick={onClose}/>
          </div>
          <div style={{flex:1, overflowY:'auto', padding: 18}}>{children}</div>
          {footer ? (
            <div style={{height: 50, flexShrink: 0, borderTop:'1px solid var(--border-1)', background:'var(--bg-1)', display:'flex', alignItems:'center', justifyContent:'flex-end', padding:'0 14px', gap: 8}}>
              {footer}
            </div>
          ) : null}
        </div>
      </div>
    );
  }

  function AddTorrentDialog({ onClose }) {
    const [tab, setTab] = React.useState('file');
    const [startPaused, setStartPaused] = React.useState(false);
    const [skip, setSkip] = React.useState(false);
    const [sequential, setSequential] = React.useState(false);
    const [firstLast, setFirstLast] = React.useState(false);

    return (
      <ModalShell
        title="Add torrent"
        onClose={onClose}
        width={820} height={620}
        footer={<>
          <span style={{flex:1, fontSize: 12, color:'var(--fg-3)'}}>Verifying pieces locally is faster than re-downloading if you already have the files.</span>
          <IT.Btn variant="ghost" onClick={onClose}>Cancel</IT.Btn>
          <IT.Btn variant="primary" onClick={onClose}>Add torrent</IT.Btn>
        </>}
      >
        {/* Source tabs */}
        <div style={{display:'flex', gap: 0, marginBottom: 16, borderBottom:'1px solid var(--border-1)'}}>
          {[
            {id:'file',label:'From .torrent file', icon: Icon.file},
            {id:'magnet',label:'Magnet link', icon: Icon.magnet},
            {id:'url',label:'URL', icon: Icon.link},
          ].map(t => (
            <button key={t.id} onClick={()=>setTab(t.id)} style={{
              display:'inline-flex', alignItems:'center', gap: 6,
              padding: '8px 14px', border:'none', background:'transparent', cursor:'pointer',
              color: tab===t.id ? 'var(--fg-0)' : 'var(--fg-2)', fontSize: 12.5,
              borderBottom: tab===t.id ? '2px solid var(--accent)' : '2px solid transparent',
              marginBottom: -1,
            }}>
              {t.icon({size:14})}{t.label}
            </button>
          ))}
        </div>

        {tab==='file' && (
          <div style={{
            border:'1.5px dashed var(--border-2)', borderRadius:'var(--r-lg)',
            padding: 20, background:'var(--bg-1)',
            display:'flex', alignItems:'center', gap: 14, marginBottom: 14,
          }}>
            <div style={{
              width: 44, height: 44, borderRadius:'var(--r-md)',
              background:'var(--bg-2)', color:'var(--accent)',
              display:'flex', alignItems:'center', justifyContent:'center',
            }}>{Icon.file({size:22})}</div>
            <div style={{flex:1}}>
              <div className="mono" style={{fontSize:13, color:'var(--fg-0)'}}>Blender.4.2.LTS.Splash.Project.Files.torrent</div>
              <div style={{fontSize: 11, color:'var(--fg-3)', marginTop: 2}}>18.4 GB · 18,900 pieces · 1.0 MiB each · v1 info hash</div>
            </div>
            <IT.Btn variant="solid">Choose file…</IT.Btn>
          </div>
        )}

        {tab==='magnet' && (
          <div style={{marginBottom: 14}}>
            <div style={{fontSize: 11, color:'var(--fg-2)', marginBottom: 4}}>Paste one or more magnet links (one per line)</div>
            <textarea defaultValue="magnet:?xt=urn:btih:a1f9c8b7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1&dn=Blender.4.2.LTS.Splash&tr=udp%3A%2F%2Fopen.blender.tracker%3A80"
              style={{
                width: '100%', height: 120, padding: 10,
                background:'var(--bg-2)', border:'1px solid var(--border-1)',
                borderRadius:'var(--r-md)', color:'var(--fg-0)',
                fontFamily:'var(--font-mono)', fontSize: 12, resize:'vertical',
              }}/>
          </div>
        )}

        {tab==='url' && (
          <div style={{marginBottom: 14}}>
            <div style={{fontSize: 11, color:'var(--fg-2)', marginBottom: 4}}>Fetch .torrent from URL (http/https)</div>
            <IT.TextInput mono placeholder="https://example.org/release.torrent" width="100%"/>
          </div>
        )}

        {/* Preview card */}
        <div style={{
          background: 'var(--bg-1)', border: '1px solid var(--border-1)',
          borderRadius: 'var(--r-md)', padding: 12, marginBottom: 14,
        }}>
          <div style={{fontSize: 11, fontWeight: 600, textTransform:'uppercase', letterSpacing:'.06em', color:'var(--fg-2)', marginBottom: 8}}>Torrent info</div>
          <div style={{display:'grid', gridTemplateColumns:'140px 1fr 120px 1fr', gap:'6px 14px', fontSize: 12}}>
            <span style={{color:'var(--fg-2)'}}>Name</span>
            <span className="mono">Blender.4.2.LTS.Splash.Project.Files.zip</span>
            <span style={{color:'var(--fg-2)'}}>Size</span>
            <span className="mono">18.4 GB</span>
            <span style={{color:'var(--fg-2)'}}>Pieces</span>
            <span className="mono">18,900 × 1.0 MiB</span>
            <span style={{color:'var(--fg-2)'}}>Files</span>
            <span className="mono">11</span>
            <span style={{color:'var(--fg-2)'}}>Comment</span>
            <span style={{gridColumn:'span 3'}}>Official Blender 4.2 LTS splash project. CC-BY.</span>
            <span style={{color:'var(--fg-2)'}}>Info hash</span>
            <span className="mono" style={{gridColumn:'span 3', fontSize: 11}}>a1f9c8b7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1</span>
          </div>
        </div>

        {/* Options */}
        <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 14}}>
          <div>
            <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Save to</div>
            <div style={{display:'flex', gap: 6}}>
              <IT.TextInput mono value="/Volumes/Storage/Torrents/Software" width="100%"/>
              <IT.Btn variant="solid">Browse…</IT.Btn>
            </div>
            <div style={{marginTop: 10, fontSize:11, color:'var(--fg-2)'}}>Category</div>
            <IT.Select value="Software" options={['(none)','Linux','Software','Video','Books','Papers','Datasets']} width={260}/>
            <div style={{marginTop: 10, fontSize:11, color:'var(--fg-2)'}}>Tags</div>
            <IT.TextInput placeholder="Add tag…" width={260}/>
          </div>
          <div style={{display:'flex', flexDirection:'column', gap: 6}}>
            <IT.Toggle on={startPaused} onChange={setStartPaused} label="Start paused"/>
            <IT.Toggle on={skip} onChange={setSkip} label="Skip hash check"/>
            <IT.Toggle on={true} label="Pre-allocate disk space"/>
            <IT.Toggle on={sequential} onChange={setSequential} label="Download in sequential order"/>
            <IT.Toggle on={firstLast} onChange={setFirstLast} label="Download first & last pieces first"/>
            <IT.Toggle on={false} label="Use alternative speed limits"/>
            <IT.Toggle on={true} label="Auto-manage (queue)"/>
          </div>
        </div>

        {/* File selection mini-table */}
        <div style={{marginTop: 14}}>
          <div style={{fontSize:11, fontWeight: 600, textTransform:'uppercase', color:'var(--fg-2)', marginBottom: 6}}>Files (11)</div>
          <div style={{
            background:'var(--bg-1)', border:'1px solid var(--border-1)',
            borderRadius:'var(--r-md)', maxHeight: 180, overflow:'auto',
          }}>
            {MOCK.fileTree[0].children.flatMap(d => d.children || [d]).map((f, i) => (
              <div key={i} style={{display:'flex', alignItems:'center', gap: 8, padding:'6px 10px', borderBottom:'1px solid var(--divider)', fontSize: 12}}>
                <span style={{width: 13, height: 13, border:'1px solid var(--border-2)', borderRadius: 3, background: 'var(--accent)', color:'var(--accent-fg)', display:'inline-flex', alignItems:'center', justifyContent:'center'}}>
                  {Icon.check({size:10})}
                </span>
                <span style={{color:'var(--fg-2)'}}>{Icon.file({size:13})}</span>
                <span style={{flex:1}}>{f.name}</span>
                <span className="mono" style={{color:'var(--fg-2)', fontSize: 11.5}}>{f.size}</span>
                <IT.Select value="Normal" options={['High','Normal','Low','Skip']} width={90}/>
              </div>
            ))}
          </div>
        </div>
      </ModalShell>
    );
  }

  function CreateTorrentDialog({ onClose }) {
    return (
      <ModalShell
        title="Create new torrent"
        onClose={onClose}
        width={720} height={560}
        footer={<>
          <IT.Btn variant="ghost" onClick={onClose}>Cancel</IT.Btn>
          <IT.Btn variant="primary">Create & save…</IT.Btn>
        </>}
      >
        <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Source file or folder</div>
        <div style={{display:'flex', gap: 6, marginBottom: 14}}>
          <IT.TextInput mono value="/Volumes/Storage/MyAlbum" width="100%"/>
          <IT.Btn variant="solid">Select file</IT.Btn>
          <IT.Btn variant="solid">Select folder</IT.Btn>
        </div>

        <div style={{fontSize:11, fontWeight: 600, textTransform:'uppercase', color:'var(--fg-2)', marginBottom: 6}}>Trackers</div>
        <textarea defaultValue="udp://tracker.openbittorrent.com:80/announce
udp://tracker.opentrackr.org:1337/announce
udp://open.stealth.si:80/announce"
          style={{width: '100%', height: 90, padding: 10, background:'var(--bg-2)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', color:'var(--fg-0)', fontFamily:'var(--font-mono)', fontSize: 12, resize:'vertical', marginBottom: 14}}/>

        <div style={{fontSize:11, fontWeight: 600, textTransform:'uppercase', color:'var(--fg-2)', marginBottom: 6}}>Web seeds (HTTP sources, one per line)</div>
        <textarea placeholder="https://mirror.example.com/myalbum/"
          style={{width: '100%', height: 60, padding: 10, background:'var(--bg-2)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', color:'var(--fg-0)', fontFamily:'var(--font-mono)', fontSize: 12, resize:'vertical', marginBottom: 14}}/>

        <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 14}}>
          <div>
            <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Piece size</div>
            <IT.Select value="Auto" options={['Auto','16 KiB','32 KiB','64 KiB','128 KiB','256 KiB','512 KiB','1 MiB','2 MiB','4 MiB','8 MiB','16 MiB']} width="100%"/>
            <div style={{marginTop: 10, fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Torrent format</div>
            <IT.Select value="Hybrid (v1 + v2)" options={['v1 (legacy)','v2 (BEP-52)','Hybrid (v1 + v2)']} width="100%"/>
            <div style={{marginTop: 10, fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Source (optional)</div>
            <IT.TextInput mono placeholder="My Private Tracker" width="100%"/>
          </div>
          <div style={{display:'flex', flexDirection:'column', gap: 6}}>
            <IT.Toggle on={true} label="Private torrent (disable DHT, PeX, LSD)"/>
            <IT.Toggle on={false} label="Optimize alignment"/>
            <IT.Toggle on={true} label="Calculate number of pieces automatically"/>
            <IT.Toggle on={true} label="Start seeding immediately"/>
            <IT.Toggle on={false} label="Ignore share ratio limits for this torrent"/>
            <div style={{marginTop: 8, fontSize:11, color:'var(--fg-2)'}}>Comment</div>
            <IT.TextInput placeholder="My album — released under CC-BY" width="100%"/>
          </div>
        </div>

        <div style={{marginTop: 14, padding: 10, background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', fontSize: 11.5, color:'var(--fg-2)'}}>
          <b style={{color:'var(--fg-1)'}}>Estimate:</b> 4,248 pieces @ 1 MiB &nbsp;·&nbsp; Total: 4.15 GB &nbsp;·&nbsp; SHA-1 hashing ≈ 12s on this machine
        </div>
      </ModalShell>
    );
  }

  function CommandPalette({ onClose }) {
    const [q, setQ] = React.useState('');
    const cmds = [
      {cat:'Action', items:['Add torrent…','Add magnet link…','Pause all','Resume all','Toggle alternative speed limits','Recheck selected','Force reannounce']},
      {cat:'Navigation', items:['Go to All torrents','Go to Downloading','Go to Seeding','Go to Search','Go to RSS','Go to Scheduler','Go to Statistics','Go to Logs','Open Preferences']},
      {cat:'Tools', items:['Create new torrent…','Export .torrent…','Reveal save folder','Copy magnet link','Clean up stale .torrent files']},
      {cat:'Jump to torrent', items: MOCK.torrents.slice(0,4).map(t => t.name)},
    ];
    const matches = cmds.map(c => ({
      ...c,
      items: c.items.filter(i => !q || i.toLowerCase().includes(q.toLowerCase())),
    })).filter(c => c.items.length);

    return (
      <div style={{
        position:'absolute', inset: 0,
        background: 'oklch(0.15 0.01 240 / 0.35)',
        display:'flex', alignItems:'flex-start', justifyContent:'center', paddingTop: 100,
        zIndex: 60,
      }} onClick={onClose}>
        <div onClick={e=>e.stopPropagation()} style={{
          width: 620, maxWidth: '92vw',
          background:'var(--bg-0)', borderRadius:'var(--r-lg)',
          border:'1px solid var(--border-1)', boxShadow:'var(--shadow-lg)',
          overflow:'hidden', display:'flex', flexDirection:'column',
          maxHeight: '60vh',
        }}>
          <div style={{display:'flex', alignItems:'center', gap: 10, padding:'12px 16px', borderBottom: '1px solid var(--divider)'}}>
            <span style={{color:'var(--fg-2)'}}>{Icon.cmd({size:16})}</span>
            <input autoFocus value={q} onChange={e=>setQ(e.target.value)} placeholder="Type a command or search…"
              style={{border:'none', outline:'none', background:'transparent', flex:1, fontSize: 15, color:'var(--fg-0)'}}/>
            <IT.Kbd>esc</IT.Kbd>
          </div>
          <div style={{flex:1, overflowY:'auto', padding: 6}}>
            {matches.map(c => (
              <div key={c.cat}>
                <div style={{padding:'8px 12px 4px', fontSize: 10, fontWeight: 600, textTransform:'uppercase', letterSpacing: '.08em', color:'var(--fg-3)'}}>{c.cat}</div>
                {c.items.map((it, i) => (
                  <div key={i} style={{
                    display:'flex', alignItems:'center', gap: 8,
                    padding: '8px 12px', borderRadius:'var(--r-md)',
                    cursor:'pointer', fontSize: 13,
                  }}
                  onMouseOver={e=>e.currentTarget.style.background='var(--bg-hover)'}
                  onMouseOut={e=>e.currentTarget.style.background='transparent'}
                  >
                    <span style={{color:'var(--fg-3)'}}>{Icon.chevronR({size:12})}</span>
                    <span style={{flex:1}}>{it}</span>
                  </div>
                ))}
              </div>
            ))}
          </div>
          <div style={{padding:'8px 12px', borderTop:'1px solid var(--divider)', display:'flex', gap: 14, fontSize: 11, color:'var(--fg-3)'}}>
            <span><IT.Kbd>↑</IT.Kbd> <IT.Kbd>↓</IT.Kbd> navigate</span>
            <span><IT.Kbd>↵</IT.Kbd> select</span>
            <span><IT.Kbd>⌘</IT.Kbd><IT.Kbd>K</IT.Kbd> open/close</span>
            <span style={{flex:1}}/>
            <span>{matches.reduce((a,c)=>a+c.items.length,0)} results</span>
          </div>
        </div>
      </div>
    );
  }

  window.Modals = { AddTorrentDialog, CreateTorrentDialog, CommandPalette };
})();
