// IronTide — mock data. Everything the UI reads lives here.
// Hashes, sizes, ratios, peers, etc. are hand-tuned to look believable.

window.MOCK = (() => {
  const torrents = [
    {
      id: 't1',
      name: 'Ubuntu 24.04.2 LTS Desktop (amd64).iso',
      hash: '2c6b7985b3b2f3d7a0e9b6c0d1a3e4f5a6b7c8d9',
      size: '4.68 GB', sizeBytes: 5023232000, done: 1.0, progress: 1.0,
      status: 'seeding', dl: '0 B/s', ul: '1.2 MB/s', seeds: '812 (12,403)', peers: '34 (921)',
      ratio: 3.42, eta: '∞', added: '2026-03-14 09:12', completed: '2026-03-14 09:48',
      category: 'Linux', tags: ['distro','verified'], tracker: 'tracker.ubuntu.com',
      savePath: '/Volumes/Storage/Torrents/Linux',
      priority: 'normal', availability: 99.9, pieces: '4784 / 4784', pieceSize: '1.00 MiB',
    },
    {
      id: 't2',
      name: 'Blender.4.2.LTS.Splash.Project.Files.zip',
      hash: 'a1f9c8b7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1',
      size: '18.4 GB', sizeBytes: 19756000000, done: 0.47, progress: 0.47,
      status: 'downloading', dl: '8.4 MB/s', ul: '412 KB/s', seeds: '46 (129)', peers: '18 (204)',
      ratio: 0.08, eta: '19m 48s', added: '2026-04-18 21:03', completed: null,
      category: 'Software', tags: ['assets'], tracker: 'open.blender.tracker',
      savePath: '/Volumes/Storage/Torrents/Software',
      priority: 'high', availability: 4.12, pieces: '9012 / 18900', pieceSize: '1.00 MiB',
    },
    {
      id: 't3',
      name: 'arXiv-astro-ph-2026-Q1-preprints.tar',
      hash: 'b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1',
      size: '2.14 GB', sizeBytes: 2297000000, done: 0.92, progress: 0.92,
      status: 'downloading', dl: '1.1 MB/s', ul: '84 KB/s', seeds: '7 (12)', peers: '3 (8)',
      ratio: 0.31, eta: '2m 04s', added: '2026-04-19 06:41', completed: null,
      category: 'Papers', tags: ['research','science'], tracker: 'academictorrents.com',
      savePath: '/Users/claude/Downloads/Papers',
      priority: 'normal', availability: 1.86, pieces: '2012 / 2187', pieceSize: '1.00 MiB',
    },
    {
      id: 't4',
      name: 'debian-12.5.0-amd64-netinst.iso',
      hash: 'c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2',
      size: '631 MB', sizeBytes: 661000000, done: 1.0, progress: 1.0,
      status: 'seeding', dl: '0 B/s', ul: '512 KB/s', seeds: '1,204 (4,211)', peers: '22 (118)',
      ratio: 8.11, eta: '∞', added: '2026-01-22 14:50', completed: '2026-01-22 15:04',
      category: 'Linux', tags: ['distro','verified'], tracker: 'bttracker.debian.org',
      savePath: '/Volumes/Storage/Torrents/Linux',
      priority: 'normal', availability: 99.9, pieces: '631 / 631', pieceSize: '1.00 MiB',
    },
    {
      id: 't5',
      name: 'Project.Gutenberg.Top.5000.EPUB.Collection.zip',
      hash: 'd4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3',
      size: '12.9 GB', sizeBytes: 13851000000, done: 0.0, progress: 0.0,
      status: 'queued', dl: '0 B/s', ul: '0 B/s', seeds: '24 (88)', peers: '0 (12)',
      ratio: 0.00, eta: '—', added: '2026-04-19 07:12', completed: null,
      category: 'Books', tags: ['library'], tracker: 'academictorrents.com',
      savePath: '/Users/claude/Downloads/Books',
      priority: 'low', availability: 2.31, pieces: '0 / 13200', pieceSize: '1.00 MiB',
    },
    {
      id: 't6',
      name: 'OpenStreetMap.Planet.2026-04-01.osm.pbf',
      hash: 'e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4',
      size: '78.2 GB', sizeBytes: 83978000000, done: 0.14, progress: 0.14,
      status: 'stalled', dl: '24 KB/s', ul: '0 B/s', seeds: '3 (5)', peers: '1 (2)',
      ratio: 0.02, eta: '∞', added: '2026-04-10 02:14', completed: null,
      category: 'Datasets', tags: ['geo'], tracker: 'planet.osm.org',
      savePath: '/Volumes/Storage/Datasets',
      priority: 'low', availability: 0.42, pieces: '11200 / 80000', pieceSize: '1.00 MiB',
    },
    {
      id: 't7',
      name: 'KDE.Plasma.6.3.Wallpapers.Official.tar.xz',
      hash: 'f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5',
      size: '1.02 GB', sizeBytes: 1095000000, done: 1.0, progress: 1.0,
      status: 'paused', dl: '0 B/s', ul: '0 B/s', seeds: '88 (312)', peers: '0 (24)',
      ratio: 1.88, eta: '∞', added: '2026-02-08 10:00', completed: '2026-02-08 10:11',
      category: 'Software', tags: [], tracker: 'mirrors.kde.org',
      savePath: '/Volumes/Storage/Torrents/Software',
      priority: 'normal', availability: 99.9, pieces: '1024 / 1024', pieceSize: '1.00 MiB',
    },
    {
      id: 't8',
      name: 'NASA.Apollo.11.Restoration.4K.ProRes.Samples.zip',
      hash: 'a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6',
      size: '44.1 GB', sizeBytes: 47338000000, done: 0.03, progress: 0.03,
      status: 'checking', dl: '0 B/s', ul: '0 B/s', seeds: '—', peers: '—',
      ratio: 0.00, eta: '—', added: '2026-04-19 07:42', completed: null,
      category: 'Video', tags: ['archival'], tracker: '(none)',
      savePath: '/Volumes/Storage/Torrents/Video',
      priority: 'normal', availability: 0, pieces: '1320 / 45000', pieceSize: '2.00 MiB',
    },
    {
      id: 't9',
      name: 'Rust.Programming.Language.Book.2026-Q1.epub',
      hash: 'b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7',
      size: '48.2 MB', sizeBytes: 50540000, done: 1.0, progress: 1.0,
      status: 'seeding', dl: '0 B/s', ul: '22 KB/s', seeds: '412 (1,203)', peers: '8 (44)',
      ratio: 12.4, eta: '∞', added: '2025-11-30 22:01', completed: '2025-11-30 22:02',
      category: 'Books', tags: ['verified','programming'], tracker: 'tracker.rust-lang.org',
      savePath: '/Users/claude/Downloads/Books',
      priority: 'normal', availability: 99.9, pieces: '50 / 50', pieceSize: '1.00 MiB',
    },
    {
      id: 't10',
      name: 'archive.org.Public.Domain.1920s.Films.Collection.tar',
      hash: 'c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8',
      size: '212 GB', sizeBytes: 227633000000, done: 0.0, progress: 0.0,
      status: 'error', dl: '0 B/s', ul: '0 B/s', seeds: '0 (0)', peers: '0 (0)',
      ratio: 0.00, eta: '—', added: '2026-04-19 07:58', completed: null,
      category: 'Video', tags: ['archival'], tracker: 'tracker.archive.org',
      savePath: '/Volumes/Storage/Torrents/Video',
      priority: 'normal', availability: 0, pieces: '0 / 216000', pieceSize: '1.00 MiB',
      errorMsg: 'Tracker returned: unregistered torrent',
    },
    {
      id: 't11',
      name: 'fedora-workstation-40-1.14-x86_64.iso',
      hash: 'd0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9',
      size: '2.11 GB', sizeBytes: 2265000000, done: 0.78, progress: 0.78,
      status: 'downloading', dl: '4.2 MB/s', ul: '1.1 MB/s', seeds: '203 (612)', peers: '41 (188)',
      ratio: 0.42, eta: '1m 52s', added: '2026-04-19 07:20', completed: null,
      category: 'Linux', tags: ['distro'], tracker: 'torrent.fedoraproject.org',
      savePath: '/Volumes/Storage/Torrents/Linux',
      priority: 'high', availability: 8.12, pieces: '1687 / 2164', pieceSize: '1.00 MiB',
    },
    {
      id: 't12',
      name: 'LibreOffice.7.6.Sources.Windows.Linux.Mac.tar.gz',
      hash: 'e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0',
      size: '6.8 GB', sizeBytes: 7301000000, done: 1.0, progress: 1.0,
      status: 'seeding', dl: '0 B/s', ul: '212 KB/s', seeds: '62 (280)', peers: '12 (48)',
      ratio: 2.01, eta: '∞', added: '2026-03-29 12:03', completed: '2026-03-29 13:18',
      category: 'Software', tags: ['verified'], tracker: 'dev.libreoffice.org',
      savePath: '/Volumes/Storage/Torrents/Software',
      priority: 'normal', availability: 99.9, pieces: '6950 / 6950', pieceSize: '1.00 MiB',
    },
  ];

  const categories = [
    { id: 'all', name: 'All', count: torrents.length, pathRule: '' },
    { id: 'Linux', name: 'Linux ISOs', count: 3, pathRule: '/Volumes/Storage/Torrents/Linux' },
    { id: 'Software', name: 'Software', count: 3, pathRule: '/Volumes/Storage/Torrents/Software' },
    { id: 'Video', name: 'Video', count: 2, pathRule: '/Volumes/Storage/Torrents/Video' },
    { id: 'Books', name: 'Books', count: 2, pathRule: '/Users/claude/Downloads/Books' },
    { id: 'Papers', name: 'Research Papers', count: 1, pathRule: '/Users/claude/Downloads/Papers' },
    { id: 'Datasets', name: 'Datasets', count: 1, pathRule: '/Volumes/Storage/Datasets' },
    { id: 'uncategorized', name: 'Uncategorized', count: 0, pathRule: '' },
  ];

  const tags = [
    { id: 'distro', count: 3 }, { id: 'verified', count: 5 },
    { id: 'archival', count: 2 }, { id: 'library', count: 1 },
    { id: 'research', count: 1 }, { id: 'assets', count: 1 },
    { id: 'geo', count: 1 }, { id: 'programming', count: 1 }, { id: 'science', count: 1 },
  ];

  const trackers = [
    { id: 'all', name: 'All trackers', count: 12 },
    { id: 'working', name: 'Working', count: 10 },
    { id: 'unreachable', name: 'Unreachable', count: 1 },
    { id: 'error', name: 'Error', count: 1 },
    { id: 'tracker.ubuntu.com', count: 1 },
    { id: 'open.blender.tracker', count: 1 },
    { id: 'academictorrents.com', count: 2 },
    { id: 'bttracker.debian.org', count: 1 },
    { id: 'planet.osm.org', count: 1 },
    { id: 'mirrors.kde.org', count: 1 },
    { id: 'tracker.archive.org', count: 1 },
    { id: 'torrent.fedoraproject.org', count: 1 },
    { id: 'tracker.rust-lang.org', count: 1 },
    { id: 'dev.libreoffice.org', count: 1 },
  ];

  // File tree for details pane
  const fileTree = [
    { name: 'blender-4.2-splash', kind: 'dir', progress: 0.47, priority: 'normal', children: [
      { name: 'assets', kind: 'dir', progress: 0.62, priority: 'high', children: [
        { name: 'characters.blend', kind: 'file', size: '842 MB', progress: 1.00, priority: 'high' },
        { name: 'environment.blend', kind: 'file', size: '2.14 GB', progress: 1.00, priority: 'high' },
        { name: 'props.blend', kind: 'file', size: '1.02 GB', progress: 0.42, priority: 'high' },
        { name: 'textures_4k.zip', kind: 'file', size: '6.81 GB', progress: 0.18, priority: 'normal' },
      ]},
      { name: 'renders', kind: 'dir', progress: 0.22, priority: 'normal', children: [
        { name: 'preview.mp4', kind: 'file', size: '412 MB', progress: 1.00, priority: 'normal' },
        { name: 'final_4k.mov', kind: 'file', size: '4.22 GB', progress: 0.14, priority: 'low' },
        { name: 'turntable.mp4', kind: 'file', size: '88 MB', progress: 1.00, priority: 'normal' },
      ]},
      { name: 'source', kind: 'dir', progress: 0.88, priority: 'normal', children: [
        { name: 'scene.blend', kind: 'file', size: '1.88 GB', progress: 1.00, priority: 'normal' },
        { name: 'scripts.py', kind: 'file', size: '42 KB', progress: 1.00, priority: 'normal' },
        { name: 'README.md', kind: 'file', size: '8 KB', progress: 1.00, priority: 'normal' },
      ]},
      { name: 'licence.txt', kind: 'file', size: '12 KB', progress: 1.00, priority: 'normal' },
    ]},
  ];

  const peers = [
    { ip: '73.201.44.12',     port: 51413, conn: 'BT', flags: 'D X I', client: 'qBittorrent 4.6.0',  progress: 1.00, dl: '1.2 MB/s', ul: '0 B/s',    rel: '1.02 MB', country: 'US' },
    { ip: '2a01:e0a:3f2::1a', port: 6881,  conn: 'BT', flags: 'd x i', client: 'Transmission 4.0.5', progress: 0.88, dl: '812 KB/s', ul: '42 KB/s',  rel: '412 KB',  country: 'FR' },
    { ip: '119.224.15.203',   port: 42069, conn: 'µ',  flags: 'd I',   client: 'µTorrent 3.6.0',     progress: 0.42, dl: '612 KB/s', ul: '204 KB/s', rel: '1.18 MB', country: 'NZ' },
    { ip: '198.51.100.22',    port: 51413, conn: 'BT', flags: 'D U',   client: 'Deluge 2.1.1',       progress: 0.23, dl: '484 KB/s', ul: '812 KB/s', rel: '2.04 MB', country: 'CA' },
    { ip: '185.199.108.14',   port: 6969,  conn: 'BT', flags: 'E',     client: 'libtorrent 2.0.9',   progress: 0.02, dl: '212 KB/s', ul: '1.1 MB/s', rel: '3.21 MB', country: 'DE' },
    { ip: '203.0.113.7',      port: 8999,  conn: 'BT', flags: 'D X',   client: 'IronTide 0.1.0',     progress: 0.18, dl: '120 KB/s', ul: '42 KB/s',  rel: '88 KB',   country: 'JP' },
    { ip: '51.15.44.201',     port: 51413, conn: 'BT', flags: 'd',     client: 'qBittorrent 5.0.0',  progress: 0.66, dl: '0 B/s',    ul: '212 KB/s', rel: '1.42 MB', country: 'NL' },
    { ip: '94.156.35.19',     port: 42188, conn: 'BT', flags: 'E P',   client: 'BiglyBT 3.5.0.0',    progress: 0.91, dl: '0 B/s',    ul: '412 KB/s', rel: '2.88 MB', country: 'SE' },
  ];

  const trackersList = [
    { url: '** [DHT] **',     status: 'working',     peers: 48, seeds: 102, leech: 24, downloaded: '—', msg: 'Working' },
    { url: '** [PeX] **',     status: 'working',     peers: 12, seeds: 18,  leech: 6,  downloaded: '—', msg: 'Working' },
    { url: '** [LSD] **',     status: 'working',     peers: 0,  seeds: 0,   leech: 0,  downloaded: '—', msg: 'Working' },
    { url: 'udp://open.blender.tracker:80/announce',  status: 'working',    peers: 34, seeds: 46, leech: 128, downloaded: '12k', msg: 'Working' },
    { url: 'udp://tracker.openbittorrent.com:80/announce', status: 'working', peers: 8, seeds: 12, leech: 44, downloaded: '—', msg: 'Working' },
    { url: 'http://bt.example.org:6969/announce',     status: 'unreachable', peers: 0, seeds: 0,  leech: 0,  downloaded: '—',   msg: 'Timeout: no response in 15s' },
    { url: 'udp://tracker.opentrackr.org:1337/announce', status: 'working', peers: 14, seeds: 22, leech: 82, downloaded: '—', msg: 'Working' },
  ];

  const httpSources = [
    { url: 'https://mirrors.blender.org/splash/textures_4k.zip',  status: 'connected' },
    { url: 'https://mirror.cdn.example.com/splash/textures_4k.zip', status: 'connected' },
  ];

  // 60-point speed graph
  const speedGraph = (() => {
    const pts = [];
    let dl = 6, ul = 0.8;
    for (let i = 0; i < 60; i++) {
      dl = Math.max(0.2, dl + (Math.random() - 0.5) * 1.8);
      ul = Math.max(0.05, ul + (Math.random() - 0.5) * 0.3);
      pts.push({ t: i, dl, ul });
    }
    return pts;
  })();

  const rssFeeds = [
    { id: 'f1', name: 'Linux ISOs',     url: 'https://tracker.example.org/rss/linux.xml',      unread: 4,  last: '2m ago' },
    { id: 'f2', name: 'Academic Torrents', url: 'https://academictorrents.com/rss.xml',          unread: 12, last: '8m ago' },
    { id: 'f3', name: 'Archive.org New', url: 'https://archive.org/rss/new-uploads.xml',       unread: 0,  last: '22m ago' },
    { id: 'f4', name: 'Public Datasets',url: 'https://datasets.example.com/feed.atom',          unread: 2,  last: '1h ago' },
  ];

  const rssItems = [
    { id: 'r1', feed: 'Linux ISOs', title: 'ubuntu-24.04.3-desktop-amd64.iso',      date: '2026-04-19 07:42', size: '4.71 GB', matched: 'Ubuntu auto-download' },
    { id: 'r2', feed: 'Linux ISOs', title: 'debian-12.6.0-amd64-netinst.iso',        date: '2026-04-19 06:11', size: '642 MB',  matched: null },
    { id: 'r3', feed: 'Academic Torrents', title: 'CORD-19.2026-Q1.latest.tar',     date: '2026-04-19 05:22', size: '18.9 GB', matched: 'Medical research' },
    { id: 'r4', feed: 'Linux ISOs', title: 'fedora-workstation-41-1.beta.iso',      date: '2026-04-18 22:10', size: '2.22 GB', matched: null },
    { id: 'r5', feed: 'Public Datasets', title: 'OSM-Planet-Weekly-2026-W16.pbf',    date: '2026-04-18 18:02', size: '82.1 GB', matched: 'OSM weekly' },
    { id: 'r6', feed: 'Archive.org New', title: 'Apollo-11-Restoration-4K-Sample.mov', date: '2026-04-18 12:44', size: '4.12 GB', matched: null },
  ];

  const rssRules = [
    { id: 'ar1', name: 'Ubuntu auto-download', enabled: true, mustContain: 'ubuntu-*-amd64.iso', mustNotContain: 'beta|rc', episodeFilter: '', smartFilter: true, category: 'Linux', savePath: '/Volumes/Storage/Torrents/Linux', feeds: ['Linux ISOs'] },
    { id: 'ar2', name: 'Medical research',     enabled: true, mustContain: 'CORD-*|biomed-*',    mustNotContain: '',        episodeFilter: '', smartFilter: false, category: 'Papers', savePath: '/Users/claude/Downloads/Papers', feeds: ['Academic Torrents'] },
    { id: 'ar3', name: 'OSM weekly',            enabled: false, mustContain: 'OSM-Planet-Weekly-*', mustNotContain: '',       episodeFilter: '', smartFilter: false, category: 'Datasets', savePath: '/Volumes/Storage/Datasets', feeds: ['Public Datasets'] },
  ];

  const searchResults = [
    { name: 'Ubuntu 24.04.3 LTS Desktop (amd64).iso',  size: '4.71 GB', seeds: 1204, peers: 312, engine: 'LinuxTracker', date: '2026-04-19', url: '#' },
    { name: 'Ubuntu 24.04.3 LTS Server (amd64).iso',    size: '2.11 GB', seeds: 812,  peers: 144, engine: 'LinuxTracker', date: '2026-04-19', url: '#' },
    { name: 'Ubuntu 24.04.3 LTS Desktop (arm64).iso',   size: '4.68 GB', seeds: 404,  peers: 88,  engine: 'LinuxTracker', date: '2026-04-19', url: '#' },
    { name: 'ubuntu-mate-24.04.3-amd64.iso',            size: '3.21 GB', seeds: 142,  peers: 42,  engine: 'LinuxTracker', date: '2026-04-19', url: '#' },
    { name: 'kubuntu-24.04.3-amd64.iso',                size: '3.44 GB', seeds: 88,   peers: 24,  engine: 'LinuxTracker', date: '2026-04-18', url: '#' },
    { name: 'lubuntu-24.04.3-amd64.iso',                size: '2.81 GB', seeds: 66,   peers: 18,  engine: 'LinuxTracker', date: '2026-04-18', url: '#' },
    { name: 'xubuntu-24.04.3-amd64.iso',                size: '2.92 GB', seeds: 44,   peers: 12,  engine: 'LinuxTracker', date: '2026-04-18', url: '#' },
    { name: 'ubuntu-budgie-24.04.3-amd64.iso',          size: '3.08 GB', seeds: 22,   peers: 6,   engine: 'LinuxTracker', date: '2026-04-18', url: '#' },
  ];

  const searchEngines = [
    { id: 'linuxtracker', name: 'LinuxTracker', url: 'linuxtracker.org', enabled: true, version: '1.0.4' },
    { id: 'archive',      name: 'Archive.org',  url: 'archive.org',      enabled: true, version: '2.1.0' },
    { id: 'academic',     name: 'AcademicTorrents', url: 'academictorrents.com', enabled: true, version: '1.2.0' },
    { id: 'eztv',         name: 'EZTV',         url: 'eztv.re',          enabled: false, version: '1.1.0' },
    { id: 'rarbg',        name: 'RARBG (archived)', url: 'rarbgmirror.org', enabled: false, version: '0.9.1' },
    { id: 'thepiratebay', name: 'ThePirateBay', url: 'thepiratebay.org', enabled: false, version: '2.0.3' },
    { id: 'nyaa',         name: 'Nyaa',         url: 'nyaa.si',          enabled: false, version: '1.0.2' },
  ];

  const logs = [
    { t: '07:58:22.412', level: 'ERROR',  msg: 'Tracker "tracker.archive.org" returned: unregistered torrent (archive.org.Public.Domain.1920s.Films)' },
    { t: '07:58:20.101', level: 'INFO',   msg: 'Added torrent: archive.org.Public.Domain.1920s.Films.Collection.tar' },
    { t: '07:42:10.887', level: 'INFO',   msg: 'Download completed: Rust.Programming.Language.Book.2026-Q1.epub' },
    { t: '07:42:09.221', level: 'INFO',   msg: 'File check passed for NASA.Apollo.11.Restoration.4K (12/45000 pieces OK, continuing)' },
    { t: '07:41:44.000', level: 'INFO',   msg: 'Peer 119.224.15.203:42069 connected (µTorrent 3.6.0)' },
    { t: '07:41:22.412', level: 'WARN',   msg: 'UPnP port mapping failed on router "Fritz!Box 7590": timeout' },
    { t: '07:40:11.001', level: 'INFO',   msg: 'DHT: bootstrap complete, 1,024 nodes in routing table' },
    { t: '07:39:52.334', level: 'INFO',   msg: 'IronTide 0.1.0 started — listening on *:6881 (BT), 8080 (WebUI local)' },
    { t: '07:39:51.002', level: 'DEBUG',  msg: 'Loaded 12 torrents from /Users/claude/.config/irontide/state.bin' },
    { t: '07:39:50.001', level: 'INFO',   msg: 'Config loaded from /Users/claude/.config/irontide/config.toml' },
    { t: '07:12:04.331', level: 'INFO',   msg: 'RSS: fetched feed "Academic Torrents" — 2 new items' },
    { t: '07:12:03.221', level: 'INFO',   msg: 'RSS: fetched feed "Linux ISOs" — 4 new items' },
    { t: '06:41:22.000', level: 'INFO',   msg: 'Added torrent (magnet): arXiv-astro-ph-2026-Q1-preprints.tar' },
    { t: '06:40:01.112', level: 'DEBUG',  msg: 'Choking algorithm: 4 upload slots allocated' },
    { t: '06:31:14.422', level: 'ERROR',  msg: 'IP filter: blocked connection from 45.133.1.22 (matched range 45.133.0.0/20)' },
  ];

  const stats = {
    allTime: {
      downloaded: '4.22 TB', uploaded: '12.8 TB', ratio: 3.03,
      addedTotal: 1242, seededTotal: 812, sharedTime: '412 days, 8 hours',
      sessionUp: '18:42:11', sessionDl: '144 GB', sessionUl: '42 GB',
      globalPeers: 128, dhtNodes: 1024, connectionsActive: 88, connectionsMax: 200,
    },
  };

  return { torrents, categories, tags, trackers, fileTree, peers, trackersList, httpSources, speedGraph, rssFeeds, rssItems, rssRules, searchResults, searchEngines, logs, stats };
})();
