// Embedded Ruffle (MIT / Apache-2.0): the "Test Movie" player.
//
// Ruffle is published to be served from a folder: it loads its core and its
// WebAssembly by relative URL. Here everything lives inside the page (OMF.html
// works by double-clicking, without a server), so it is unpacked in memory
// and given blob: URLs: when loading it, the two places where Ruffle decides
// which files to request are rewritten.

const FILES = {
  ruffle: 'ruffle.js',
  core: 'core.ruffle.994eeef0a471a88cd0e3.js',
  wasm: '878d548199cddef07035.wasm',
};

async function gunzip(bytes) {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Bytes of a Ruffle file: embedded in OMF.html or, during development, from web/vendor/ruffle. */
async function file(name) {
  const el = document.getElementById(`ruffle-${name}`);
  let gz;
  if (el) gz = Uint8Array.from(atob(el.textContent.trim()), (c) => c.charCodeAt(0));
  else gz = new Uint8Array(await (await fetch(`vendor/ruffle/${name}.gz`)).arrayBuffer());
  return gunzip(gz);
}

const PATCHES = [
  // Always the variant without extensions (the only one included; works in every browser).
  ['await(n?a.e(110)', 'await(!1?a.e(110)'],
  ['const s=n?new URL(a(797),a.b):new URL(a(124),a.b)', `const s=new URL(globalThis.OMF_RUFFLE_FILES["${FILES.wasm}"],a.b)`],
  // The core is loaded with a <script> tag: its URL comes from the table.
  ['s.src=t', 's.src=(globalThis.OMF_RUFFLE_FILES||{})[String(t).split("/").pop()]||t'],
];

let loading = null;

/** Loads Ruffle only once and returns its API (`window.RufflePlayer.newest()`). */
export function loadRuffle() {
  if (!loading) {
    loading = (async () => {
      const [js, core, wasm] = await Promise.all([file(FILES.ruffle), file(FILES.core), file(FILES.wasm)]);
      globalThis.OMF_RUFFLE_FILES = {
        [FILES.core]: URL.createObjectURL(new Blob([core], { type: 'text/javascript' })),
        [FILES.wasm]: URL.createObjectURL(new Blob([wasm], { type: 'application/wasm' })),
      };
      let src = new TextDecoder().decode(js);
      for (const [a, b] of PATCHES) {
        if (!src.includes(a)) throw new Error(`Ruffle: no se encontró «${a}»`);
        src = src.replace(a, b);
      }
      window.RufflePlayer = window.RufflePlayer || {};
      window.RufflePlayer.config = {
        publicPath: '',
        autoplay: 'on',
        unmuteOverlay: 'hidden',
        splashScreen: false,
        letterbox: 'on',
        warnOnUnsupportedContent: false,
        showSwfDownload: false,
        polyfills: false,
        logLevel: 'info',
        ...(window.RufflePlayer.config || {}),
      };
      await new Promise((ok, fail) => {
        const s = document.createElement('script');
        s.src = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
        s.onload = ok;
        s.onerror = () => fail(new Error('Ruffle no se pudo cargar'));
        document.head.appendChild(s);
      });
      return window.RufflePlayer.newest();
    })();
    loading.catch(() => (loading = null));
  }
  return loading;
}
