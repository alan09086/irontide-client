// IronTide — details pane with all tabs: General, Trackers, Peers, HTTP Sources, Content (files), Speed.
(() => {
  const { Icon, IT } = window;

  function Row({ label, value, mono }) {
    return (
      <div style={{display:'flex', padding:'5px 0', gap: 12, alignItems:'baseline'}}>
        <div style={{width: 140, color:'var(--fg-2)', fontSize: 12, flexShrink:0}}>{label}</div>
        <div className={mono?'mono':''} style={{fontSize: 12.5, color:'var(--fg-0)', flex:1, minWidth:0, wordBreak:'break-all'}}>{value}</div>
      </div>
    );
  }

  function Card({ title, children, right }) {
    return (
      <div style={{
        background:'var(--bg-1)',
        border:'1px solid var(--border-1)',
        borderRadius:'var(--r-md)',
        padding: 14, marginBottom: 10,
      }}>
        <div style={{display:'flex', alignItems:'center', justifyContent:'space-between', marginBottom: 6}}>
          <div style={{fontSize: 11, fontWeight: 600, textTransform:'uppercase', letterSpacing: '.06em', color:'var(--fg-2)'}}>{title}</div>
          {right}
        </div>
        {children}
      </div>
    );
  }

  function SpeedGraph({ data, height = 120 }) {
    const w = 560;
    const max = Math.max(...data.map(p => Math.max(p.dl, p.ul))) * 1.15 || 10;
    const path = (key) => {
      const pts = data.map((p, i) => [(i/(data.length-1))*w, height - (p[key]/max)*height]);
      return 'M' + pts.map(p=>p.join(',')).join(' L');
    };
    const area = (key) => path(key) + ` L${w},${height} L0,${height} Z`;
    return (
      <div style={{width:'100%', overflow:'hidden'}}>
        <svg width="100%" height={height} viewBox={`0 0 ${w} ${height}`} preserveAspectRatio="none">
          <defs>
            <linearGradient id="g-dl" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--st-downloading)" stopOpacity=".35"/>
              <stop offset="100%" stopColor="var(--st-downloading)" stopOpacity="0"/>
            </linearGradient>
            <linearGradient id="g-ul" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--st-seeding)" stopOpacity=".25"/>
              <stop offset="100%" stopColor="var(--st-seeding)" stopOpacity="0"/>
            </linearGradient>
          </defs>
          {[0.25, 0.5, 0.75].map(f => (
            <line key={f} x1="0" x2={w} y1={height*f} y2={height*f} stroke="var(--divider)" strokeDasharray="2 3"/>
          ))}
          <path d={area('dl')} fill="url(#g-dl)"/>
          <path d={path('dl')} fill="none" stroke="var(--st-downloading)" strokeWidth="1.5"/>
          <path d={area('ul')} fill="url(#g-ul)"/>
          <path d={path('ul')} fill="none" stroke="var(--st-seeding)" strokeWidth="1.5"/>
        </svg>
        <div style={{display:'flex', gap: 16, fontSize: 11, color:'var(--fg-2)', marginTop: 6}}>
          <span style={{display:'inline-flex', alignItems:'center', gap: 5}}>
            <span style={{width:8, height:2, background:'var(--st-downloading)'}}/> Download
          </span>
          <span style={{display:'inline-flex', alignItems:'center', gap: 5}}>
            <span style={{width:8, height:2, background:'var(--st-seeding)'}}/> Upload
          </span>
          <span style={{flex:1}}/>
          <span className="mono">max {max.toFixed(1)} MB/s · 60s window</span>
        </div>
      </div>
    );
  }

  function FileTree({ nodes, depth = 0 }) {
    return (
      <div>
        {nodes.map((n, i) => (
          <React.Fragment key={i}>
            <div style={{
              display:'flex', alignItems:'center', gap: 6,
              height: 26, padding: `0 8px 0 ${8 + depth*16}px`,
              borderBottom: '1px solid var(--divider)',
              fontSize: 12.5,
            }}>
              <span style={{color:'var(--fg-3)', width: 14, display:'inline-flex'}}>
                {n.kind==='dir' ? Icon.chevronD({size:11}) : null}
              </span>
              <span style={{color: n.kind==='dir' ? 'var(--accent)' : 'var(--fg-2)', width: 16, display:'inline-flex'}}>
                {n.kind==='dir' ? Icon.folder({size:14}) : Icon.file({size:14})}
              </span>
              <span style={{flex:'1 0 200px', whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{n.name}</span>
              <span className="mono" style={{width: 80, textAlign:'right', color:'var(--fg-2)'}}>{n.size || ''}</span>
              <span style={{width: 140}}><IT.ProgressBar value={n.progress} tone={n.progress===1?'complete':'downloading'} showLabel/></span>
              <span style={{width: 90, fontSize: 11, color:'var(--fg-2)', textAlign:'right', textTransform:'capitalize'}}>
                {n.priority==='high' ? '↑ high' : n.priority==='low' ? '↓ low' : n.priority==='skip' ? '✕ skip' : 'normal'}
              </span>
            </div>
            {n.children ? <FileTree nodes={n.children} depth={depth+1}/> : null}
          </React.Fragment>
        ))}
      </div>
    );
  }

  function DetailsPane({ torrent, onClose }) {
    const [tab, setTab] = React.useState('general');
    if (!torrent) {
      return (
        <div style={{
          display:'flex', alignItems:'center', justifyContent:'center',
          flex:1, color:'var(--fg-3)', fontSize: 13,
          background:'var(--bg-0)',
        }}>
          <div style={{textAlign:'center'}}>
            <div style={{margin:'0 auto 10px', color:'var(--fg-3)'}}>{Icon.list({size:28})}</div>
            Select a torrent to see details
          </div>
        </div>
      );
    }

    const TABS = [
      {id:'general',  label:'General'},
      {id:'trackers', label:`Trackers (${MOCK.trackersList.length})`},
      {id:'peers',    label:`Peers (${MOCK.peers.length})`},
      {id:'http',     label:`HTTP Sources (${MOCK.httpSources.length})`},
      {id:'content',  label:'Content'},
      {id:'speed',    label:'Speed'},
    ];

    return (
      <div style={{flex:1, display:'flex', flexDirection:'column', minHeight:0, background:'var(--bg-0)'}}>
        {/* Tab strip */}
        <div style={{
          display:'flex', height: 36, flexShrink: 0,
          background:'var(--bg-1)',
          borderBottom: '1px solid var(--border-1)',
          padding: '0 8px', alignItems:'center',
        }}>
          {TABS.map(t => (
            <button key={t.id} onClick={()=>setTab(t.id)} style={{
              padding:'0 12px', height: '100%', border:'none', background:'transparent',
              cursor:'pointer', fontSize: 12.5, fontWeight: 500,
              color: tab===t.id ? 'var(--fg-0)' : 'var(--fg-2)',
              borderBottom: tab===t.id ? '2px solid var(--accent)' : '2px solid transparent',
              marginBottom: -1,
            }}>{t.label}</button>
          ))}
          <div style={{flex:1}}/>
          {onClose ? <IT.IconBtn icon={Icon.x({size:14})} onClick={onClose} title="Close inspector"/> : null}
        </div>

        <div style={{flex:1, overflowY:'auto', padding: 14}}>
          {tab === 'general' && (
            <div>
              <Card title="Transfer">
                <Row label="Name"        value={torrent.name}/>
                <Row label="Status"      value={<span style={{display:'inline-flex', alignItems:'center', gap: 6, textTransform:'capitalize', whiteSpace:'nowrap'}}><IT.StatusDot tone={torrent.status}/>{torrent.status}</span>}/>
                <Row label="Progress"    value={<div style={{display:'flex', alignItems:'center', gap: 8}}><IT.ProgressBar value={torrent.progress} tone={torrent.status}/><span className="mono" style={{fontSize:12}}>{(torrent.progress*100).toFixed(2)}%</span></div>}/>
                <Row label="Size"        value={torrent.size} mono/>
                <Row label="Download speed" value={torrent.dl} mono/>
                <Row label="Upload speed"   value={torrent.ul} mono/>
                <Row label="Ratio"       value={torrent.ratio.toFixed(3)} mono/>
                <Row label="ETA"         value={torrent.eta} mono/>
                <Row label="Availability"value={`${torrent.availability.toFixed(2)}×`} mono/>
                <Row label="Pieces"      value={`${torrent.pieces}  ·  ${torrent.pieceSize}/piece`} mono/>
              </Card>

              <Card title="Information">
                <Row label="Info hash v1"  value={torrent.hash} mono/>
                <Row label="Info hash v2"  value="—" mono/>
                <Row label="Save path"     value={torrent.savePath} mono/>
                <Row label="Category"      value={<IT.Chip>{torrent.category}</IT.Chip>}/>
                <Row label="Tags"          value={<span style={{display:'inline-flex', gap: 4}}>{torrent.tags.map(t=><IT.Chip key={t}>{t}</IT.Chip>)}</span>}/>
                <Row label="Added on"      value={torrent.added} mono/>
                <Row label="Completed on"  value={torrent.completed || '—'} mono/>
                <Row label="Private"       value="No"/>
                <Row label="Created by"    value="libtorrent 2.0.9"/>
                <Row label="Comment"       value="Official release. See https://example.org"/>
              </Card>

              {torrent.errorMsg ? (
                <Card title="Errors">
                  <div style={{color:'var(--st-error)', fontSize: 12.5, display:'flex', gap: 8, alignItems:'flex-start'}}>
                    <span style={{marginTop: 2}}>{Icon.warn({size:14})}</span>
                    <span>{torrent.errorMsg}</span>
                  </div>
                </Card>
              ) : null}

              {/* Piece map — 32x12 grid */}
              <Card title="Piece availability">
                <div style={{display:'grid', gridTemplateColumns:'repeat(48, 1fr)', gap: 1}}>
                  {Array.from({length: 48*8}).map((_, i) => {
                    const seed = Math.sin(i * 12.9898 + torrent.id.charCodeAt(1)) * 43758.5453;
                    const r = seed - Math.floor(seed);
                    const done = r < torrent.progress;
                    return <div key={i} style={{
                      aspectRatio: 1,
                      background: done ? 'var(--accent)' : 'var(--bg-inset)',
                      opacity: done ? (0.5 + r*0.5) : 1,
                      borderRadius: 1,
                    }}/>;
                  })}
                </div>
                <div style={{fontSize: 11, color:'var(--fg-3)', marginTop: 8}}>
                  Dark: missing · Light: downloaded · Brightness indicates peer availability
                </div>
              </Card>
            </div>
          )}

          {tab === 'trackers' && (
            <Card title={`Trackers (${MOCK.trackersList.length})`} right={
              <div style={{display:'flex', gap: 4}}>
                <IT.Btn variant="solid" size="sm" icon={Icon.add({size:12})}>Add</IT.Btn>
                <IT.Btn variant="solid" size="sm" icon={Icon.refresh({size:12})}>Reannounce</IT.Btn>
              </div>
            }>
              <div style={{fontSize:12}}>
                <div style={{display:'flex', padding:'6px 8px', color:'var(--fg-2)', borderBottom:'1px solid var(--border-1)', fontSize: 11, textTransform:'uppercase'}}>
                  <span style={{flex:'1 0 240px'}}>URL</span>
                  <span style={{width: 90}}>Status</span>
                  <span style={{width: 60, textAlign:'right'}}>Seeds</span>
                  <span style={{width: 60, textAlign:'right'}}>Peers</span>
                  <span style={{width: 60, textAlign:'right'}}>Leech</span>
                  <span style={{width: 60, textAlign:'right'}}>DL</span>
                  <span style={{flex:'1 0 180px', paddingLeft: 12}}>Message</span>
                </div>
                {MOCK.trackersList.map((tr,i) => (
                  <div key={i} style={{display:'flex', padding:'7px 8px', borderBottom:'1px solid var(--divider)', alignItems:'center'}}>
                    <span className="mono" style={{flex:'1 0 240px', fontSize: 11.5, whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{tr.url}</span>
                    <span style={{width: 90, display:'flex', alignItems:'center', gap: 6}}>
                      <IT.StatusDot tone={tr.status==='working'?'seeding':tr.status==='unreachable'?'stalled':'error'}/>
                      <span style={{textTransform:'capitalize'}}>{tr.status}</span>
                    </span>
                    <span className="mono" style={{width: 60, textAlign:'right', color:'var(--fg-1)'}}>{tr.seeds}</span>
                    <span className="mono" style={{width: 60, textAlign:'right', color:'var(--fg-1)'}}>{tr.peers}</span>
                    <span className="mono" style={{width: 60, textAlign:'right', color:'var(--fg-1)'}}>{tr.leech}</span>
                    <span className="mono" style={{width: 60, textAlign:'right', color:'var(--fg-2)'}}>{tr.downloaded}</span>
                    <span style={{flex:'1 0 180px', paddingLeft: 12, color:'var(--fg-2)', fontSize: 11.5}}>{tr.msg}</span>
                  </div>
                ))}
              </div>
            </Card>
          )}

          {tab === 'peers' && (
            <Card title={`Connected peers (${MOCK.peers.length} of 128 max)`}>
              <div style={{fontSize:12}}>
                <div style={{display:'flex', padding:'6px 8px', color:'var(--fg-2)', borderBottom:'1px solid var(--border-1)', fontSize: 11, textTransform:'uppercase'}}>
                  <span style={{flex:'0 0 180px'}}>IP:Port</span>
                  <span style={{width: 40}}>C</span>
                  <span style={{width: 80}}>Flags</span>
                  <span style={{flex:'1 0 160px'}}>Client</span>
                  <span style={{width: 100}}>Progress</span>
                  <span style={{width: 84, textAlign:'right'}}>Down</span>
                  <span style={{width: 84, textAlign:'right'}}>Up</span>
                  <span style={{width: 76, textAlign:'right'}}>Relev.</span>
                </div>
                {MOCK.peers.map((p,i) => (
                  <div key={i} style={{display:'flex', padding:'7px 8px', borderBottom:'1px solid var(--divider)', alignItems:'center'}}>
                    <span className="mono" style={{flex:'0 0 180px', fontSize: 11.5}}>{p.ip}:{p.port}</span>
                    <span style={{width: 40, color:'var(--fg-2)'}}>{p.conn}</span>
                    <span className="mono" style={{width: 80, color:'var(--fg-2)', fontSize: 11.5}}>{p.flags}</span>
                    <span style={{flex:'1 0 160px', color:'var(--fg-1)'}}>{p.client}</span>
                    <span style={{width: 100}}><IT.ProgressBar value={p.progress} tone="downloading"/></span>
                    <span className="mono" style={{width: 84, textAlign:'right', color: p.dl!=='0 B/s' ? 'var(--st-downloading)' : 'var(--fg-2)'}}>{p.dl}</span>
                    <span className="mono" style={{width: 84, textAlign:'right', color: p.ul!=='0 B/s' ? 'var(--st-seeding)' : 'var(--fg-2)'}}>{p.ul}</span>
                    <span className="mono" style={{width: 76, textAlign:'right', color:'var(--fg-2)'}}>{p.rel}</span>
                  </div>
                ))}
              </div>
              <div style={{fontSize: 11, color:'var(--fg-3)', marginTop: 10, lineHeight: 1.6}}>
                <b style={{color:'var(--fg-2)'}}>Flag key:</b> D/d — downloading from peer (we/they interested) · U/u — uploading · K — peer unchoked us but we're not interested · I — incoming connection · E — encrypted · P — µTP · X — via PEX · H — via DHT
              </div>
            </Card>
          )}

          {tab === 'http' && (
            <Card title={`HTTP seed sources (${MOCK.httpSources.length})`} right={<IT.Btn variant="solid" size="sm" icon={Icon.add({size:12})}>Add URL</IT.Btn>}>
              {MOCK.httpSources.map((h,i) => (
                <div key={i} style={{display:'flex', alignItems:'center', gap: 10, padding:'8px 0', borderBottom:'1px solid var(--divider)'}}>
                  <IT.StatusDot tone="seeding"/>
                  <span className="mono" style={{flex:1, fontSize: 12}}>{h.url}</span>
                  <IT.Chip tone="complete">{h.status}</IT.Chip>
                  <IT.IconBtn icon={Icon.x({size:12})}/>
                </div>
              ))}
            </Card>
          )}

          {tab === 'content' && (
            <Card title="Files" right={
              <div style={{display:'flex', gap: 4, alignItems:'center'}}>
                <IT.Btn variant="ghost" size="sm" icon={Icon.up({size:12})}>Priority up</IT.Btn>
                <IT.Btn variant="ghost" size="sm" icon={Icon.down({size:12})}>Priority down</IT.Btn>
                <IT.Btn variant="solid" size="sm">Sequential</IT.Btn>
                <IT.Btn variant="solid" size="sm">First/last pieces first</IT.Btn>
              </div>
            }>
              <div style={{fontSize:11, color:'var(--fg-2)', display:'flex', padding:'6px 8px', borderBottom:'1px solid var(--border-1)', textTransform:'uppercase'}}>
                <span style={{width: 14}}></span>
                <span style={{width: 16}}></span>
                <span style={{flex:'1 0 200px'}}>Name</span>
                <span style={{width: 80, textAlign:'right'}}>Size</span>
                <span style={{width: 140, paddingLeft: 12}}>Progress</span>
                <span style={{width: 90, textAlign:'right'}}>Priority</span>
              </div>
              <FileTree nodes={MOCK.fileTree}/>
            </Card>
          )}

          {tab === 'speed' && (
            <>
              <Card title="Transfer speed · last 60 seconds" right={
                <div style={{display:'flex', gap: 4}}>
                  <IT.Btn variant="ghost" size="sm" active>1m</IT.Btn>
                  <IT.Btn variant="ghost" size="sm">5m</IT.Btn>
                  <IT.Btn variant="ghost" size="sm">1h</IT.Btn>
                  <IT.Btn variant="ghost" size="sm">1d</IT.Btn>
                </div>
              }>
                <SpeedGraph data={MOCK.speedGraph}/>
              </Card>
              <Card title="Speed limits">
                <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 14}}>
                  <div>
                    <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Download limit</div>
                    <div style={{display:'flex', gap: 8, alignItems:'center'}}>
                      <IT.TextInput mono value="∞" width={100} right suffix="KB/s"/>
                      <IT.Toggle on={false} label="Override global"/>
                    </div>
                  </div>
                  <div>
                    <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Upload limit</div>
                    <div style={{display:'flex', gap: 8, alignItems:'center'}}>
                      <IT.TextInput mono value="5000" width={100} right suffix="KB/s"/>
                      <IT.Toggle on={true} label="Override global"/>
                    </div>
                  </div>
                </div>
              </Card>
            </>
          )}
        </div>
      </div>
    );
  }

  window.DetailsPane = DetailsPane;
})();
