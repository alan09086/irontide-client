// IronTide — main app shell. Combines all pieces.
(() => {
  const { Icon, IT, Sidebar, TorrentList, DetailsPane, Toolbar, Tools, Modals, PrefsDialog, TweaksPanel, Chrome } = window;

  // Load persisted tweaks or defaults
  const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
    "skin": "tide",
    "theme": "dark",
    "density": "compact",
    "sidebar": "full",
    "layoutVariant": "L1",
    "radius": "rounded",
    "striping": true,
    "platform": "mac",
    "font": "Inter"
  }/*EDITMODE-END*/;

  function App() {
    const [tweaks, setTweaks] = React.useState(() => {
      try {
        const saved = localStorage.getItem('irontide-tweaks');
        return saved ? { ...TWEAK_DEFAULTS, ...JSON.parse(saved) } : TWEAK_DEFAULTS;
      } catch { return TWEAK_DEFAULTS; }
    });
    const [nav, setNav] = React.useState('torrents');
    const [filter, setFilter] = React.useState('all');
    const [category, setCategory] = React.useState('all');
    const [selected, setSelected] = React.useState('t2');
    const [modal, setModal] = React.useState(null);   // 'add' | 'create' | 'cmd' | 'prefs'
    const [tweaksOpen, setTweaksOpen] = React.useState(false);

    // Apply tweaks to root
    React.useEffect(() => {
      document.documentElement.dataset.skin = tweaks.skin;
      document.documentElement.dataset.theme = tweaks.theme;
      document.documentElement.dataset.density = tweaks.density;
      document.documentElement.dataset.radius = tweaks.radius;
      document.documentElement.style.setProperty('--font-ui',
        tweaks.font === 'System' ? '-apple-system, system-ui, sans-serif' :
        tweaks.font === 'Helvetica' ? 'Helvetica, Arial, sans-serif' :
        tweaks.font === 'IBM Plex Sans' ? "'IBM Plex Sans', sans-serif" :
        "'Inter', sans-serif"
      );
      try { localStorage.setItem('irontide-tweaks', JSON.stringify(tweaks)); } catch {}
    }, [tweaks]);

    // Tweaks protocol
    React.useEffect(() => {
      const onMsg = (e) => {
        if (e.data && e.data.type === '__activate_edit_mode') setTweaksOpen(true);
        if (e.data && e.data.type === '__deactivate_edit_mode') setTweaksOpen(false);
      };
      window.addEventListener('message', onMsg);
      window.parent.postMessage({type:'__edit_mode_available'}, '*');
      return () => window.removeEventListener('message', onMsg);
    }, []);

    // Update edit-mode keys whenever tweaks change
    const firstRender = React.useRef(true);
    React.useEffect(() => {
      if (firstRender.current) { firstRender.current = false; return; }
      window.parent.postMessage({type:'__edit_mode_set_keys', edits: tweaks}, '*');
    }, [tweaks]);

    // Keyboard shortcuts
    React.useEffect(() => {
      const onKey = (e) => {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
          e.preventDefault(); setModal(m => m==='cmd' ? null : 'cmd');
        }
        if ((e.metaKey || e.ctrlKey) && e.key === ',') { e.preventDefault(); setModal('prefs'); }
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'n') { e.preventDefault(); setModal('add'); }
        if (e.key === 'Escape') setModal(null);
      };
      window.addEventListener('keydown', onKey);
      return () => window.removeEventListener('keydown', onKey);
    }, []);

    // Filter torrents
    const rows = React.useMemo(() => {
      let r = MOCK.torrents;
      if (filter === 'downloading') r = r.filter(t => t.status==='downloading' || t.status==='stalled' || t.status==='checking');
      else if (filter === 'seeding') r = r.filter(t => t.status==='seeding');
      else if (filter === 'completed') r = r.filter(t => t.progress === 1);
      else if (filter === 'paused') r = r.filter(t => t.status==='paused');
      else if (filter === 'active') r = r.filter(t => t.dl !== '0 B/s' || t.ul !== '0 B/s');
      else if (filter === 'inactive') r = r.filter(t => t.dl === '0 B/s' && t.ul === '0 B/s');
      else if (filter === 'error') r = r.filter(t => t.status==='error');
      if (category !== 'all' && category !== 'uncategorized') r = r.filter(t => t.category === category);
      return r;
    }, [filter, category]);

    const torrent = MOCK.torrents.find(t => t.id === selected);

    const MainContent = () => {
      if (nav === 'torrents') {
        if (tweaks.layoutVariant === 'L1') {
          return (
            <div style={{display:'flex', flexDirection:'column', flex:1, minWidth: 0}}>
              <div style={{flex:'1 1 55%', minHeight: 0, display:'flex'}}>
                <TorrentList rows={rows} selected={selected} setSelected={setSelected} striping={tweaks.striping}/>
              </div>
              <div style={{flex:'1 1 45%', minHeight: 0, borderTop:'1px solid var(--border-1)', display:'flex'}}>
                <DetailsPane torrent={torrent}/>
              </div>
            </div>
          );
        }
        if (tweaks.layoutVariant === 'L2') {
          return (
            <div style={{display:'flex', flex:1, minWidth:0}}>
              <TorrentList rows={rows} selected={selected} setSelected={setSelected} striping={tweaks.striping}/>
              <div style={{width: 460, flexShrink: 0, borderLeft: '1px solid var(--border-1)', display:'flex'}}>
                <DetailsPane torrent={torrent} onClose={()=>setSelected(null)}/>
              </div>
            </div>
          );
        }
        // L3 — Command workspace: just torrent list full-bleed; details via keyboard
        return (
          <div style={{display:'flex', flex:1, minWidth:0}}>
            <TorrentList rows={rows} selected={selected} setSelected={setSelected} striping={tweaks.striping}/>
          </div>
        );
      }
      const Page = {
        search: Tools.SearchPage, rss: Tools.RSSPage, scheduler: Tools.SchedulerPage,
        stats: Tools.StatsPage, logs: Tools.LogsPage, ipfilter: Tools.IPFilterPage,
        create: Tools.CreatePage, webui: Tools.WebUIPage,
      }[nav];
      return Page ? <Page/> : null;
    };

    return (
      <div style={{position:'relative', display:'flex', flexDirection:'column', height: '100vh', width: '100vw', background:'var(--bg-0)', overflow:'hidden'}}>
        <Chrome.WindowChrome subtitle={tweaks.layoutVariant === 'L3' ? 'Command workspace' : tweaks.layoutVariant === 'L2' ? 'Inspector drawer' : '3-pane'} />
        <Chrome.MenuBar />
        <Toolbar
          onAdd={() => setModal('add')}
          onAddMagnet={() => setModal('add')}
          openPrefs={() => setModal('prefs')}
          openCommand={() => setModal('cmd')}
          layoutVariant={tweaks.layoutVariant}
          setLayoutVariant={v => setTweaks({...tweaks, layoutVariant: v})}
          theme={tweaks.theme}
          setTheme={v => setTweaks({...tweaks, theme: v})}
          dl="13.7 MB/s" ul="3.2 MB/s" connections="128"
        />
        <div style={{flex:1, display:'flex', minHeight: 0}}>
          <Sidebar nav={nav} setNav={setNav} filter={filter} setFilter={setFilter}
            category={category} setCategory={setCategory} labelMode={tweaks.sidebar}/>
          <MainContent/>
        </div>

        {modal==='add' && <Modals.AddTorrentDialog onClose={()=>setModal(null)}/>}
        {modal==='create' && <Modals.CreateTorrentDialog onClose={()=>setModal(null)}/>}
        {modal==='cmd' && <Modals.CommandPalette onClose={()=>setModal(null)}/>}
        {modal==='prefs' && <PrefsDialog onClose={()=>setModal(null)}/>}

        {tweaksOpen && <TweaksPanel tweaks={tweaks} setTweaks={setTweaks} onClose={()=>setTweaksOpen(false)}/>}
      </div>
    );
  }

  window.IronTideApp = App;
})();
