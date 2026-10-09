// Minimal static server: node serve.js [folder] [port]
// Without a folder it serves web/ (development); `node serve.js dist` serves
// the installable version (PWA): on http://localhost Edge and Chrome offer to install it.
const http = require('http');
const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
const dir = args.find((a) => !/^\d+$/.test(a)) || 'web';
const root = path.resolve(__dirname, dir);
const port = +args.find((a) => /^\d+$/.test(a)) || 8737;
const types = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8',
  '.wasm': 'application/wasm', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.webmanifest': 'application/manifest+json',
};

http.createServer((req, res) => {
  const url = decodeURIComponent(req.url.split('?')[0]);
  let file = path.normalize(path.join(root, url === '/' ? 'index.html' : url));
  if (!file.startsWith(root)) return res.writeHead(403).end();
  fs.readFile(file, (err, data) => {
    if (err) return res.writeHead(404).end('No encontrado');
    res.writeHead(200, { 'Content-Type': types[path.extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
    res.end(data);
  });
}).listen(port, '127.0.0.1', () => console.log(`Open McFlash en http://localhost:${port}`));
