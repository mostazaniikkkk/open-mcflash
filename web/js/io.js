// File > Import (images to the Stage or to the Library), Publish Settings,
// Publish and Publish Preview.

const $ = (q, el = document) => el.querySelector(q);
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

/** Web player that replaces the Flash plugin in the published HTML. */
export const RUFFLE_URL = 'https://unpkg.com/@ruffle-rs/ruffle';

export function initIO(app) {
  const { ed } = app;

  // ───────── Import ─────────

  /** Any image the browser understands → PNG (what the core stores). */
  async function toPng(file) {
    const bmp = await createImageBitmap(file);
    const c = document.createElement('canvas');
    c.width = bmp.width;
    c.height = bmp.height;
    c.getContext('2d').drawImage(bmp, 0, 0);
    bmp.close?.();
    const blob = await new Promise((res) => c.toBlob(res, 'image/png'));
    return new Uint8Array(await blob.arrayBuffer());
  }

  const isAudio = (f) => /^audio\//.test(f.type) || /\.(wav|mp3|ogg|oga|m4a|aac|flac|aif|aiff)$/i.test(f.name);

  async function importFiles(files, toStage) {
    let n = 0;
    for (const f of files) {
      // Sounds always go to the Library (as in Flash).
      if (isAudio(f)) {
        try {
          if (await app.audio.importFile(f)) n++;
        } catch {
          app.toast(`No se pudo leer el sonido «${f.name}»`);
        }
        continue;
      }
      if (!/^image\//.test(f.type) && !/\.(png|jpe?g|gif|bmp|webp)$/i.test(f.name)) {
        app.toast(`«${f.name}» no es una imagen ni un sonido`);
        continue;
      }
      try {
        const png = await toPng(f);
        if (ed.import_bitmap(f.name, png, toStage)) n++;
      } catch {
        app.toast(`No se pudo leer «${f.name}»`);
      }
    }
    app.refresh();
    if (n) app.toast(toStage ? `Importado al escenario (${n})` : `Importado a la biblioteca (${n})`);
  }

  function importDialog(toStage) {
    const inp = document.createElement('input');
    inp.type = 'file';
    inp.accept = 'image/*,audio/*,.wav,.mp3,.ogg';
    inp.multiple = true;
    inp.id = 'import-input';
    inp.hidden = true;
    inp.addEventListener('change', () => {
      importFiles([...inp.files], toStage);
      inp.remove();
    });
    document.body.appendChild(inp);
    inp.click();
  }

  // Dropping images on the Stage imports them there.
  const vp = $('#viewport');
  vp.addEventListener('dragover', (e) => {
    if ([...e.dataTransfer.items].some((i) => i.kind === 'file')) e.preventDefault();
  });
  vp.addEventListener('drop', (e) => {
    const files = [...e.dataTransfer.files];
    if (!files.length) return;
    e.preventDefault();
    importFiles(files, true);
  });

  // ───────── Publish ─────────

  const settings = () => JSON.parse(ed.publish_settings());
  const baseName = () => settings().name || app.docName();

  function htmlPage(p, { swfFile, swfBytes }) {
    const S = app.state();
    const w = p.size === 'match' ? S.width : p.size === 'percent' ? `${p.width}%` : p.width;
    const h = p.size === 'match' ? S.height : p.size === 'percent' ? `${p.height}%` : p.height;
    const cfg = { autoplay: p.paused ? 'off' : 'on', unmuteOverlay: 'hidden', splashScreen: false, contextMenu: p.menu ? 'on' : 'off', backgroundColor: S.bg };
    const ruffle = p.ruffle ? `\n<script>window.RufflePlayer = window.RufflePlayer || {}; window.RufflePlayer.config = ${JSON.stringify(cfg)};</script>\n<script src="${RUFFLE_URL}"></script>` : '';
    const params = `<param name="movie" value="${esc(swfFile)}"><param name="play" value="${!p.paused}"><param name="loop" value="${p.looping ?? p.loop}"><param name="menu" value="${p.menu}"><param name="quality" value="high"><param name="bgcolor" value="${S.bg}">`;
    const body = swfBytes
      // Preview: the movie is embedded in the page.
      ? `<div id="omf-movie" style="width:${typeof w === 'number' ? w + 'px' : w};height:${typeof h === 'number' ? h + 'px' : h}"></div>
<script>
window.addEventListener('load', async () => {
  const p = window.RufflePlayer.newest().createPlayer();
  p.style.width = '100%'; p.style.height = '100%';
  document.getElementById('omf-movie').appendChild(p);
  await p.ruffle().load({ data: Uint8Array.from(atob('${swfBytes}'), (c) => c.charCodeAt(0)), allowScriptAccess: false });
});
</script>`
      : `<object type="application/x-shockwave-flash" data="${esc(swfFile)}" width="${w}" height="${h}">${params}</object>`;
    return `<!DOCTYPE html>
<html lang="es">
<head>
<meta charset="utf-8">
<title>${esc(baseName())}</title>
<style>html, body { margin: 0; height: 100%; background: ${S.bg}; } body { display: flex; align-items: center; justify-content: center; }</style>${ruffle}
</head>
<body>
${body}
</body>
</html>
`;
  }

  function publish() {
    const p = settings();
    const name = baseName();
    let n = 0;
    if (p.swf || p.html) {
      app.download(`${name}.swf`, new Blob([ed.export_swf()], { type: 'application/x-shockwave-flash' }));
      n++;
    }
    if (p.html) {
      app.download(`${name}.html`, new Blob([htmlPage(p, { swfFile: `${name}.swf` })], { type: 'text/html' }));
      n++;
    }
    if (p.png) {
      app.download(`${name}.png`, new Blob([ed.export_png(1)], { type: 'image/png' }));
      n++;
    }
    app.toast(n ? `Publicado: ${[p.swf || p.html ? 'SWF' : '', p.html ? 'HTML' : '', p.png ? 'PNG' : ''].filter(Boolean).join(', ')}` : 'No hay ningún formato marcado en la configuración de publicación');
  }

  function previewHTML() {
    const swf = ed.export_swf();
    let bin = '';
    for (let i = 0; i < swf.length; i += 0x8000) bin += String.fromCharCode.apply(null, swf.subarray(i, i + 0x8000));
    const html = htmlPage({ ...settings(), ruffle: true }, { swfFile: '', swfBytes: btoa(bin) });
    const w = window.open('', '_blank');
    if (!w) return app.toast('El navegador bloqueó la ventana de vista previa');
    w.document.open();
    w.document.write(html);
    w.document.close();
  }

  function settingsDialog() {
    const p = settings();
    const chk = (id, label, on) => `<label class="chk"><input type="checkbox" id="${id}" ${on ? 'checked' : ''}> ${label}</label>`;
    app.modal('Configuración de publicación', `
      <div class="pub-tabs"><button class="btn small on" data-ptab="formats">Formatos</button><button class="btn small" data-ptab="flash">Flash</button><button class="btn small" data-ptab="html">HTML</button></div>
      <div class="pub-page" data-page="formats">
        <div class="prow"><span class="lab" style="width:90px">Nombre</span><input type="text" id="pb-name" placeholder="${esc(app.docName())}" value="${esc(p.name)}" style="width:200px"></div>
        <div style="display:grid;gap:6px;margin-left:96px">${chk('pb-swf', 'Flash (.swf)', p.swf)}${chk('pb-html', 'HTML (.html)', p.html)}${chk('pb-png', 'Imagen PNG (.png)', p.png)}</div>
      </div>
      <div class="pub-page" data-page="flash" hidden>
        <div class="prow"><span class="lab" style="width:150px">Versión</span><select disabled><option>Flash Player 8</option></select></div>
        <div class="prow"><span class="lab" style="width:150px">Versión de ActionScript</span><select disabled><option>ActionScript 2.0</option></select></div>
        <div style="display:grid;gap:6px;margin-left:156px">${chk('pb-compress', 'Comprimir película', p.compress)}${chk('pb-protect', 'Proteger frente a importación', p.protect)}${chk('pb-hidden', 'Exportar capas ocultas', app.state().exportHidden)}</div>
      </div>
      <div class="pub-page" data-page="html" hidden>
        <div class="prow"><span class="lab" style="width:110px">Plantilla</span><select id="pb-tpl"><option>Solo Flash</option></select></div>
        <div class="prow"><span class="lab" style="width:110px">Dimensiones</span><select id="pb-size"><option value="match">Igual que la película</option><option value="pixels">Píxeles</option><option value="percent">Porcentaje</option></select>
          <input type="number" id="pb-w" value="${p.width}" style="width:60px"> × <input type="number" id="pb-h" value="${p.height}" style="width:60px"></div>
        <div class="prow"><span class="lab" style="width:110px">Reproducción</span><div style="display:grid;gap:6px">${chk('pb-paused', 'Pausa al comenzar', p.paused)}${chk('pb-loop', 'Reproducir indefinidamente', p.loop)}${chk('pb-menu', 'Mostrar menú', p.menu)}</div></div>
        <div class="prow"><span class="lab" style="width:110px">Reproductor</span>${chk('pb-ruffle', 'Incluir Ruffle (reproduce el SWF en navegadores actuales)', p.ruffle)}</div>
      </div>
      <div class="modal-actions"><button class="btn" id="pb-publish">Publicar</button><div class="grow"></div><button class="btn" data-close>Cancelar</button><button class="btn primary" id="pb-ok">Aceptar</button></div>`, (b) => {
      $('#pb-size', b).value = p.size;
      b.addEventListener('click', (e) => {
        const t = e.target.closest('[data-ptab]');
        if (!t) return;
        b.querySelectorAll('[data-ptab]').forEach((x) => x.classList.toggle('on', x === t));
        b.querySelectorAll('[data-page]').forEach((x) => (x.hidden = x.dataset.page !== t.dataset.ptab));
      });
      const read = () => ({
        swf: $('#pb-swf', b).checked, html: $('#pb-html', b).checked, png: $('#pb-png', b).checked,
        name: $('#pb-name', b).value.trim(), compress: $('#pb-compress', b).checked, protect: $('#pb-protect', b).checked,
        size: $('#pb-size', b).value, width: +$('#pb-w', b).value || 550, height: +$('#pb-h', b).value || 400,
        paused: $('#pb-paused', b).checked, loop: $('#pb-loop', b).checked, menu: $('#pb-menu', b).checked, ruffle: $('#pb-ruffle', b).checked,
      });
      const save = () => {
        ed.set_publish_settings(JSON.stringify(read()));
        if ($('#pb-hidden', b).checked !== app.state().exportHidden) ed.set_export_hidden($('#pb-hidden', b).checked);
      };
      $('#pb-ok', b).addEventListener('click', () => { save(); app.closeModal(); app.refresh(); });
      $('#pb-publish', b).addEventListener('click', () => { save(); app.closeModal(); app.refresh(); publish(); });
    });
  }

  return { importDialog, importFiles, publish, previewHTML, settingsDialog, htmlPage };
}
