// IronTide — Preferences dialog with 8 tabs: Behavior, Downloads, Connection, Speed, BitTorrent, RSS, WebUI, Advanced.
(() => {
  const { Icon, IT } = window;

  function Field({ label, hint, children, stacked }) {
    return (
      <div style={{
        display: stacked ? 'block' : 'grid',
        gridTemplateColumns: stacked ? undefined : '240px 1fr',
        gap: stacked ? 6 : 14,
        alignItems: 'baseline',
        padding: '8px 0',
      }}>
        <div style={{fontSize: 12.5, color:'var(--fg-1)'}}>
          {label}
          {hint ? <div style={{fontSize: 11, color:'var(--fg-3)', marginTop: 2, lineHeight: 1.4}}>{hint}</div> : null}
        </div>
        <div style={{display:'flex', alignItems:'center', gap: 8, flexWrap:'wrap'}}>{children}</div>
      </div>
    );
  }

  function Group({ title, children }) {
    return (
      <div style={{marginBottom: 22}}>
        <div style={{
          fontSize: 11, fontWeight: 600, textTransform:'uppercase', letterSpacing:'.06em',
          color: 'var(--fg-2)', paddingBottom: 6,
          borderBottom: '1px solid var(--divider)', marginBottom: 6,
        }}>{title}</div>
        {children}
      </div>
    );
  }

  const TABS = [
    { id: 'behavior',   label: 'Behavior',    icon: Icon.gear },
    { id: 'downloads',  label: 'Downloads',   icon: Icon.download },
    { id: 'connection', label: 'Connection',  icon: Icon.globe },
    { id: 'speed',      label: 'Speed',       icon: Icon.bolt },
    { id: 'bittorrent', label: 'BitTorrent',  icon: Icon.tracker },
    { id: 'rss',        label: 'RSS',         icon: Icon.rss },
    { id: 'webui',      label: 'Web UI',      icon: Icon.webui },
    { id: 'advanced',   label: 'Advanced',    icon: Icon.shield },
  ];

  function PrefsDialog({ onClose }) {
    const [tab, setTab] = React.useState('behavior');

    return (
      <div style={{
        position:'absolute', inset: 0,
        background: 'oklch(0.15 0.01 240 / 0.45)',
        display:'flex', alignItems:'center', justifyContent:'center',
        zIndex: 50,
      }} onClick={onClose}>
        <div onClick={e=>e.stopPropagation()} style={{
          width: 960, height: 640, maxWidth: '94vw', maxHeight: '92vh',
          background:'var(--bg-0)', borderRadius:'var(--r-lg)',
          border: '1px solid var(--border-1)', boxShadow:'var(--shadow-lg)',
          display:'flex', flexDirection:'column', overflow:'hidden',
        }}>
          <div style={{
            height: 42, flexShrink:0, padding:'0 14px',
            display:'flex', alignItems:'center',
            borderBottom: '1px solid var(--border-1)',
            background:'var(--bg-1)',
          }}>
            <span style={{fontSize: 13, fontWeight: 600}}>Preferences</span>
            <div style={{flex:1}}/>
            <IT.IconBtn icon={Icon.x({size:14})} onClick={onClose}/>
          </div>

          <div style={{flex:1, display:'flex', minHeight: 0}}>
            {/* Left nav */}
            <div style={{
              width: 200, flexShrink: 0, padding: 8,
              borderRight:'1px solid var(--border-1)', background:'var(--bg-1)',
            }}>
              {TABS.map(t => (
                <div key={t.id} onClick={()=>setTab(t.id)} style={{
                  display:'flex', alignItems:'center', gap: 8,
                  height: 30, padding: '0 10px', borderRadius:'var(--r-md)',
                  cursor:'pointer', fontSize: 13,
                  background: tab===t.id ? 'var(--bg-selected)' : 'transparent',
                  color: tab===t.id ? 'var(--fg-0)' : 'var(--fg-1)',
                }}>
                  <span style={{color:'var(--fg-2)'}}>{t.icon({size:14})}</span>
                  {t.label}
                </div>
              ))}
            </div>

            {/* Content */}
            <div style={{flex:1, overflowY:'auto', padding: 20}}>
              {tab==='behavior' && (
                <>
                  <Group title="Interface">
                    <Field label="Density"><IT.Select value="Compact" options={['Compact','Balanced','Spacious']}/></Field>
                    <Field label="Language"><IT.Select value="English (US)" options={['English (US)','English (UK)','Français','Deutsch','日本語','中文 (简体)']}/></Field>
                    <Field label="Confirm before deleting"><IT.Toggle on/></Field>
                    <Field label="Confirm pause all / resume all"><IT.Toggle on={false}/></Field>
                    <Field label="Show splash screen"><IT.Toggle on={false}/></Field>
                    <Field label="Show torrent-added toast"><IT.Toggle on/></Field>
                    <Field label="Double-click on torrent"><IT.Select value="Open destination folder" options={['Open destination folder','Show details','Pause/Resume','Do nothing']}/></Field>
                    <Field label="Keyboard shortcut set"><IT.Select value="Default" options={['Default','Emacs-style','Vim-style','qBittorrent parity']}/></Field>
                  </Group>
                  <Group title="Startup">
                    <Field label="Start IronTide on system login"><IT.Toggle on/></Field>
                    <Field label="Start minimized"><IT.Toggle on={false}/></Field>
                    <Field label="Minimize to tray on close"><IT.Toggle on/></Field>
                    <Field label="Resume torrents from previous session"><IT.Toggle on/></Field>
                  </Group>
                  <Group title="Notifications">
                    <Field label="On torrent complete"><IT.Toggle on/></Field>
                    <Field label="On torrent error"><IT.Toggle on/></Field>
                    <Field label="On RSS match"><IT.Toggle on/></Field>
                    <Field label="Play sound" hint="on completion"><IT.Toggle on={false}/></Field>
                    <Field label="Run external program on completion" hint="%N = name, %F = file, %D = save path, %I = hash"><IT.TextInput mono placeholder="/usr/local/bin/notify.sh %N" width={360}/></Field>
                  </Group>
                </>
              )}

              {tab==='downloads' && (
                <>
                  <Group title="Save locations">
                    <Field label="Default save path"><IT.TextInput mono value="/home/alan/Downloads/Torrents" width={360}/><IT.Btn variant="solid" size="sm">Browse…</IT.Btn></Field>
                    <Field label="Keep incomplete in separate folder"><IT.Toggle on/></Field>
                    <Field label="Incomplete save path"><IT.TextInput mono value="/home/alan/Downloads/.incomplete" width={360}/><IT.Btn variant="solid" size="sm">Browse…</IT.Btn></Field>
                    <Field label="Append .!it extension to incomplete files"><IT.Toggle on={false}/></Field>
                    <Field label="Use auto-categories" hint="Route torrents to folders based on category rules"><IT.Toggle on/></Field>
                  </Group>
                  <Group title="When adding a torrent">
                    <Field label="Show add-torrent dialog"><IT.Toggle on/></Field>
                    <Field label="Start paused"><IT.Toggle on={false}/></Field>
                    <Field label="Skip hash check"><IT.Toggle on={false}/></Field>
                    <Field label="Pre-allocate disk space"><IT.Toggle on/></Field>
                    <Field label="Append date to save path"><IT.Toggle on={false}/></Field>
                    <Field label="Use smart category suggestion (AI)" hint="Detect Linux ISO / Video / Books from file names and route automatically"><IT.Toggle on/></Field>
                  </Group>
                  <Group title="Torrent file handling">
                    <Field label="Watched folder" hint="Auto-add .torrent files dropped here"><IT.TextInput mono value="/home/alan/Downloads/.watch" width={360}/><IT.Btn variant="solid" size="sm">Browse…</IT.Btn></Field>
                    <Field label="Copy .torrent files to"><IT.TextInput mono value="/home/alan/.local/share/irontide/torrents" width={360}/></Field>
                    <Field label="Move .torrent files of completed downloads"><IT.TextInput mono value="/home/alan/.local/share/irontide/done" width={360}/></Field>
                    <Field label="Delete .torrent after adding"><IT.Toggle on={false}/></Field>
                  </Group>
                  <Group title="On completion">
                    <Field label="Move completed downloads to"><IT.Toggle on={false}/><IT.TextInput mono value="/Volumes/Archive" width={280}/></Field>
                    <Field label="Run external program"><IT.TextInput mono placeholder="/usr/local/bin/on-complete.sh %N" width={360}/></Field>
                  </Group>
                </>
              )}

              {tab==='connection' && (
                <>
                  <Group title="Listening port">
                    <Field label="Incoming port"><IT.TextInput mono value="6881" width={100}/><IT.Btn variant="ghost" size="sm" icon={Icon.refresh({size:12})}>Random</IT.Btn></Field>
                    <Field label="Use different port on each startup"><IT.Toggle on={false}/></Field>
                    <Field label="UPnP / NAT-PMP port forwarding"><IT.Toggle on/></Field>
                    <Field label="Port mapping status"><IT.Chip tone="complete">Forwarded</IT.Chip><span style={{fontSize:11, color:'var(--fg-3)'}} className="mono">via Fritz!Box 7590</span></Field>
                  </Group>
                  <Group title="Connections limit">
                    <Field label="Global max connections"><IT.TextInput mono value="200" width={100}/></Field>
                    <Field label="Max connections per torrent"><IT.TextInput mono value="100" width={100}/></Field>
                    <Field label="Max upload slots (global)"><IT.TextInput mono value="20" width={100}/></Field>
                    <Field label="Max upload slots per torrent"><IT.TextInput mono value="4" width={100}/></Field>
                    <Field label="Max active downloads"><IT.TextInput mono value="5" width={100}/></Field>
                    <Field label="Max active uploads"><IT.TextInput mono value="3" width={100}/></Field>
                    <Field label="Max active torrents total"><IT.TextInput mono value="8" width={100}/></Field>
                  </Group>
                  <Group title="Proxy">
                    <Field label="Type"><IT.Select value="None" options={['None','HTTP','SOCKS4','SOCKS5','SOCKS5 (username/password)']} width={200}/></Field>
                    <Field label="Host"><IT.TextInput mono placeholder="proxy.example.com" width={280}/></Field>
                    <Field label="Port"><IT.TextInput mono value="1080" width={100}/></Field>
                    <Field label="Use proxy for peer connections"><IT.Toggle on={false}/></Field>
                    <Field label="Use proxy for hostname lookups"><IT.Toggle on={false}/></Field>
                  </Group>
                  <Group title="IP Filtering">
                    <Field label="Enable IP filter"><IT.Toggle on/></Field>
                    <Field label="Filter file"><IT.TextInput mono value="/home/alan/.local/share/irontide/ipfilter.dat" width={360}/></Field>
                    <Field label="Automatically refresh filter on startup"><IT.Toggle on/></Field>
                  </Group>
                </>
              )}

              {tab==='speed' && (
                <>
                  <Group title="Global rate limits">
                    <Field label="Download limit"><IT.Toggle on={false}/><IT.TextInput mono value="∞" width={100} right suffix="KB/s"/></Field>
                    <Field label="Upload limit"><IT.Toggle on/><IT.TextInput mono value="5000" width={100} right suffix="KB/s"/></Field>
                  </Group>
                  <Group title="Alternative rate limits">
                    <Field label="Download limit"><IT.TextInput mono value="500" width={100} right suffix="KB/s"/></Field>
                    <Field label="Upload limit"><IT.TextInput mono value="100" width={100} right suffix="KB/s"/></Field>
                    <Field label="Scheduled hours" hint="Use alternative limits at these times"><IT.Select value="22:00 – 07:00 weekdays" options={['22:00 – 07:00 weekdays','22:00 – 07:00 daily','All weekend','Never','Custom…']}/></Field>
                    <Field label="Auto-switch on peak hours"><IT.Toggle on/></Field>
                  </Group>
                  <Group title="Rate limits apply to">
                    <Field label="Transport overhead"><IT.Toggle on/></Field>
                    <Field label="µTP connections"><IT.Toggle on/></Field>
                    <Field label="Peers on the same LAN"><IT.Toggle on={false}/></Field>
                  </Group>
                </>
              )}

              {tab==='bittorrent' && (
                <>
                  <Group title="Privacy">
                    <Field label="DHT (decentralized network)"><IT.Toggle on/></Field>
                    <Field label="Peer exchange (PeX)"><IT.Toggle on/></Field>
                    <Field label="Local Peer Discovery (LSD)"><IT.Toggle on/></Field>
                    <Field label="Encryption"><IT.Select value="Prefer encryption" options={['Prefer encryption','Require encryption','Disable encryption']} width={220}/></Field>
                    <Field label="Anonymous mode" hint="Hide client version and user-agent; disable LSD/DHT/UPnP"><IT.Toggle on={false}/></Field>
                  </Group>
                  <Group title="Seeding limits">
                    <Field label="When ratio reaches"><IT.Toggle on/><IT.TextInput mono value="2.00" width={80} right/><IT.Select value="Pause torrent" options={['Pause torrent','Remove torrent','Remove torrent and data','Super-seeding mode']}/></Field>
                    <Field label="When seeding time reaches"><IT.Toggle on={false}/><IT.TextInput mono value="1440" width={80} right suffix="min"/></Field>
                    <Field label="When inactive seeding time reaches"><IT.Toggle on={false}/><IT.TextInput mono value="60" width={80} right suffix="min"/></Field>
                  </Group>
                  <Group title="Queueing">
                    <Field label="Enable torrent queueing"><IT.Toggle on/></Field>
                    <Field label="Do not count slow torrents in queue"><IT.Toggle on/></Field>
                    <Field label="Slow torrent DL threshold"><IT.TextInput mono value="2" width={80} right suffix="KB/s"/></Field>
                    <Field label="Slow torrent UL threshold"><IT.TextInput mono value="2" width={80} right suffix="KB/s"/></Field>
                  </Group>
                </>
              )}

              {tab==='rss' && (
                <>
                  <Group title="RSS reader">
                    <Field label="Enable fetching"><IT.Toggle on/></Field>
                    <Field label="Feeds refresh interval"><IT.TextInput mono value="15" width={80} right suffix="min"/></Field>
                    <Field label="Maximum articles per feed"><IT.TextInput mono value="50" width={80} right/></Field>
                  </Group>
                  <Group title="Auto-download">
                    <Field label="Enable auto-downloading"><IT.Toggle on/></Field>
                    <Field label="Smart episode filter"><IT.Toggle on/></Field>
                    <Field label="Smart filter regex" hint="Matched against item titles"><IT.TextInput mono value="s\\d{1,4}e\\d{1,4}|\\d{1,4}x\\d{1,4}" width={300}/></Field>
                    <Field label="Download repacks/proper"><IT.Toggle on/></Field>
                  </Group>
                </>
              )}

              {tab==='webui' && (
                <>
                  <Group title="Web User Interface">
                    <Field label="Enable"><IT.Toggle on/></Field>
                    <Field label="Listen on"><IT.TextInput mono value="0.0.0.0" width={160}/><span style={{color:'var(--fg-3)'}}>:</span><IT.TextInput mono value="8080" width={80}/></Field>
                    <Field label="Use HTTPS"><IT.Toggle on={false}/></Field>
                    <Field label="Certificate path"><IT.TextInput mono placeholder="/etc/irontide/cert.pem" width={320}/></Field>
                    <Field label="Key path"><IT.TextInput mono placeholder="/etc/irontide/key.pem" width={320}/></Field>
                  </Group>
                  <Group title="Authentication">
                    <Field label="Username"><IT.TextInput mono value="admin" width={200}/></Field>
                    <Field label="Password"><IT.TextInput mono type="password" value="********" width={200}/><IT.Btn variant="solid" size="sm">Change</IT.Btn></Field>
                    <Field label="Bypass auth on localhost"><IT.Toggle on/></Field>
                    <Field label="Session timeout"><IT.TextInput mono value="3600" width={80} right suffix="sec"/></Field>
                    <Field label="Max login attempts"><IT.TextInput mono value="5" width={80}/></Field>
                    <Field label="Ban on failed attempts for"><IT.TextInput mono value="1800" width={80} right suffix="sec"/></Field>
                  </Group>
                  <Group title="Security">
                    <Field label="Clickjacking protection"><IT.Toggle on/></Field>
                    <Field label="CSRF protection"><IT.Toggle on/></Field>
                    <Field label="Host header validation"><IT.Toggle on/></Field>
                    <Field label="Allowed host list"><IT.TextInput mono placeholder="*.example.com" width={320}/></Field>
                    <Field label="Reverse proxy support" hint="Trust X-Forwarded-For from these IPs"><IT.TextInput mono placeholder="127.0.0.1/32, 10.0.0.0/8" width={320}/></Field>
                  </Group>
                  <Group title="Dynamic DNS">
                    <Field label="Enable"><IT.Toggle on={false}/></Field>
                    <Field label="Service"><IT.Select value="dyndns.org" options={['dyndns.org','no-ip.com','duckdns.org','cloudflare']}/></Field>
                    <Field label="Domain"><IT.TextInput mono placeholder="my-seed.duckdns.org" width={280}/></Field>
                  </Group>
                </>
              )}

              {tab==='advanced' && (
                <>
                  <Group title="libtorrent tuning">
                    <Field label="Async I/O threads"><IT.TextInput mono value="10" width={80}/></Field>
                    <Field label="Hashing threads"><IT.TextInput mono value="4" width={80}/></Field>
                    <Field label="File pool size"><IT.TextInput mono value="5000" width={80}/></Field>
                    <Field label="Outstanding memory when checking"><IT.TextInput mono value="32" width={80} right suffix="MiB"/></Field>
                    <Field label="Disk cache size"><IT.TextInput mono value="-1" width={80} right suffix="MiB"/><span style={{fontSize:11, color:'var(--fg-3)'}}>(-1 = auto)</span></Field>
                    <Field label="Disk cache expiry"><IT.TextInput mono value="60" width={80} right suffix="sec"/></Field>
                    <Field label="Coalesce reads & writes"><IT.Toggle on/></Field>
                    <Field label="Piece extent affinity"><IT.Toggle on={false}/></Field>
                    <Field label="Socket send buffer"><IT.TextInput mono value="0" width={80} right suffix="bytes"/></Field>
                    <Field label="Socket receive buffer"><IT.TextInput mono value="0" width={80} right suffix="bytes"/></Field>
                  </Group>
                  <Group title="Network interfaces">
                    <Field label="Network interface"><IT.Select value="Any" options={['Any','en0 (Ethernet)','en1 (Wi-Fi)','utun3 (VPN)']}/></Field>
                    <Field label="Optional IP address to bind to"><IT.TextInput mono placeholder="0.0.0.0" width={200}/></Field>
                    <Field label="IP address reported to tracker"><IT.Select value="Auto-detected" options={['Auto-detected','Custom IP','None']}/></Field>
                    <Field label="Program update check"><IT.Toggle on/></Field>
                    <Field label="Resolve peer countries"><IT.Toggle on/></Field>
                    <Field label="Resolve peer hostnames"><IT.Toggle on={false}/></Field>
                  </Group>
                  <Group title="Behavior">
                    <Field label="Strict super-seeding"><IT.Toggle on={false}/></Field>
                    <Field label="Announce all trackers in tier"><IT.Toggle on={false}/></Field>
                    <Field label="Announce to all tiers"><IT.Toggle on={false}/></Field>
                    <Field label="µTP-TCP mixed mode"><IT.Select value="Prefer TCP" options={['Prefer TCP','Peer proportional','Disable TCP','Disable µTP']}/></Field>
                    <Field label="Always announce to all trackers"><IT.Toggle on={false}/></Field>
                    <Field label="Save resume data interval"><IT.TextInput mono value="60" width={80} right suffix="min"/></Field>
                  </Group>
                  <Group title="Export configuration">
                    <Field label="Configuration file"><span className="mono" style={{fontSize:12, color:'var(--fg-2)'}}>/home/alan/.config/irontide/config.toml</span><IT.Btn variant="solid" size="sm">Reveal</IT.Btn></Field>
                    <Field label="Reset to defaults"><IT.Btn variant="solid" size="sm" icon={Icon.refresh({size:12})}>Reset…</IT.Btn></Field>
                  </Group>
                </>
              )}
            </div>
          </div>

          <div style={{
            height: 50, flexShrink: 0,
            borderTop: '1px solid var(--border-1)',
            background:'var(--bg-1)',
            display:'flex', alignItems:'center', justifyContent:'flex-end',
            padding:'0 14px', gap: 8,
          }}>
            <IT.Btn variant="ghost" onClick={onClose}>Cancel</IT.Btn>
            <IT.Btn variant="solid">Apply</IT.Btn>
            <IT.Btn variant="primary" onClick={onClose}>OK</IT.Btn>
          </div>
        </div>
      </div>
    );
  }

  window.PrefsDialog = PrefsDialog;
})();
