// IronTide — tool pages: RSS, Search, Scheduler, IP Filter, Logs, Statistics, Web UI settings.
(() => {
  const { Icon, IT } = window;

  function PageShell({ title, subtitle, actions, children }) {
    return (
      <div style={{flex:1, display:'flex', flexDirection:'column', minHeight:0, background:'var(--bg-0)'}}>
        <div style={{
          padding:'14px 18px', borderBottom:'1px solid var(--border-1)',
          display:'flex', alignItems:'center', gap: 12,
        }}>
          <div style={{flex:1}}>
            <div style={{fontSize: 15, fontWeight: 600}}>{title}</div>
            {subtitle ? <div style={{fontSize: 12, color:'var(--fg-2)', marginTop: 2}}>{subtitle}</div> : null}
          </div>
          {actions}
        </div>
        <div style={{flex:1, overflowY:'auto', padding: 18}}>{children}</div>
      </div>
    );
  }

  // ------------ RSS ------------
  function RSSPage() {
    const [feed, setFeed] = React.useState('Linux ISOs');
    return (
      <div style={{flex:1, display:'flex', minHeight:0}}>
        {/* Feeds list */}
        <div style={{width: 260, flexShrink:0, borderRight:'1px solid var(--border-1)', background:'var(--bg-1)', display:'flex', flexDirection:'column'}}>
          <div style={{padding:'10px 12px', borderBottom:'1px solid var(--divider)', display:'flex', gap: 6, alignItems:'center'}}>
            <span style={{fontSize: 11, color:'var(--fg-2)', flex:1, textTransform:'uppercase', fontWeight: 600, letterSpacing:'.06em'}}>Feeds</span>
            <IT.IconBtn icon={Icon.add({size:13})} title="New feed"/>
            <IT.IconBtn icon={Icon.refresh({size:13})} title="Refresh all"/>
          </div>
          <div style={{flex:1, overflowY:'auto'}}>
            {MOCK.rssFeeds.map(f => (
              <div key={f.id} onClick={()=>setFeed(f.name)} style={{
                padding:'10px 12px', borderBottom:'1px solid var(--divider)',
                background: feed===f.name ? 'var(--bg-selected)' : 'transparent',
                cursor:'pointer',
              }}>
                <div style={{display:'flex', alignItems:'center', gap: 8}}>
                  <span style={{color:'var(--fg-2)'}}>{Icon.rss({size:13})}</span>
                  <span style={{flex:1, fontSize: 13, fontWeight: 500}}>{f.name}</span>
                  {f.unread ? <IT.Chip>{f.unread}</IT.Chip> : null}
                </div>
                <div className="mono" style={{fontSize: 10.5, color:'var(--fg-3)', marginTop: 4, whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{f.url}</div>
                <div style={{fontSize: 10.5, color:'var(--fg-3)', marginTop: 2}}>Updated {f.last}</div>
              </div>
            ))}
          </div>
        </div>

        {/* Items list + rules */}
        <div style={{flex:1, display:'flex', flexDirection:'column', minHeight:0}}>
          <div style={{padding:'10px 14px', borderBottom:'1px solid var(--divider)', display:'flex', alignItems:'center', gap: 8}}>
            <span style={{fontSize: 13, fontWeight: 600, flex:1}}>{feed}</span>
            <IT.Btn variant="solid" size="sm" icon={Icon.refresh({size:12})}>Refresh</IT.Btn>
            <IT.Btn variant="solid" size="sm">Mark all read</IT.Btn>
          </div>

          <div style={{flex:1, overflowY:'auto'}}>
            {MOCK.rssItems.filter(it => feed==='Linux ISOs' ? it.feed==='Linux ISOs' : true).map(it => (
              <div key={it.id} style={{
                padding:'10px 14px', borderBottom:'1px solid var(--divider)',
                display:'flex', alignItems:'center', gap: 10,
              }}>
                <span style={{width: 6, height: 6, borderRadius: 3, background: it.matched ? 'var(--accent)' : 'var(--border-2)', flexShrink: 0}}/>
                <span className="mono" style={{flex:1, minWidth: 0, fontSize: 12.5, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>{it.title}</span>
                {it.matched ? <span style={{flexShrink: 0}}><IT.Chip tone="complete">matched: {it.matched}</IT.Chip></span> : null}
                <span className="mono" style={{fontSize: 11, color:'var(--fg-2)', width: 70, flexShrink: 0, textAlign:'right'}}>{it.size}</span>
                <span className="mono" style={{fontSize: 11, color:'var(--fg-3)', width: 120, flexShrink: 0, textAlign:'right'}}>{it.date}</span>
                <span style={{flexShrink: 0}}><IT.Btn variant="solid" size="sm" icon={Icon.download({size:12})}>Download</IT.Btn></span>
              </div>
            ))}
          </div>

          <div style={{borderTop:'1px solid var(--border-1)', background:'var(--bg-1)', padding: 12}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 8}}>Auto-download rules</div>
            <div style={{display:'flex', gap: 8, flexWrap:'wrap'}}>
              {MOCK.rssRules.map(r => (
                <div key={r.id} style={{
                  padding:'8px 12px', background:'var(--bg-2)', border:'1px solid var(--border-1)',
                  borderRadius:'var(--r-md)', minWidth: 260, flex:'1 1 260px',
                }}>
                  <div style={{display:'flex', alignItems:'center', gap: 8, marginBottom: 4}}>
                    <IT.Toggle on={r.enabled}/>
                    <span style={{fontSize: 13, fontWeight: 500, flex:1}}>{r.name}</span>
                    <IT.IconBtn icon={Icon.gear({size:13})}/>
                  </div>
                  <div className="mono" style={{fontSize: 11, color:'var(--fg-2)'}}>match: {r.mustContain}</div>
                  {r.mustNotContain ? <div className="mono" style={{fontSize: 11, color:'var(--fg-3)'}}>exclude: {r.mustNotContain}</div> : null}
                  <div style={{fontSize: 11, color:'var(--fg-3)', marginTop: 4}}>→ {r.category} @ <span className="mono">{r.savePath}</span></div>
                </div>
              ))}
              <div style={{padding:'8px 12px', border:'1.5px dashed var(--border-2)', borderRadius:'var(--r-md)', color:'var(--fg-2)', display:'flex', alignItems:'center', gap: 6, cursor:'pointer', minWidth: 200}}>
                {Icon.add({size:14})}<span>New rule…</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // ------------ Search ------------
  function SearchPage() {
    return (
      <PageShell title="Search" subtitle="Query enabled plugins across trackers and indexers">
        <div style={{display:'flex', gap: 8, marginBottom: 14}}>
          <IT.TextInput mono value="ubuntu 24.04" width={420}/>
          <IT.Select value="All plugins (enabled)" options={['All plugins (enabled)','LinuxTracker','Archive.org','AcademicTorrents']} width={200}/>
          <IT.Select value="All categories" options={['All categories','Software','Movies','TV','Music','Books','Games','Anime','Other']} width={180}/>
          <IT.Btn variant="primary" icon={Icon.search({size:13})}>Search</IT.Btn>
          <IT.Btn variant="solid">Stop</IT.Btn>
        </div>

        <div style={{display:'flex', gap: 12}}>
          <div style={{flex:1, background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', overflow:'hidden'}}>
            <div style={{display:'flex', gap: 10, padding:'8px 12px', fontSize: 11, textTransform:'uppercase', color:'var(--fg-2)', borderBottom:'1px solid var(--border-1)', fontWeight: 600, letterSpacing:'.04em'}}>
              <span style={{flex:'1 1 0', minWidth: 100}}>Name</span>
              <span style={{width: 80, flexShrink: 0, textAlign:'right'}}>Size</span>
              <span style={{width: 56, flexShrink: 0, textAlign:'right'}}>Seeds</span>
              <span style={{width: 56, flexShrink: 0, textAlign:'right'}}>Peers</span>
              <span style={{width: 120, flexShrink: 0}}>Engine</span>
              <span style={{width: 92, flexShrink: 0, textAlign:'right'}}>Date</span>
              <span style={{width: 96, flexShrink: 0}}></span>
            </div>
            {MOCK.searchResults.map((r,i) => (
              <div key={i} style={{display:'flex', gap: 10, padding:'10px 12px', borderBottom:'1px solid var(--divider)', alignItems:'center', fontSize: 12.5}}>
                <span style={{flex:'1 1 0', minWidth: 100, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>{r.name}</span>
                <span className="mono" style={{width: 80, flexShrink: 0, textAlign:'right', whiteSpace:'nowrap'}}>{r.size}</span>
                <span className="mono" style={{width: 56, flexShrink: 0, textAlign:'right', color:'var(--st-seeding)'}}>{r.seeds}</span>
                <span className="mono" style={{width: 56, flexShrink: 0, textAlign:'right', color:'var(--fg-2)'}}>{r.peers}</span>
                <span style={{width: 120, flexShrink: 0, color:'var(--fg-2)', overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>{r.engine}</span>
                <span className="mono" style={{width: 92, flexShrink: 0, textAlign:'right', color:'var(--fg-3)', fontSize: 11, whiteSpace:'nowrap'}}>{r.date}</span>
                <div style={{width: 96, flexShrink: 0, display:'flex', gap: 4, justifyContent:'flex-end'}}>
                  <IT.Btn variant="solid" size="sm" icon={Icon.download({size:11})}>Add</IT.Btn>
                  <IT.IconBtn icon={Icon.link({size:12})} title="Open in browser"/>
                </div>
              </div>
            ))}
          </div>

          <div style={{width: 280, flexShrink: 0, background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 12}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 8}}>Search plugins</div>
            {MOCK.searchEngines.map(e => (
              <div key={e.id} style={{display:'flex', alignItems:'center', gap: 8, padding:'6px 0', borderBottom:'1px solid var(--divider)'}}>
                <IT.Toggle on={e.enabled}/>
                <div style={{flex:1}}>
                  <div style={{fontSize: 12.5}}>{e.name}</div>
                  <div className="mono" style={{fontSize: 10.5, color:'var(--fg-3)'}}>{e.url} · v{e.version}</div>
                </div>
              </div>
            ))}
            <div style={{display:'flex', gap: 6, marginTop: 10}}>
              <IT.Btn variant="solid" size="sm" icon={Icon.add({size:12})}>Install…</IT.Btn>
              <IT.Btn variant="solid" size="sm" icon={Icon.refresh({size:12})}>Update all</IT.Btn>
            </div>
          </div>
        </div>
      </PageShell>
    );
  }

  // ------------ Scheduler ------------
  function SchedulerPage() {
    const hours = 24, days = ['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];
    const active = (d, h) => (h >= 22 || h < 7) ? 'alt' : ((d>=5 && h>=10 && h<18) ? 'off' : 'normal');
    return (
      <PageShell title="Bandwidth Scheduler" subtitle="Auto-apply alternative speed limits (or pause) by time of day">
        <div style={{display:'flex', gap: 16, marginBottom: 18, alignItems:'center'}}>
          <IT.Toggle on label="Enable scheduler"/>
          <div style={{display:'flex', gap: 12, fontSize: 12}}>
            {[['normal','var(--accent)','Full speed'],['alt','var(--st-queued)','Alternative limits'],['off','var(--st-paused)','Paused']].map(([k,c,l]) => (
              <span key={k} style={{display:'inline-flex', alignItems:'center', gap: 6}}>
                <span style={{width: 10, height: 10, borderRadius: 2, background: c}}/>{l}
              </span>
            ))}
          </div>
          <div style={{flex:1}}/>
          <IT.Btn variant="solid" size="sm">Clear all</IT.Btn>
          <IT.Btn variant="solid" size="sm">Preset: work hours</IT.Btn>
        </div>

        <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 12}}>
          <div style={{display:'grid', gridTemplateColumns: `40px repeat(${hours}, 1fr)`, gap: 2, fontSize: 10, color:'var(--fg-3)'}}>
            <span></span>
            {Array.from({length: hours}).map((_, h) => (
              <span key={h} className="mono" style={{textAlign:'center'}}>{h%3===0 ? h.toString().padStart(2,'0') : ''}</span>
            ))}
          </div>
          {days.map((d, di) => (
            <div key={d} style={{display:'grid', gridTemplateColumns: `40px repeat(${hours}, 1fr)`, gap: 2, marginTop: 2}}>
              <span className="mono" style={{fontSize: 11, color:'var(--fg-2)', textAlign:'right', paddingRight: 6}}>{d}</span>
              {Array.from({length: hours}).map((_, h) => {
                const a = active(di, h);
                const color = a==='alt' ? 'var(--st-queued)' : a==='off' ? 'var(--st-paused)' : 'var(--accent)';
                return <div key={h} style={{height: 22, background: color, opacity: a==='normal' ? 0.55 : 0.85, borderRadius: 2, cursor:'pointer'}}/>;
              })}
            </div>
          ))}
          <div style={{fontSize: 11, color:'var(--fg-3)', marginTop: 10}}>Click cells to toggle state · Drag to paint multiple · Right-click for presets</div>
        </div>

        <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 14, marginTop: 14}}>
          <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 8}}>Alternative limits</div>
            <div style={{display:'flex', gap: 10, alignItems:'center', marginBottom: 8}}>
              <span style={{width: 100, fontSize: 12, color:'var(--fg-1)'}}>Download</span>
              <IT.TextInput mono value="500" width={100} right suffix="KB/s"/>
            </div>
            <div style={{display:'flex', gap: 10, alignItems:'center'}}>
              <span style={{width: 100, fontSize: 12, color:'var(--fg-1)'}}>Upload</span>
              <IT.TextInput mono value="100" width={100} right suffix="KB/s"/>
            </div>
          </div>
          <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 8}}>Currently active</div>
            <div style={{display:'flex', alignItems:'center', gap: 8}}>
              <IT.StatusDot tone="queued"/>
              <span style={{fontSize: 13}}>Alternative limits — until 07:00 tomorrow</span>
            </div>
          </div>
        </div>
      </PageShell>
    );
  }

  // ------------ IP Filter ------------
  function IPFilterPage() {
    const ranges = [
      { range: '0.0.0.0/8',         level: 'block',  comment: 'Reserved (RFC 1700)' },
      { range: '10.0.0.0/8',        level: 'allow',  comment: 'Private network (LAN)' },
      { range: '45.133.0.0/20',     level: 'block',  comment: 'Known abuser AS212238' },
      { range: '92.118.160.0/24',   level: 'block',  comment: 'Anti-piracy scanner' },
      { range: '193.188.0.0/16',    level: 'block',  comment: 'Anti-piracy scanner' },
      { range: '203.0.113.0/24',    level: 'block',  comment: 'TEST-NET-3 (RFC 5737)' },
      { range: '240.0.0.0/4',       level: 'block',  comment: 'Reserved future use' },
    ];
    return (
      <PageShell
        title="IP Filter"
        subtitle="Block or allow connections from specific IP ranges"
        actions={<>
          <IT.Btn variant="solid" size="sm" icon={Icon.refresh({size:12})}>Reload</IT.Btn>
          <IT.Btn variant="solid" size="sm" icon={Icon.link({size:12})}>Import list…</IT.Btn>
          <IT.Btn variant="primary" size="sm" icon={Icon.add({size:12})}>Add range</IT.Btn>
        </>}
      >
        <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 1fr 1fr', gap: 10, marginBottom: 14}}>
          {[
            ['Filter','Enabled','var(--accent)'],
            ['Total ranges','4,218','var(--fg-1)'],
            ['Blocked today','142 IPs','var(--st-error)'],
            ['Last refresh','2m ago','var(--fg-1)'],
          ].map(([label, value, color]) => (
            <div key={label} style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 12}}>
              <div style={{fontSize: 11, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em'}}>{label}</div>
              <div className="mono" style={{fontSize: 18, color, marginTop: 4}}>{value}</div>
            </div>
          ))}
        </div>

        <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', overflow:'hidden'}}>
          <div style={{display:'flex', padding:'8px 12px', borderBottom:'1px solid var(--border-1)', fontSize: 11, textTransform:'uppercase', color:'var(--fg-2)', fontWeight: 600, letterSpacing:'.04em'}}>
            <span style={{width: 260}}>Range</span>
            <span style={{width: 100}}>Level</span>
            <span style={{flex:1}}>Comment</span>
          </div>
          {ranges.map((r,i) => (
            <div key={i} style={{display:'flex', padding:'8px 12px', borderBottom:'1px solid var(--divider)', alignItems:'center', fontSize: 12.5}}>
              <span className="mono" style={{width: 260}}>{r.range}</span>
              <span style={{width: 100}}>
                <IT.Chip tone={r.level==='block'?'error':'complete'}>{r.level}</IT.Chip>
              </span>
              <span style={{flex:1, color:'var(--fg-2)'}}>{r.comment}</span>
            </div>
          ))}
        </div>

        <div style={{marginTop: 14, padding: 12, background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)'}}>
          <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 10}}>Banned peers (session)</div>
          {['45.133.1.22 — reason: IP filter','118.192.44.8 — reason: sent unknown message','51.89.2.41 — reason: too many bad pieces'].map((s,i) => (
            <div key={i} className="mono" style={{fontSize: 12, padding: '4px 0', color:'var(--fg-1)'}}>{s}</div>
          ))}
        </div>
      </PageShell>
    );
  }

  // ------------ Logs ------------
  function LogsPage() {
    const [levels, setLevels] = React.useState({INFO:true, WARN:true, ERROR:true, DEBUG:false});
    const visible = MOCK.logs.filter(l => levels[l.level]);
    return (
      <PageShell
        title="Event log"
        subtitle={`${visible.length} events · tailing`}
        actions={<>
          {['INFO','WARN','ERROR','DEBUG'].map(l => (
            <IT.Btn key={l} variant="solid" size="sm"
              active={levels[l]}
              onClick={()=>setLevels({...levels, [l]: !levels[l]})}>{l}</IT.Btn>
          ))}
          <IT.Btn variant="solid" size="sm" icon={Icon.refresh({size:12})}>Clear</IT.Btn>
          <IT.Btn variant="solid" size="sm" icon={Icon.upload({size:12})}>Export…</IT.Btn>
        </>}
      >
        <div style={{
          background:'var(--bg-inset)', border:'1px solid var(--border-1)',
          borderRadius:'var(--r-md)', padding: 12, fontFamily:'var(--font-mono)', fontSize: 12,
          maxHeight: 'none',
        }}>
          {visible.map((l, i) => (
            <div key={i} style={{display:'flex', gap: 10, padding:'3px 0', borderBottom: '1px dashed var(--divider)'}}>
              <span style={{color:'var(--fg-3)'}}>{l.t}</span>
              <span style={{
                width: 50, flexShrink:0, fontSize: 11, fontWeight: 600,
                color: l.level==='ERROR' ? 'var(--st-error)' : l.level==='WARN' ? 'var(--st-queued)' : l.level==='DEBUG' ? 'var(--fg-3)' : 'var(--st-seeding)',
              }}>{l.level}</span>
              <span style={{flex:1, color:'var(--fg-1)'}}>{l.msg}</span>
            </div>
          ))}
        </div>
      </PageShell>
    );
  }

  // ------------ Statistics ------------
  function StatsPage() {
    const s = MOCK.stats.allTime;
    const cards = [
      ['All-time downloaded', s.downloaded, 'var(--st-downloading)'],
      ['All-time uploaded',   s.uploaded,   'var(--st-seeding)'],
      ['All-time ratio',      s.ratio.toFixed(2), 'var(--accent)'],
      ['Total shared time',   s.sharedTime, 'var(--fg-0)'],
      ['Session download',    s.sessionDl,  'var(--st-downloading)'],
      ['Session upload',      s.sessionUl,  'var(--st-seeding)'],
      ['Session time',        s.sessionUp,  'var(--fg-0)'],
      ['Global peers',        String(s.globalPeers),        'var(--fg-0)'],
      ['DHT nodes',           s.dhtNodes.toLocaleString(),  'var(--fg-0)'],
      ['Active connections', `${s.connectionsActive} / ${s.connectionsMax}`, 'var(--fg-0)'],
    ];
    return (
      <PageShell title="Statistics" subtitle="Aggregate transfer data, session, and network">
        <div style={{display:'grid', gridTemplateColumns:'repeat(5, 1fr)', gap: 10, marginBottom: 14}}>
          {cards.map(([l,v,c]) => (
            <div key={l} style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 12}}>
              <div style={{fontSize: 11, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em'}}>{l}</div>
              <div className="mono" style={{fontSize: 17, color: c, marginTop: 6, fontWeight: 500}}>{v}</div>
            </div>
          ))}
        </div>

        <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14, marginBottom: 14}}>
          <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 8}}>Transfer — 90 day rolling</div>
          <svg width="100%" height="180" viewBox="0 0 800 180" preserveAspectRatio="none">
            <defs>
              <linearGradient id="st-dl" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--st-downloading)" stopOpacity=".3"/>
                <stop offset="100%" stopColor="var(--st-downloading)" stopOpacity="0"/>
              </linearGradient>
              <linearGradient id="st-ul" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--st-seeding)" stopOpacity=".25"/>
                <stop offset="100%" stopColor="var(--st-seeding)" stopOpacity="0"/>
              </linearGradient>
            </defs>
            {[0.25,0.5,0.75].map(f => <line key={f} x1="0" x2="800" y1={180*f} y2={180*f} stroke="var(--divider)" strokeDasharray="2 3"/>)}
            {(() => {
              const pts = Array.from({length: 90}).map((_,i) => {
                const dl = 60 + Math.sin(i*0.4)*40 + (i%7===0?40:0) + Math.random()*20;
                const ul = 40 + Math.cos(i*0.3)*25 + Math.random()*10;
                return { x: (i/89)*800, dl: 180-dl, ul: 180-ul };
              });
              const p = (k) => 'M' + pts.map(pt => `${pt.x},${pt[k]}`).join(' L');
              const a = (k) => p(k) + ` L800,180 L0,180 Z`;
              return <>
                <path d={a('dl')} fill="url(#st-dl)"/>
                <path d={p('dl')} fill="none" stroke="var(--st-downloading)" strokeWidth="1.5"/>
                <path d={a('ul')} fill="url(#st-ul)"/>
                <path d={p('ul')} fill="none" stroke="var(--st-seeding)" strokeWidth="1.5"/>
              </>;
            })()}
          </svg>
        </div>

        <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 14}}>
          <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 10}}>Top peer countries (session)</div>
            {[
              ['United States', 0.32], ['Germany', 0.18], ['France', 0.12],
              ['Japan', 0.09], ['Netherlands', 0.08], ['Canada', 0.06], ['Other', 0.15],
            ].map(([c, v]) => (
              <div key={c} style={{display:'flex', alignItems:'center', gap: 8, margin:'6px 0'}}>
                <span style={{width: 110, fontSize: 12, color:'var(--fg-1)'}}>{c}</span>
                <div style={{flex:1, height: 6, background:'var(--bg-inset)', borderRadius: 3}}>
                  <div style={{width: `${v*100}%`, height: '100%', background:'var(--accent)', borderRadius: 3}}/>
                </div>
                <span className="mono" style={{width: 40, textAlign:'right', fontSize: 11, color:'var(--fg-2)'}}>{(v*100).toFixed(0)}%</span>
              </div>
            ))}
          </div>
          <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 10}}>Cache & I/O</div>
            {[
              ['Read cache hit rate', '94.2%'],
              ['Write cache queue', '0 blocks'],
              ['Queued writes', '142 KiB'],
              ['Outstanding reads', '0'],
              ['Average disk read time', '0.4 ms'],
              ['Average disk write time', '1.2 ms'],
              ['Piece download time (avg)', '18 ms'],
            ].map(([l,v]) => (
              <div key={l} style={{display:'flex', padding:'4px 0', fontSize: 12.5}}>
                <span style={{flex:1, color:'var(--fg-2)'}}>{l}</span>
                <span className="mono" style={{color:'var(--fg-0)'}}>{v}</span>
              </div>
            ))}
          </div>
        </div>
      </PageShell>
    );
  }

  // ------------ Create torrent (page embed) ------------
  function CreatePage() {
    return (
      <PageShell title="Create new torrent" subtitle="Bundle local files for sharing">
        <div style={{maxWidth: 760}}>
          <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Source file or folder</div>
          <div style={{display:'flex', gap: 6, marginBottom: 14}}>
            <IT.TextInput mono value="/mnt/storage/MyRelease" width="100%"/>
            <IT.Btn variant="solid">File</IT.Btn>
            <IT.Btn variant="solid">Folder</IT.Btn>
          </div>
          <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Trackers (one URL per line, blank line for tier separator)</div>
          <textarea defaultValue="udp://tracker.openbittorrent.com:80/announce
udp://tracker.opentrackr.org:1337/announce

udp://open.stealth.si:80/announce"
            style={{width:'100%', height: 100, padding: 10, background:'var(--bg-2)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', color:'var(--fg-0)', fontFamily:'var(--font-mono)', fontSize: 12, resize:'vertical', marginBottom: 14}}/>
          <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 14, marginBottom: 14}}>
            <div>
              <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Piece size</div>
              <IT.Select value="Auto" options={['Auto','64 KiB','256 KiB','1 MiB','4 MiB','16 MiB']} width="100%"/>
            </div>
            <div>
              <div style={{fontSize:11, color:'var(--fg-2)', marginBottom: 4}}>Format</div>
              <IT.Select value="Hybrid (v1 + v2)" options={['v1','v2','Hybrid (v1 + v2)']} width="100%"/>
            </div>
          </div>
          <div style={{display:'flex', gap: 10}}>
            <IT.Btn variant="primary" icon={Icon.torrentCreate({size:13})}>Create & save…</IT.Btn>
            <IT.Btn variant="solid">Reset</IT.Btn>
          </div>
        </div>
      </PageShell>
    );
  }

  // ------------ WebUI settings (lives under tool, mirrors prefs) ------------
  function WebUIPage() {
    return (
      <PageShell
        title="Web UI"
        subtitle="Remote access to IronTide through a browser or mobile client"
        actions={<IT.Btn variant="solid" size="sm" icon={Icon.link({size:12})}>Open in browser</IT.Btn>}
      >
        <div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap: 14}}>
          <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 10}}>Endpoint</div>
            <div style={{display:'grid', gridTemplateColumns:'140px 1fr', gap:'6px 14px', fontSize: 12.5}}>
              <span style={{color:'var(--fg-2)'}}>Local URL</span><span className="mono">http://localhost:8080</span>
              <span style={{color:'var(--fg-2)'}}>LAN URL</span><span className="mono">http://192.168.1.18:8080</span>
              <span style={{color:'var(--fg-2)'}}>External URL</span><span className="mono">https://seed.example.com</span>
              <span style={{color:'var(--fg-2)'}}>Status</span><span><IT.Chip tone="complete">running</IT.Chip></span>
              <span style={{color:'var(--fg-2)'}}>Uptime</span><span className="mono">18h 42m 11s</span>
              <span style={{color:'var(--fg-2)'}}>Active sessions</span><span className="mono">2</span>
            </div>
          </div>
          <div style={{background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14}}>
            <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 10}}>Active sessions</div>
            {[
              {who:'admin', ip:'192.168.1.24', client:'Firefox 124 / macOS', since:'2h 11m'},
              {who:'admin', ip:'10.44.2.8',    client:'iOS Shortcuts',       since:'22m'},
            ].map((s,i) => (
              <div key={i} style={{display:'flex', alignItems:'center', gap: 10, padding:'6px 0', borderBottom:'1px solid var(--divider)', fontSize: 12.5}}>
                <span style={{color:'var(--accent)'}}>{Icon.peer({size:14})}</span>
                <span style={{flex:1}}>
                  <span style={{fontWeight: 500}}>{s.who}</span>
                  <span className="mono" style={{color:'var(--fg-2)', marginLeft: 8}}>{s.ip}</span>
                </span>
                <span style={{color:'var(--fg-2)'}}>{s.client}</span>
                <span className="mono" style={{color:'var(--fg-3)'}}>{s.since}</span>
                <IT.Btn variant="solid" size="sm">Revoke</IT.Btn>
              </div>
            ))}
          </div>
        </div>

        <div style={{marginTop: 14, background:'var(--bg-1)', border:'1px solid var(--border-1)', borderRadius:'var(--r-md)', padding: 14}}>
          <div style={{fontSize: 11, fontWeight: 600, color:'var(--fg-2)', textTransform:'uppercase', letterSpacing:'.06em', marginBottom: 10}}>Pair a device</div>
          <div style={{display:'flex', gap: 20, alignItems:'center'}}>
            <div style={{width: 140, height: 140, background:'var(--fg-0)', borderRadius:'var(--r-md)', display:'grid', gridTemplateColumns:'repeat(14, 1fr)'}}>
              {Array.from({length: 196}).map((_,i) => (
                <div key={i} style={{background: Math.sin(i*7.1)>0 ? 'var(--bg-0)' : 'transparent'}}/>
              ))}
            </div>
            <div>
              <div style={{fontSize: 13, marginBottom: 4}}>Scan with IronTide Mobile</div>
              <div style={{fontSize: 12, color:'var(--fg-2)', marginBottom: 10}}>The QR encodes: URL, user, and a one-time pairing token.</div>
              <div style={{display:'flex', gap: 6}}>
                <IT.Btn variant="solid" size="sm" icon={Icon.refresh({size:12})}>Rotate token</IT.Btn>
                <IT.Btn variant="solid" size="sm">Copy URL</IT.Btn>
                <IT.Btn variant="solid" size="sm">Show password</IT.Btn>
              </div>
            </div>
          </div>
        </div>
      </PageShell>
    );
  }

  window.Tools = { RSSPage, SearchPage, SchedulerPage, IPFilterPage, LogsPage, StatsPage, CreatePage, WebUIPage };
})();
