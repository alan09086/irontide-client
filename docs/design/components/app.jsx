// IronTide — main app shell. Combines all pieces.
(() => {
  const { Icon, IT, Sidebar, TorrentList, DetailsPane, Toolbar, Tools, Modals, PrefsDialog, TweaksPanel, Chrome, FirstRunWizard } = window;

  // Locked single direction (Alan Gaudet emerald-dark, KDE, L1 3-pane).
  // Only a few tasteful runtime tweaks remain.
  const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
    "density": "compact",
    "sidebar": "full",
    "striping": true,
    "accentGlow": true,
    "detailsPanel": true
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

    // Apply the few runtime tweaks to root
    React.useEffect(() => {
      document.documentElement.dataset.density = tweaks.density;
      document.documentElement.dataset.glow = tweaks.accentGlow ? 'on' : 'off';
      try { localStorage.setItem('irontide-tweaks', JSON.stringify(tweaks)); } catch {}
    }, [tweaks]);

    // Screenshot / automation hook (harmless; used by the reference gallery)
    React.useEffect(() => {
      window.__it = { nav: setNav, modal: setModal, select: setSelected, tweak: setTweaks };
    }, []);

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

    // Dispatch a menu-bar action key
    const onMenu = (k) => {
      if (!k) return;
      if (k === 'add' || k === 'addMagnet') { setNav('torrents'); setModal('add'); }
      else if (k === 'create') setModal('create');
      else if (k === 'command') setModal('cmd');
      else if (k === 'prefs') setModal('prefs');
      else if (k.startsWith('nav:')) setNav(k.slice(4));
      else if (k.startsWith('sidebar:')) setTweaks(t => ({ ...t, sidebar: k.slice(8) }));
      else if (k.startsWith('density:')) setTweaks(t => ({ ...t, density: k.slice(8) }));
      else if (k === 'toggleStriping') setTweaks(t => ({ ...t, striping: !t.striping }));
      else if (k === 'toggleGlow') setTweaks(t => ({ ...t, accentGlow: !t.accentGlow }));
      else if (k === 'toggleDetails') setTweaks(t => ({ ...t, detailsPanel: t.detailsPanel === false }));
      else if (k === 'firstrun') setModal('firstrun');
      else if (k === 'remove' || k === 'removeData') { setNav('torrents'); setModal('confirm'); }
      else if (k === 'selectAll') { setNav('torrents'); }
      // Edit/Help/export actions are mapped in the spec; no-op in the static prototype.
    };

    // Keyboard shortcuts
    React.useEffect(() => {
      const onKey = (e) => {
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
          e.preventDefault(); setModal(m => m==='cmd' ? null : 'cmd');
        }
        if ((e.metaKey || e.ctrlKey) && e.key === ',') { e.preventDefault(); setModal('prefs'); }
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'n') { e.preventDefault(); setModal('create'); }
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'i') { e.preventDefault(); setTweaks(t => ({ ...t, detailsPanel: t.detailsPanel === false })); }
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
        // L1 — the single locked layout: list (top) over docked details (bottom)
        return (
          <div style={{display:'flex', flexDirection:'column', flex:1, minWidth: 0}}>
            <div style={{flex: tweaks.detailsPanel === false ? '1 1 100%' : '1 1 56%', minHeight: 0, display:'flex'}}>
              <TorrentList rows={rows} selected={selected} setSelected={setSelected} striping={tweaks.striping}/>
            </div>
            {tweaks.detailsPanel === false ? null : (
              <div style={{flex:'1 1 44%', minHeight: 0, borderTop:'1px solid var(--border-1)', display:'flex'}}>
                <DetailsPane torrent={torrent}/>
              </div>
            )}
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
        <Chrome.WindowChrome />
        <Chrome.MenuBar state={tweaks} onItem={onMenu} />
        <Toolbar
          onAdd={() => setModal('add')}
          onAddMagnet={() => setModal('add')}
          openPrefs={() => setModal('prefs')}
          openCommand={() => setModal('cmd')}
          onRemove={() => { setNav('torrents'); setModal('confirm'); }}
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
        {modal==='confirm' && <Modals.ConfirmDeleteDialog onClose={()=>setModal(null)}/>}
        {modal==='firstrun' && <FirstRunWizard onClose={()=>setModal(null)}/>}
        {modal==='prefs' && <PrefsDialog onClose={()=>setModal(null)}/>}

        {tweaksOpen && <TweaksPanel tweaks={tweaks} setTweaks={setTweaks} onClose={()=>setTweaksOpen(false)}/>}
      </div>
    );
  }

  window.IronTideApp = App;
})();
