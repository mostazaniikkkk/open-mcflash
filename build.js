// Bundles web/ into a single OMF.html that works by double-clicking (file://),
// without a server: CSS, JS and the .wasm are embedded.
// Usage: node build.js   (after compiling with wasm-pack)
const fs = require('fs');
const path = require('path');

const web = path.join(__dirname, 'web');
const read = (p) => fs.readFileSync(path.join(web, p), 'utf8');
const dataUrl = (js) => 'data:text/javascript;base64,' + Buffer.from(js).toString('base64');

let main = read('js/main.js')
  .replace("'../pkg/omf_core.js'", `'${dataUrl(read('pkg/omf_core.js'))}'`)
  .replace("'./icons.js'", `'${dataUrl(read('js/icons.js'))}'`)
  .replace("'./timeline.js'", `'${dataUrl(read('js/timeline.js'))}'`)
  .replace("'./library.js'", `'${dataUrl(read('js/library.js'))}'`)
  .replace("'./gradient.js'", `'${dataUrl(read('js/gradient.js'))}'`)
  .replace("'./textui.js'", `'${dataUrl(read('js/textui.js'))}'`)
  .replace("'./io.js'", `'${dataUrl(read('js/io.js'))}'`)
  .replace("'./rulers.js'", `'${dataUrl(read('js/rulers.js'))}'`)
  .replace("'./actions.js'", `'${dataUrl(read('js/actions.js'))}'`)
  .replace("'./audio.js'", `'${dataUrl(read('js/audio.js'))}'`)
  .replace("'./ruffle.js'", `'${dataUrl(read('js/ruffle.js'))}'`)
  .replace("'./history.js'", `'${dataUrl(read('js/history.js'))}'`)
  .replace("'./spell.js'", `'${dataUrl(read('js/spell.js').replace("'../vendor/nspell.mjs'", `'${dataUrl(read('vendor/nspell.mjs'))}'`))}'`);
// Ruffle for "Test Movie" (MIT / Apache-2.0).
const ruffle = [
  ['ruffle.js', 'ruffle.js'],
  ['core.ruffle.994eeef0a471a88cd0e3.js', 'core.ruffle.994eeef0a471a88cd0e3.js'],
  ['878d548199cddef07035.wasm', '878d548199cddef07035.wasm'],
];
// Spell-checking dictionaries (gzip, in base64).
const dicts = {};
for (const lang of ['es', 'en']) {
  dicts[lang] = {};
  for (const part of ['aff', 'dic']) dicts[lang][part] = fs.readFileSync(path.join(web, `dict/${lang}.${part}.gz`)).toString('base64');
}
const wasm = fs.readFileSync(path.join(web, 'pkg/omf_core_bg.wasm')).toString('base64');

const html = read('index.html')
  .replace('<link rel="stylesheet" href="css/style.css">', () => `<style>\n${read('css/style.css')}\n</style>`)
  .replace('<script type="module" src="js/main.js"></script>', () =>
    `<script>globalThis.OMF_WASM = Uint8Array.from(atob("${wasm}"), (c) => c.charCodeAt(0));</script>\n` +
    `<script>globalThis.OMF_DICTS = ${JSON.stringify(dicts)};</script>\n` +
    // Ruffle (gzip in base64): not executed, read when "Test Movie" opens.
    ruffle.map(([id, f]) => `<script type="application/octet-stream" id="ruffle-${id}">${fs.readFileSync(path.join(web, 'vendor/ruffle', f + '.gz')).toString('base64')}</script>\n`).join('') +
    `<script type="module">\n${main.replace(/<\/script/gi, '<\\/script')}\n</script>`);

const out = path.join(__dirname, 'OMF.html');
fs.writeFileSync(out, html);
console.log(`OMF.html generado (${(fs.statSync(out).size / 1024 / 1024).toFixed(2)} MB)`);

// ───────── Installable version (PWA): dist/ folder to upload to an https
// host (or test with `node serve.js dist`). Edge and Chrome install it as an
// app; it works offline and opens .omf files.
const zlib = require('zlib');
const dist = path.join(__dirname, 'dist');
fs.mkdirSync(dist, { recursive: true });
fs.writeFileSync(path.join(dist, 'index.html'), html);

// Icon: the white film reel on the red rectangle (same as the favicon).
function iconPng(size, pad = 0) {
  const S = 4; // supersampling
  const px = Buffer.alloc(size * size * 4);
  const k = (size - pad * 2) / 32;
  const shapes = [
    ['rrect', 0, 0, 32, 32, 6, [200, 16, 46]],
    ['rrect', 16, 24, 11, 2.6, 1.3, [255, 255, 255]],
    ['circle', 16, 16, 10.5, [255, 255, 255]],
    ...[[16, 10.6], [21.1, 14.3], [19.2, 20.4], [12.8, 20.4], [10.9, 14.3]].map(([x, y]) => ['circle', x, y, 2.5, [200, 16, 46]]),
    ['circle', 16, 16, 1.4, [200, 16, 46]],
  ];
  const inside = (sh, x, y) => {
    if (sh[0] === 'circle') return (x - sh[1]) ** 2 + (y - sh[2]) ** 2 <= sh[3] ** 2;
    const [, rx, ry, w, h, r] = sh;
    if (x < rx || y < ry || x > rx + w || y > ry + h) return false;
    const cx = Math.min(Math.max(x, rx + r), rx + w - r), cy = Math.min(Math.max(y, ry + r), ry + h - r);
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r;
  };
  for (let j = 0; j < size; j++) {
    for (let i = 0; i < size; i++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let sj = 0; sj < S; sj++) {
        for (let si = 0; si < S; si++) {
          const x = (i + (si + 0.5) / S - pad) / k, y = (j + (sj + 0.5) / S - pad) / k;
          let c = null;
          for (const sh of shapes) if (inside(sh, x, y)) c = sh[sh.length - 1];
          if (c) { r += c[0]; g += c[1]; b += c[2]; a++; }
        }
      }
      const o = (j * size + i) * 4;
      if (a) { px[o] = r / a; px[o + 1] = g / a; px[o + 2] = b / a; px[o + 3] = (a / (S * S)) * 255; }
    }
  }
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let j = 0; j < size; j++) px.copy(raw, j * (size * 4 + 1) + 1, j * size * 4, (j + 1) * size * 4);
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let q = 0; q < 8; q++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (buf) => { let c = 0xffffffff; for (const v of buf) c = crcTable[(c ^ v) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (type, data) => {
    const t = Buffer.concat([Buffer.from(type), data]);
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const c = Buffer.alloc(4); c.writeUInt32BE(crc(t));
    return Buffer.concat([len, t, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0); ihdr.writeUInt32BE(size, 4); ihdr[8] = 8; ihdr[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
for (const n of [192, 512]) {
  fs.writeFileSync(path.join(dist, `icon-${n}.png`), iconPng(n));
  // "maskable": with padding so the system can crop it to a circle or teardrop.
  fs.writeFileSync(path.join(dist, `icon-${n}-maskable.png`), iconPng(n, Math.round(n * 0.1)));
}

const manifest = {
  name: 'Open McFlash',
  short_name: 'McFlash',
  description: 'Editor de animación al estilo Flash 8 que funciona en el navegador',
  lang: 'es',
  start_url: './',
  scope: './',
  id: './',
  display: 'standalone',
  background_color: '#1b1b1b',
  theme_color: '#252525',
  icons: [
    { src: 'icon-192.png', sizes: '192x192', type: 'image/png' },
    { src: 'icon-512.png', sizes: '512x512', type: 'image/png' },
    { src: 'icon-192-maskable.png', sizes: '192x192', type: 'image/png', purpose: 'maskable' },
    { src: 'icon-512-maskable.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
  ],
  file_handlers: [{ action: './', accept: { 'application/x-openmcflash': ['.omf'] } }],
  launch_handler: { client_mode: 'focus-existing' },
  categories: ['graphics', 'design', 'productivity'],
};
fs.writeFileSync(path.join(dist, 'manifest.webmanifest'), JSON.stringify(manifest, null, 2));

// Service worker: caches the app for offline use; each new version
// (different size or date of the index) replaces the previous one.
const version = require('crypto').createHash('sha1').update(html).digest('hex').slice(0, 12);
fs.writeFileSync(path.join(dist, 'sw.js'), `// Open McFlash: offline cache (version ${version}).
const CACHE = 'omf-${version}';
const FILES = ['./', 'manifest.webmanifest', 'icon-192.png', 'icon-512.png', 'icon-192-maskable.png', 'icon-512-maskable.png'];
self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(FILES)).then(() => self.skipWaiting()));
});
self.addEventListener('activate', (e) => {
  e.waitUntil(caches.keys().then((ks) => Promise.all(ks.filter((k) => k !== CACHE).map((k) => caches.delete(k)))).then(() => self.clients.claim()));
});
self.addEventListener('fetch', (e) => {
  if (e.request.method !== 'GET') return;
  const url = new URL(e.request.url);
  if (url.origin !== location.origin) return;
  // The page: network first (to get new versions) and, when offline, the cached one.
  if (e.request.mode === 'navigate') {
    e.respondWith(fetch(e.request).then((r) => { const c = r.clone(); caches.open(CACHE).then((x) => x.put('./', c)); return r; }).catch(() => caches.match('./')));
    return;
  }
  e.respondWith(caches.match(e.request).then((r) => r || fetch(e.request)));
});
`);
console.log('dist/ generado (PWA: index.html, manifest.webmanifest, sw.js e iconos)');
