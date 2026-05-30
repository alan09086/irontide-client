// IronTide — left sidebar with nav + filters + categories + tags + trackers.

(() => {
  const { Icon, IT } = window;

  function SidebarItem({ icon, label, count, active, indent = 0, onClick, tone }) {
    return (
      <div onClick={onClick} style={{
        display:'flex', alignItems:'center', gap: 8,
        height: 28, padding: `0 10px 0 ${10 + indent*12}px`,
        borderRadius: 'var(--r-md)',
        cursor: 'pointer', margin: '0 6px',
        background: active ? 'var(--bg-selected)' : 'transparent',
        color: active ? 'var(--fg-0)' : 'var(--fg-1)',
        fontWeight: active ? 500 : 400,
        fontSize: 13,
        transition: 'background var(--dur-fast)',
      }}
      onMouseOver={e=>{ if(!active) e.currentTarget.style.background='var(--bg-hover)'; }}
      onMouseOut={e=>{ if(!active) e.currentTarget.style.background='transparent'; }}
      >
        {tone ? <span style={{width:8, height:8, borderRadius:4, background: tone, flexShrink:0}}/> : null}
        {icon ? <span style={{display:'inline-flex', color: 'var(--fg-1)'}}>{icon}</span> : null}
        <span style={{flex:1, whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{label}</span>
        {count != null ? (
          <span className="num" style={{
            fontSize: 11, color:'var(--fg-3)',
            minWidth: 20, textAlign:'right',
          }}>{count}</span>
        ) : null}
      </div>
    );
  }

  function Sidebar({ nav, setNav, filter, setFilter, category, setCategory, collapsed, labelMode='full' }) {
    const { categories, tags, trackers } = MOCK;

    if (labelMode === 'icons') {
      const items = [
        {id:'torrents', icon: Icon.list({size:18})},
        {id:'search',   icon: Icon.search({size:18})},
        {id:'rss',      icon: Icon.rss({size:18})},
        {id:'scheduler',icon: Icon.scheduler({size:18})},
        {id:'stats',    icon: Icon.stats({size:18})},
        {id:'logs',     icon: Icon.logs({size:18})},
        {id:'ipfilter', icon: Icon.ipfilter({size:18})},
        {id:'create',   icon: Icon.torrentCreate({size:18})},
        {id:'webui',    icon: Icon.webui({size:18})},
      ];
      return (
        <div style={{
          width: 52, flexShrink:0, background:'var(--bg-1)',
          borderRight: '1px solid var(--border-1)',
          display:'flex', flexDirection:'column', alignItems:'center',
          padding: '10px 0', gap: 4,
        }}>
          {items.map(it => (
            <button key={it.id} onClick={()=>setNav(it.id)}
              title={it.id}
              style={{
                width: 36, height: 36, border:'none', cursor:'pointer',
                borderRadius:'var(--r-md)',
                background: nav===it.id ? 'var(--bg-selected)' : 'transparent',
                color: nav===it.id ? 'var(--accent)' : 'var(--fg-1)',
                display:'inline-flex', alignItems:'center', justifyContent:'center',
              }}
              onMouseOver={e=>{ if(nav!==it.id) e.currentTarget.style.background='var(--bg-hover)'; }}
              onMouseOut={e=>{ if(nav!==it.id) e.currentTarget.style.background='transparent'; }}
            >
              {it.icon}
            </button>
          ))}
        </div>
      );
    }

    if (labelMode === 'hidden') {
      return <div style={{width: 0, flexShrink:0}}/>;
    }

    return (
      <div style={{
        width: 'var(--sidebar-w)', flexShrink:0,
        background: 'var(--bg-1)',
        borderRight: '1px solid var(--border-1)',
        display:'flex', flexDirection:'column',
        overflow: 'hidden',
      }}>
        <div style={{overflowY:'auto', flex:1, paddingBottom: 12}}>
          <IT.SectionLabel>Library</IT.SectionLabel>
          <SidebarItem icon={Icon.list({size:14})}     label="All torrents" count={12}
            active={nav==='torrents' && filter==='all'} onClick={()=>{setNav('torrents'); setFilter('all');}}/>
          <SidebarItem icon={Icon.download({size:14})} label="Downloading"  count={3}
            active={nav==='torrents' && filter==='downloading'} onClick={()=>{setNav('torrents'); setFilter('downloading');}}
            tone="var(--st-downloading)"/>
          <SidebarItem icon={Icon.upload({size:14})}   label="Seeding"      count={4}
            active={nav==='torrents' && filter==='seeding'} onClick={()=>{setNav('torrents'); setFilter('seeding');}}
            tone="var(--st-seeding)"/>
          <SidebarItem icon={Icon.check({size:14})}    label="Completed"    count={5}
            active={nav==='torrents' && filter==='completed'} onClick={()=>{setNav('torrents'); setFilter('completed');}}
            tone="var(--st-complete)"/>
          <SidebarItem icon={Icon.pause({size:14})}    label="Paused"       count={1}
            active={nav==='torrents' && filter==='paused'} onClick={()=>{setNav('torrents'); setFilter('paused');}}
            tone="var(--st-paused)"/>
          <SidebarItem icon={Icon.bolt({size:14})}     label="Active"       count={3}
            active={nav==='torrents' && filter==='active'} onClick={()=>{setNav('torrents'); setFilter('active');}}/>
          <SidebarItem icon={Icon.stop({size:14})}     label="Inactive"     count={9}
            active={nav==='torrents' && filter==='inactive'} onClick={()=>{setNav('torrents'); setFilter('inactive');}}/>
          <SidebarItem icon={Icon.warn({size:14})}     label="Errored"      count={1}
            active={nav==='torrents' && filter==='error'} onClick={()=>{setNav('torrents'); setFilter('error');}}
            tone="var(--st-error)"/>

          <IT.SectionLabel right={<button style={{background:'none', border:'none', color:'var(--fg-3)', cursor:'pointer', padding:0}}>{Icon.add({size:12})}</button>}>
            Categories
          </IT.SectionLabel>
          {categories.map(c => (
            <SidebarItem key={c.id} icon={Icon.folder({size:14})} label={c.name} count={c.count}
              active={nav==='torrents' && category===c.id}
              onClick={()=>{setNav('torrents'); setCategory(c.id);}}/>
          ))}

          <IT.SectionLabel right={<button style={{background:'none', border:'none', color:'var(--fg-3)', cursor:'pointer', padding:0}}>{Icon.add({size:12})}</button>}>
            Tags
          </IT.SectionLabel>
          {tags.slice(0,6).map(t => (
            <SidebarItem key={t.id} icon={Icon.tag({size:14})} label={t.id} count={t.count}/>
          ))}

          <IT.SectionLabel>Trackers</IT.SectionLabel>
          {trackers.slice(0,4).map(t => (
            <SidebarItem key={t.id} icon={Icon.tracker({size:14})} label={t.name || t.id} count={t.count}/>
          ))}

          <IT.SectionLabel>Tools</IT.SectionLabel>
          <SidebarItem icon={Icon.search({size:14})}    label="Search"         active={nav==='search'}    onClick={()=>setNav('search')}/>
          <SidebarItem icon={Icon.rss({size:14})}       label="RSS"            active={nav==='rss'}       onClick={()=>setNav('rss')}/>
          <SidebarItem icon={Icon.scheduler({size:14})} label="Scheduler"      active={nav==='scheduler'} onClick={()=>setNav('scheduler')}/>
          <SidebarItem icon={Icon.stats({size:14})}     label="Statistics"     active={nav==='stats'}     onClick={()=>setNav('stats')}/>
          <SidebarItem icon={Icon.logs({size:14})}      label="Logs"           active={nav==='logs'}      onClick={()=>setNav('logs')}/>
          <SidebarItem icon={Icon.ipfilter({size:14})}  label="IP Filter"      active={nav==='ipfilter'}  onClick={()=>setNav('ipfilter')}/>
          <SidebarItem icon={Icon.torrentCreate({size:14})} label="Create torrent" active={nav==='create'} onClick={()=>setNav('create')}/>
          <SidebarItem icon={Icon.webui({size:14})}     label="Web UI"         active={nav==='webui'}     onClick={()=>setNav('webui')}/>
        </div>

        <div style={{borderTop:'1px solid var(--border-1)', padding: 10, fontSize: 11}}>
          <div style={{display:'flex', justifyContent:'space-between', color:'var(--fg-2)', marginBottom: 4}}>
            <span>Disk</span><span className="mono">1.2 TB free</span>
          </div>
          <div style={{height: 4, background:'var(--bg-inset)', borderRadius: 2, overflow:'hidden'}}>
            <div style={{width: '68%', height:'100%', background:'var(--accent)'}}/>
          </div>
          <div style={{display:'flex', justifyContent:'space-between', color:'var(--fg-3)', marginTop: 4}}>
            <span>of 3.6 TB</span><span>68%</span>
          </div>
        </div>
      </div>
    );
  }

  window.Sidebar = Sidebar;
})();
