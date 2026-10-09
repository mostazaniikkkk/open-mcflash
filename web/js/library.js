// Symbols: Library panel, Convert to Symbol (F8), New Symbol (Ctrl+F8),
// instance properties and Filters panel (Flash 8).

const $ = (q, el = document) => el.querySelector(q);
const $$ = (q, el = document) => [...el.querySelectorAll(q)];
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const clamp = (v, a, b) => Math.min(b, Math.max(a, isNaN(v) ? a : v));

export const KIND_LABEL = { movieclip: 'Clip de película', button: 'Botón', graphic: 'Gráfico', bitmap: 'Mapa de bits', sound: 'Sonido' };
const SYMBOL_KINDS = { movieclip: 'Clip de película', button: 'Botón', graphic: 'Gráfico' };
export const KIND_ICON = {
  movieclip: '<svg viewBox="0 0 20 20"><rect x="3" y="4" width="14" height="12" rx="1"/><path d="M3 7h14M3 13h14M6 4v3M10 4v3M14 4v3M6 13v3M10 13v3M14 13v3"/></svg>',
  button: '<svg viewBox="0 0 20 20"><rect x="2.5" y="6" width="15" height="8" rx="3"/><path d="M11 9l4 5-2 .2-1 2z" class="f"/></svg>',
  graphic: '<svg viewBox="0 0 20 20"><circle cx="7" cy="8" r="4"/><path d="M10 16l3.5-7 3.5 7z"/></svg>',
  sound: '<svg viewBox="0 0 20 20"><path d="M3 8h3l4-3.5v11L6 12H3z"/><path d="M13 7.5a3.5 3.5 0 010 5M15 5a7 7 0 010 10"/></svg>',
  bitmap: '<svg viewBox="0 0 20 20"><rect x="2.5" y="4" width="15" height="12"/><path d="M2.5 14l4.5-4.5 3.5 3.5 2.5-2.5 4.5 4.5"/><circle class="f" cx="13" cy="7.5" r="1.3"/></svg>',
};
const FOLDER_ICON = (open) => `<svg viewBox="0 0 20 20"><path d="${open ? 'M2 6h6l2 2h8v2H5l-3 6z M2 16l3-6h13l-3 6z' : 'M2 6h6l2 2h8v8H2z'}"/></svg>`;

const hexToRgba = (hex, a = 255) => {
  const n = parseInt(hex.slice(1), 16);
  return { r: n >> 16, g: (n >> 8) & 255, b: n & 255, a };
};
const rgbaToHex = (c) => '#' + [c.r, c.g, c.b].map((v) => v.toString(16).padStart(2, '0')).join('');

const FILTER_NAMES = { dropshadow: 'Sombra', blur: 'Desenfocar', glow: 'Iluminado', bevel: 'Bisel', adjustcolor: 'Ajustar color' };
const QUALITY = [[1, 'Baja'], [2, 'Media'], [3, 'Alta']];

export function initSymbols(app) {
  const { ed } = app;
  let S = null;
  let libSel = null; // id of the symbol selected in the Library
  let filterSel = 0;
  let previewFrame = 0, previewTimer = 0;

  // ───────── Library ─────────

  function buildLibrary(host) {
    host.innerHTML = `
      <div class="lib-count muted" id="lib-count"></div>
      <div class="lib-preview"><canvas id="lib-prev" width="236" height="96"></canvas>
        <button class="ib" id="lib-play" title="Reproducir" hidden><svg viewBox="0 0 20 20"><path d="M6 4l10 6-10 6z" class="f"/></svg></button></div>
      <div class="lib-list" id="lib-list"></div>
      <div class="lib-actions">
        <button class="ib" id="lib-newsym" title="Nuevo símbolo (Ctrl+F8)"><svg viewBox="0 0 20 20"><path d="M10 4v12M4 10h12"/></svg></button>
        <button class="ib" id="lib-newfolder" title="Nueva carpeta"><svg viewBox="0 0 20 20"><path d="M2 6h6l2 2h8v8H2z"/><path d="M13 10v4M11 12h4"/></svg></button>
        <button class="ib" id="lib-props" title="Propiedades"><svg viewBox="0 0 20 20"><circle cx="10" cy="10" r="7"/><path d="M10 9v5M10 6v.5"/></svg></button>
        <div class="grow"></div>
        <button class="ib" id="lib-delete" title="Eliminar"><svg viewBox="0 0 20 20"><path d="M5 6h10l-1 11H6zM3.5 6h13M8 6V4h4v2"/></svg></button>
      </div>`;
    $('#lib-newsym', host).addEventListener('click', () => newSymbolDialog());
    $('#lib-newfolder', host).addEventListener('click', () => app.cmd(() => ed.lib_new_folder('')));
    $('#lib-props', host).addEventListener('click', () => libSel && symbolPropsDialog(libSel));
    $('#lib-delete', host).addEventListener('click', () => deleteSelected());
    $('#lib-play', host).addEventListener('click', () => togglePreviewPlay());
    const list = $('#lib-list', host);
    list.addEventListener('click', (e) => {
      const f = e.target.closest('[data-folder]');
      if (f && e.target.closest('.fold')) return app.cmd(() => ed.lib_toggle_folder(+f.dataset.folder));
      const r = e.target.closest('[data-sym]');
      selectItem(r ? +r.dataset.sym : null);
    });
    list.addEventListener('dblclick', (e) => {
      const f = e.target.closest('[data-folder]');
      if (f) return renameInline(f, (v) => ed.lib_rename_folder(+f.dataset.folder, v));
      const r = e.target.closest('[data-sym]');
      if (!r) return;
      // Double-click on the name: rename; on the icon: edit (like Flash).
      const sym = S.library.find((s) => s.id === +r.dataset.sym);
      if (e.target.closest('.lname')) renameInline(r, (v) => ed.lib_rename(+r.dataset.sym, v));
      else if (sym && sym.kind === 'bitmap') bitmapPropsDialog(sym.id);
      else if (sym && sym.kind === 'sound') soundPropsDialog(sym.id);
      else app.cmd(() => ed.edit_symbol(+r.dataset.sym));
    });
    list.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      const r = e.target.closest('[data-sym]');
      const f = e.target.closest('[data-folder]');
      if (r) {
        const id = +r.dataset.sym;
        selectItem(id);
        const sym = S.library.find((s) => s.id === id);
        if (sym.kind === 'sound') {
          return app.contextMenu(e.clientX, e.clientY, [
            { label: 'Reproducir', action: () => app.audio.play(id) },
            { label: 'Detener', action: () => app.audio.stop(id) },
            '-',
            { label: 'Cambiar nombre', action: () => renameInline(r, (v) => ed.lib_rename(id, v)) },
            { label: 'Duplicar', action: () => app.cmd(() => { libSel = ed.lib_duplicate(id); }) },
            { label: 'Eliminar', action: () => deleteSelected() },
            '-',
            { label: 'Propiedades…', action: () => soundPropsDialog(id) },
          ]);
        }
        if (sym.kind === 'bitmap') {
          return app.contextMenu(e.clientX, e.clientY, [
            { label: 'Cambiar nombre', action: () => renameInline(r, (v) => ed.lib_rename(id, v)) },
            { label: 'Eliminar', action: () => deleteSelected() },
            { label: 'Mover a', sub: [{ label: '(Raíz de la biblioteca)', check: sym.folder == null, action: () => app.cmd(() => ed.lib_move(id, -1)) }]
              .concat(S.libFolders.map((fo) => ({ label: esc(fo.name), check: sym.folder === fo.id, action: () => app.cmd(() => ed.lib_move(id, fo.id)) }))) },
            '-',
            { label: 'Propiedades…', action: () => bitmapPropsDialog(id) },
          ]);
        }
        app.contextMenu(e.clientX, e.clientY, [
          { label: 'Editar', action: () => app.cmd(() => ed.edit_symbol(id)) },
          { label: 'Cambiar nombre', action: () => renameInline(r, (v) => ed.lib_rename(id, v)) },
          { label: 'Duplicar…', action: () => app.cmd(() => { libSel = ed.lib_duplicate(id); }) },
          { label: 'Eliminar', action: () => deleteSelected() },
          '-',
          { label: 'Tipo', sub: Object.entries(SYMBOL_KINDS).map(([k, l]) => ({ label: l, check: sym.kind === k, action: () => app.cmd(() => ed.lib_set_kind(id, k)) })) },
          { label: 'Mover a', sub: [{ label: '(Raíz de la biblioteca)', check: sym.folder == null, action: () => app.cmd(() => ed.lib_move(id, -1)) }]
            .concat(S.libFolders.map((fo) => ({ label: esc(fo.name), check: sym.folder === fo.id, action: () => app.cmd(() => ed.lib_move(id, fo.id)) }))) },
          { label: 'Activar guías para escala en 9 divisiones', check: sym.scale9, disabled: sym.kind !== 'movieclip', action: () => app.cmd(() => ed.set_scale9(id, !sym.scale9)) },
          { label: 'Vinculación…', action: () => linkageDialog(id) },
          '-',
          { label: 'Propiedades…', action: () => symbolPropsDialog(id) },
        ]);
      } else if (f) {
        const id = +f.dataset.folder;
        app.contextMenu(e.clientX, e.clientY, [
          { label: 'Cambiar nombre', action: () => renameInline(f, (v) => ed.lib_rename_folder(id, v)) },
          { label: 'Eliminar carpeta', action: () => app.cmd(() => ed.lib_delete_folder(id)) },
        ]);
      } else {
        app.contextMenu(e.clientX, e.clientY, [
          { label: 'Nuevo símbolo…', key: 'Ctrl+F8', action: () => newSymbolDialog() },
          { label: 'Nueva carpeta', action: () => app.cmd(() => ed.lib_new_folder('')) },
        ]);
      }
    });
    bindDrag(list);
    new ResizeObserver(() => drawPreview()).observe($('.lib-preview', host));
  }

  function selectItem(id) {
    if (libSel === id) return;
    libSel = id;
    previewFrame = 0;
    stopPreview();
    // Without repainting the list: this way the double-click reaches the same row.
    $$('#lib-list [data-sym]').forEach((r) => r.classList.toggle('sel', +r.dataset.sym === id));
    lastSig = '';
    drawPreview();
  }

  function deleteSelected() {
    if (!libSel) return;
    const sym = S.library.find((s) => s.id === libSel);
    if (!sym) return;
    const go = () => { app.cmd(() => ed.lib_delete(sym.id)); libSel = null; drawPreview(); };
    if (!sym.uses) return go();
    app.modal('Eliminar', `<p style="max-width:340px">«${esc(sym.name)}» tiene ${sym.uses} instancia${sym.uses === 1 ? '' : 's'}. Al eliminar el símbolo también se eliminan sus instancias.</p>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="ld-ok">Eliminar</button></div>`, (b) => {
      $('#ld-ok', b).addEventListener('click', () => { app.closeModal(); go(); });
    });
  }

  function renameInline(row, apply) {
    const name = row.querySelector('.lname');
    const old = name.textContent;
    name.innerHTML = `<input type="text" value="${esc(old)}">`;
    const inp = name.querySelector('input');
    inp.focus();
    inp.select();
    let done = false;
    const finish = (ok) => {
      if (done) return;
      done = true;
      const v = inp.value;
      name.textContent = old;
      if (ok && v.trim() && v !== old) apply(v);
      lastSig = '';
      app.refresh();
    };
    inp.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Enter') finish(true);
      if (e.key === 'Escape') finish(false);
    });
    inp.addEventListener('blur', () => finish(true));
    inp.addEventListener('pointerdown', (e) => e.stopPropagation());
  }

  // Drag a symbol onto the Stage (creates an instance) or into a folder.
  function bindDrag(list) {
    let drag = null;
    list.addEventListener('pointerdown', (e) => {
      const r = e.target.closest('[data-sym]');
      if (!r || e.button !== 0 || e.target.closest('input')) return;
      drag = { id: +r.dataset.sym, x: e.clientX, y: e.clientY, ghost: null };
    });
    document.addEventListener('pointermove', (e) => {
      if (!drag) return;
      if (!drag.ghost && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) > 5) {
        const sym = S.library.find((s) => s.id === drag.id);
        drag.ghost = document.createElement('div');
        drag.ghost.className = 'lib-ghost';
        drag.ghost.innerHTML = `${KIND_ICON[sym.kind]}<span>${esc(sym.name)}</span>`;
        document.body.appendChild(drag.ghost);
      }
      if (drag.ghost) {
        drag.ghost.style.left = e.clientX + 10 + 'px';
        drag.ghost.style.top = e.clientY + 6 + 'px';
        $$('.lib-row.drop', list).forEach((x) => x.classList.remove('drop'));
        const over = document.elementFromPoint(e.clientX, e.clientY);
        over?.closest?.('[data-folder]')?.classList.add('drop');
      }
    });
    document.addEventListener('pointerup', (e) => {
      if (!drag) return;
      const d = drag;
      drag = null;
      if (!d.ghost) return;
      d.ghost.remove();
      $$('.lib-row.drop', list).forEach((x) => x.classList.remove('drop'));
      const over = document.elementFromPoint(e.clientX, e.clientY);
      const canvas = $('#stage');
      if (over === canvas) {
        const r = canvas.getBoundingClientRect();
        const dpr = window.devicePixelRatio || 1;
        app.cmd(() => ed.place_symbol(d.id, (e.clientX - r.left) * dpr, (e.clientY - r.top) * dpr));
      } else if (over?.closest?.('[data-folder]')) {
        app.cmd(() => ed.lib_move(d.id, +over.closest('[data-folder]').dataset.folder));
      } else if (over?.closest?.('#lib-list')) {
        app.cmd(() => ed.lib_move(d.id, -1));
      }
    });
  }

  let lastSig = '';
  function renderList(force = false) {
    const list = $('#lib-list');
    if (!list || list.querySelector('input')) return;
    const sig = JSON.stringify([S.library, S.libFolders, libSel]);
    if (!force && sig === lastSig) return;
    lastSig = sig;
    const byName = (a, b) => a.name.localeCompare(b.name, 'es', { numeric: true });
    const item = (s, depth) => `<div class="lib-row${s.id === libSel ? ' sel' : ''}" data-sym="${s.id}" style="padding-left:${6 + depth * 14}px" title="${KIND_LABEL[s.kind]} · ${s.uses} instancia${s.uses === 1 ? '' : 's'}">
      <span class="licon">${KIND_ICON[s.kind]}</span><span class="lname">${esc(s.name)}</span><span class="lkind muted">${KIND_LABEL[s.kind]}</span></div>`;
    let html = '';
    for (const f of [...S.libFolders].sort(byName)) {
      html += `<div class="lib-row folder" data-folder="${f.id}"><span class="fold">${f.open ? '▾' : '▸'}</span><span class="licon">${FOLDER_ICON(f.open)}</span><span class="lname">${esc(f.name)}</span></div>`;
      if (f.open) html += S.library.filter((s) => s.folder === f.id).sort(byName).map((s) => item(s, 1)).join('');
    }
    html += S.library.filter((s) => s.folder == null || !S.libFolders.some((f) => f.id === s.folder)).sort(byName).map((s) => item(s, 0)).join('');
    list.innerHTML = html || '<div class="muted lib-empty">La biblioteca está vacía. Selecciona algo en el escenario y pulsa F8.</div>';
    const n = S.library.length;
    $('#lib-count').textContent = `${n} elemento${n === 1 ? '' : 's'}`;
  }

  function drawPreview() {
    const c = $('#lib-prev');
    if (!c || !S) return;
    const w = Math.max(1, c.parentElement.clientWidth - 2), h = 96;
    if (c.width !== w) c.width = w;
    const g = c.getContext('2d');
    const sym = libSel && S.library.find((s) => s.id === libSel);
    g.fillStyle = '#fff';
    g.fillRect(0, 0, c.width, c.height);
    const frames = sym ? ed.symbol_frames(sym.id) : 1;
    $('#lib-play').hidden = !sym || (frames < 2 && sym.kind !== 'sound');
    if (!sym) return;
    if (sym.kind === 'sound') {
      // Sound waveform.
      const peaks = ed.sound_peaks(sym.id, c.width);
      g.fillStyle = '#2f6fd1';
      for (let x = 0; x < peaks.length; x++) {
        const hgt = Math.max(1, peaks[x] * (h - 8));
        g.fillRect(x, (h - hgt) / 2, 1, hgt);
      }
      return;
    }
    const px = ed.render_symbol_thumb(sym.id, previewFrame % frames, c.width, h);
    if (px.length === c.width * h * 4) g.putImageData(new ImageData(new Uint8ClampedArray(px), c.width, h), 0, 0);
  }

  function togglePreviewPlay() {
    const sym = libSel && S.library.find((s) => s.id === libSel);
    if (sym && sym.kind === 'sound') {
      if (app.audio.isPlaying(sym.id)) app.audio.stop(sym.id);
      else app.audio.play(sym.id);
      return;
    }
    if (previewTimer) return stopPreview();
    const btn = $('#lib-play');
    btn.classList.add('on');
    previewTimer = setInterval(() => {
      previewFrame++;
      drawPreview();
    }, 1000 / (S.fps || 12));
  }
  function stopPreview() {
    clearInterval(previewTimer);
    previewTimer = 0;
    $('#lib-play')?.classList.remove('on');
  }

  // ───────── Dialogs ─────────

  function nextName() {
    let n = S.library.length + 1;
    while (S.library.some((s) => s.name === `Símbolo ${n}`)) n++;
    return `Símbolo ${n}`;
  }

  const kindRadios = (name, cur) => Object.entries(SYMBOL_KINDS).map(([k, l]) =>
    `<label class="chk"><input type="radio" name="${name}" value="${k}" ${k === cur ? 'checked' : ''}> ${l}</label>`).join('');

  function convertDialog() {
    if (!S.sel) return app.toast('Selecciona algo en el escenario para convertirlo en símbolo');
    const last = app.loadPref('omf.f8', { kind: 'movieclip', reg: 0 });
    app.modal('Convertir en símbolo', `
      <div class="prow"><span class="lab" style="width:80px">Nombre</span><input type="text" id="cs-name" value="${esc(nextName())}" style="width:200px"></div>
      <div class="prow"><span class="lab" style="width:80px">Tipo</span><div style="display:grid;gap:4px">${kindRadios('cs-kind', last.kind)}</div>
        <span class="lab" style="width:70px;text-align:right">Registro</span><div class="reg-grid" id="cs-reg">${[...Array(9)].map((_, i) => `<i data-reg="${i}" class="${i === last.reg ? 'on' : ''}"></i>`).join('')}</div></div>
      <div class="prow"><span class="lab" style="width:80px"></span><label class="chk"><input type="checkbox" id="cs-9"> Activar guías para escala en 9 divisiones</label></div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="cs-ok">Aceptar</button></div>`, (b) => {
      let reg = last.reg;
      const inp = $('#cs-name', b);
      inp.focus();
      inp.select();
      $('#cs-reg', b).addEventListener('click', (e) => {
        const c = e.target.closest('[data-reg]');
        if (!c) return;
        reg = +c.dataset.reg;
        $$('[data-reg]', b).forEach((x) => x.classList.toggle('on', x === c));
      });
      const ok = () => {
        const kind = $('input[name=cs-kind]:checked', b).value;
        app.savePref('omf.f8', { kind, reg });
        const name = inp.value;
        const s9 = $('#cs-9', b).checked;
        app.closeModal();
        app.cmd(() => { libSel = ed.convert_to_symbol(name, kind, reg, s9) || libSel; });
        drawPreview();
      };
      $('#cs-ok', b).addEventListener('click', ok);
      inp.addEventListener('keydown', (e) => e.key === 'Enter' && ok());
    });
  }

  function newSymbolDialog() {
    app.modal('Crear un nuevo símbolo', `
      <div class="prow"><span class="lab" style="width:80px">Nombre</span><input type="text" id="ns-name" value="${esc(nextName())}" style="width:200px"></div>
      <div class="prow"><span class="lab" style="width:80px">Tipo</span><div style="display:grid;gap:4px">${kindRadios('ns-kind', 'movieclip')}</div></div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="ns-ok">Aceptar</button></div>`, (b) => {
      const inp = $('#ns-name', b);
      inp.focus();
      inp.select();
      const ok = () => {
        const kind = $('input[name=ns-kind]:checked', b).value;
        const name = inp.value;
        app.closeModal();
        app.cmd(() => { libSel = ed.new_symbol(name, kind); });
      };
      $('#ns-ok', b).addEventListener('click', ok);
      inp.addEventListener('keydown', (e) => e.key === 'Enter' && ok());
    });
  }

  const kb = (n) => (n >= 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${(n / 1024).toFixed(1)} KB`);

  function soundPropsDialog(id) {
    const sym = S.library.find((s) => s.id === id);
    if (!sym) return;
    const info = JSON.parse(ed.sound_info(id)) || {};
    const orig = info.adpcm || 0;
    app.modal('Propiedades de sonido', `
      <div class="prow"><span class="lab" style="width:90px">Nombre</span><input type="text" id="sd-name" value="${esc(sym.name)}" style="width:220px"></div>
      <div class="prow"><span class="lab" style="width:90px">Formato</span><span>${(info.rate / 1000).toFixed(1)} kHz · 16 bits · ${info.stereo ? 'estéreo' : 'mono'} · ${info.seconds?.toFixed(2)} s</span></div>
      <div class="prow"><span class="lab" style="width:90px">Compresión</span><select id="sd-comp"><option value="raw">Sin formato</option><option value="adpcm">ADPCM</option></select></div>
      <div class="prow" id="sd-bits-row"><span class="lab" style="width:90px">Bits ADPCM</span><select id="sd-bits"><option value="2">2 bits</option><option value="3">3 bits</option><option value="4">4 bits</option><option value="5">5 bits</option></select></div>
      <p class="muted" id="sd-size"></p>
      <div class="modal-actions"><button class="btn" id="sd-play">Probar</button><button class="btn" id="sd-stop">Detener</button><div class="grow"></div><button class="btn" data-close>Cancelar</button><button class="btn primary" id="sd-ok">Aceptar</button></div>`, (b) => {
      const comp = $('#sd-comp', b), bits = $('#sd-bits', b);
      comp.value = orig ? 'adpcm' : 'raw';
      bits.value = String(orig || 4);
      const value = () => (comp.value === 'adpcm' ? +bits.value : 0);
      const show = () => {
        $('#sd-bits-row', b).hidden = comp.value !== 'adpcm';
        const raw = info.raw || 0;
        const n = value() ? Math.floor(raw / 16) * value() + 1 : raw;
        $('#sd-size', b).textContent = `${kb(raw)} sin comprimir → ${kb(n)} en la película (${Math.round((n / Math.max(1, raw)) * 100)} %)`;
      };
      comp.addEventListener('change', show);
      bits.addEventListener('change', show);
      show();
      // Test: plays with the chosen compression.
      $('#sd-play', b).addEventListener('click', () => { app.audio.stop(id); app.audio.play(id, { bits: value() }); });
      $('#sd-stop', b).addEventListener('click', () => app.audio.stop(id));
      $$('[data-close]', b).forEach((x) => x.addEventListener('click', () => app.audio.stop(id)));
      $('#sd-ok', b).addEventListener('click', () => {
        const name = $('#sd-name', b).value;
        app.audio.stop(id);
        app.closeModal();
        app.cmd(() => {
          if (name !== sym.name) ed.lib_rename(id, name);
          if (value() !== orig) ed.set_sound_compression(id, value());
        });
        app.audio.forget(id);
      });
    });
  }

  /** JPEG of the image at the given quality (the browser encodes it). */
  async function encodeJpeg(id, quality) {
    const png = ed.bitmap_png(id);
    const img = await createImageBitmap(new Blob([png], { type: 'image/png' }));
    const c = document.createElement('canvas');
    c.width = img.width;
    c.height = img.height;
    c.getContext('2d').drawImage(img, 0, 0);
    const blob = await new Promise((res) => c.toBlob(res, 'image/jpeg', quality / 100));
    return new Uint8Array(await blob.arrayBuffer());
  }

  function bitmapPropsDialog(id) {
    const sym = S.library.find((s) => s.id === id);
    if (!sym) return;
    app.modal('Propiedades de mapa de bits', `
      <div class="prow"><span class="lab" style="width:90px">Nombre</span><input type="text" id="bp-name" value="${esc(sym.name)}" style="width:220px"></div>
      <div class="prow"><span class="lab" style="width:90px">Tamaño</span><span>${sym.size ? `${sym.size[0]} × ${sym.size[1]} píxeles` : ''}</span></div>
      <div class="prow"><span class="lab" style="width:90px"></span><label class="chk"><input type="checkbox" id="bp-smooth" ${sym.smooth ? 'checked' : ''}> Permitir suavizado</label></div>
      <div class="prow"><span class="lab" style="width:90px">Compresión</span><select id="bp-comp"><option value="lossless">Sin pérdida (PNG/GIF)</option><option value="jpeg">Foto (JPEG)</option></select></div>
      <div class="prow" id="bp-q-row"><span class="lab" style="width:90px">Calidad</span><input type="number" id="bp-q" min="1" max="100" style="width:56px"> (100 = la más alta)<button class="btn small" id="bp-test">Probar</button></div>
      <p class="muted" id="bp-size"></p>
      <p class="muted">${sym.uses} instancia${sym.uses === 1 ? '' : 's'} en la película.</p>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="bp-ok">Aceptar</button></div>`, (b) => {
      const comp = $('#bp-comp', b), q = $('#bp-q', b);
      comp.value = sym.quality ? 'jpeg' : 'lossless';
      q.value = sym.quality || 80;
      let jpeg = null;
      const show = () => {
        if (!$('#bp-q-row', b)) return; // the dialog was already closed
        $('#bp-q-row', b).hidden = comp.value !== 'jpeg';
        const png = sym.pngBytes || 0;
        const n = comp.value === 'jpeg' ? (jpeg ? jpeg.length : sym.jpegBytes) : null;
        $('#bp-size', b).textContent = comp.value === 'jpeg' && n
          ? `Calidad ${q.value}: ${kb(png)} sin pérdida → ${kb(n)} comprimido (${Math.round((n / Math.max(1, png)) * 100)} %)`
          : `Sin pérdida: ${kb(png)}`;
      };
      const test = async () => {
        jpeg = await encodeJpeg(id, Math.max(1, Math.min(100, +q.value || 80)));
        show();
      };
      comp.addEventListener('change', () => { jpeg = null; if (comp.value === 'jpeg') test(); show(); });
      q.addEventListener('change', test);
      $('#bp-test', b).addEventListener('click', test);
      show();
      $('#bp-ok', b).addEventListener('click', async () => {
        const name = $('#bp-name', b).value, smooth = $('#bp-smooth', b).checked;
        const quality = Math.max(1, Math.min(100, +q.value || 80));
        let bytes = null;
        if (comp.value === 'jpeg' && (quality !== sym.quality || !sym.quality)) bytes = jpeg && +q.value === quality ? jpeg : await encodeJpeg(id, quality);
        app.closeModal();
        app.cmd(() => {
          if (name !== sym.name) ed.lib_rename(id, name);
          if (smooth !== sym.smooth) ed.set_bitmap_smooth(id, smooth);
          if (comp.value === 'lossless' && sym.quality) ed.set_bitmap_jpeg(id, new Uint8Array(0), 0);
          if (bytes) ed.set_bitmap_jpeg(id, bytes, quality);
        });
      });
    });
  }

  /** Linkage (Export for ActionScript): identifier and AS 2.0 class. */
  function linkageDialog(id) {
    const sym = S.library.find((s) => s.id === id);
    if (!sym) return;
    const l = sym.linkage;
    app.modal('Propiedades de vinculación', `
      <div class="prow"><span class="lab" style="width:110px">Identificador</span><input type="text" id="lk-id" value="${esc(l ? l.id : sym.name)}" style="width:200px" ${l ? '' : 'disabled'}></div>
      <div class="prow"><span class="lab" style="width:110px">Clase AS 2.0</span><input type="text" id="lk-class" value="${esc(l ? l.class || '' : '')}" style="width:200px" placeholder="p. ej. juego.Pelota" ${l ? '' : 'disabled'}></div>
      <div class="prow"><span class="lab" style="width:110px">Vinculación</span><div style="display:grid;gap:4px">
        <label class="chk"><input type="checkbox" id="lk-export" ${l ? 'checked' : ''}> Exportar para ActionScript</label>
        <label class="chk"><input type="checkbox" id="lk-first" ${l ? 'checked' : ''} disabled> Exportar en primer fotograma</label></div></div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="lk-ok">Aceptar</button></div>`, (b) => {
      const exp = $('#lk-export', b);
      exp.addEventListener('change', () => {
        $('#lk-id', b).disabled = $('#lk-class', b).disabled = !exp.checked;
        $('#lk-first', b).checked = exp.checked;
      });
      $('#lk-ok', b).addEventListener('click', () => {
        const json = exp.checked ? JSON.stringify({ id: $('#lk-id', b).value, class: $('#lk-class', b).value }) : 'null';
        app.closeModal();
        app.cmd(() => ed.set_linkage(id, json));
      });
    });
  }

  function symbolPropsDialog(id) {
    const sym = S.library.find((s) => s.id === id);
    if (!sym) return;
    if (sym.kind === 'bitmap') return bitmapPropsDialog(id);
    app.modal('Propiedades de símbolo', `
      <div class="prow"><span class="lab" style="width:80px">Nombre</span><input type="text" id="sp-name" value="${esc(sym.name)}" style="width:200px"></div>
      <div class="prow"><span class="lab" style="width:80px">Tipo</span><div style="display:grid;gap:4px">${kindRadios('sp-kind', sym.kind)}</div></div>
      <div class="prow"><span class="lab" style="width:80px"></span><label class="chk"><input type="checkbox" id="sp-9" ${sym.scale9 ? 'checked' : ''}> Activar guías para escala en 9 divisiones</label></div>
      <div class="prow"><span class="lab" style="width:80px">Vinculación</span><span class="muted">${sym.linkage ? esc(sym.linkage.id + (sym.linkage.class ? ` (${sym.linkage.class})` : '')) : 'No se exporta'}</span><button class="btn small" id="sp-link">Vinculación…</button></div>
      <p class="muted">${sym.uses} instancia${sym.uses === 1 ? '' : 's'} en la película.</p>
      <div class="modal-actions"><button class="btn" id="sp-edit">Editar</button><div class="grow"></div><button class="btn" data-close>Cancelar</button><button class="btn primary" id="sp-ok">Aceptar</button></div>`, (b) => {
      $('#sp-edit', b).addEventListener('click', () => { app.closeModal(); app.cmd(() => ed.edit_symbol(id)); });
      $('#sp-link', b).addEventListener('click', () => { app.closeModal(); linkageDialog(id); });
      $('#sp-ok', b).addEventListener('click', () => {
        const name = $('#sp-name', b).value, kind = $('input[name=sp-kind]:checked', b).value, s9 = $('#sp-9', b).checked;
        app.closeModal();
        app.cmd(() => {
          if (name !== sym.name) ed.lib_rename(id, name);
          if (kind !== sym.kind) ed.lib_set_kind(id, kind);
          if (s9 !== sym.scale9) ed.set_scale9(id, s9);
        });
      });
    });
  }

  function swapDialog() {
    const inst = S.inst;
    if (!inst) return;
    let pick = inst.symbol;
    app.modal('Intercambiar símbolo', `
      <div class="swap-list" id="sw-list">${S.library.filter((s) => (s.kind === 'bitmap') === (inst.symbolKind === 'bitmap')).map((s) => `<div class="lib-row${s.id === pick ? ' sel' : ''}" data-sym="${s.id}"><span class="licon">${KIND_ICON[s.kind]}</span><span class="lname">${esc(s.name)}</span></div>`).join('')}</div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="sw-ok">Aceptar</button></div>`, (b) => {
      $('#sw-list', b).addEventListener('click', (e) => {
        const r = e.target.closest('[data-sym]');
        if (!r) return;
        pick = +r.dataset.sym;
        $$('[data-sym]', b).forEach((x) => x.classList.toggle('sel', x === r));
      });
      $('#sw-list', b).addEventListener('dblclick', () => $('#sw-ok', b).click());
      $('#sw-ok', b).addEventListener('click', () => { app.closeModal(); app.cmd(() => ed.swap_symbol(pick)); });
    });
  }

  // ───────── Instance properties ─────────

  const COLOR_MODES = [['none', 'Ninguno'], ['brightness', 'Brillo'], ['tint', 'Tinta'], ['alpha', 'Alfa'], ['advanced', 'Avanzado']];
  const BLENDS = [['normal', 'Normal'], ['layer', 'Capa'], ['darken', 'Oscurecer'], ['multiply', 'Multiplicar'], ['lighten', 'Aclarar'], ['screen', 'Trama'],
    ['overlay', 'Superponer'], ['hardlight', 'Luz fuerte'], ['add', 'Añadir'], ['subtract', 'Restar'], ['difference', 'Diferencia'], ['invert', 'Invertir'], ['alpha', 'Alfa'], ['erase', 'Borrar']];
  const opts = (list) => list.map(([v, l]) => `<option value="${v}">${l}</option>`).join('');

  function instanceSig() {
    const i = S.inst;
    return i ? `inst:${i.kind}:${i.color.mode}:${i.symbolKind}` : '';
  }

  function instanceHTML() {
    const i = S.inst;
    if (!i) return '';
    const fx = i.color.mode;
    return `
      <div class="ptitle">${i.kind === 'bitmap' ? 'Mapa de bits' : 'Instancia'}</div>
      ${i.kind === 'bitmap' ? '' : `<div class="prow"><select id="pi-kind">${opts(Object.entries(SYMBOL_KINDS))}</select></div>`}
      <div class="prow"><span class="lab">De</span><span class="inst-of" id="pi-of"></span><button class="btn small" id="pi-swap" title="Intercambiar símbolo">Intercambiar…</button></div>
      ${i.kind === 'bitmap' ? '' : i.kind !== 'graphic' ? '<div class="prow"><span class="lab">Nombre</span><input type="text" id="pi-name" placeholder="<Nombre de instancia>" style="width:140px"></div>' : ''}
      ${i.kind === 'graphic' ? `<div class="prow"><span class="lab">Reproducir</span><select id="pi-loop"><option value="loop">Reproducir indefinidamente</option><option value="once">Reproducir una vez</option><option value="single">Fotograma único</option></select></div>
        <div class="prow"><span class="lab">Primero</span><input type="number" id="pi-first" min="1" step="1"></div>` : ''}
      ${i.kind === 'bitmap' ? '' : `<div class="prow"><span class="lab">Color</span><select id="pi-color">${opts(COLOR_MODES)}</select>
        ${fx === 'brightness' || fx === 'alpha' ? '<input type="number" id="pi-fxv" style="width:52px">%' : ''}
        ${fx === 'tint' ? '<div class="chip" id="pi-tint"><i></i></div><input type="number" id="pi-amount" min="0" max="100" style="width:48px">%' : ''}
        ${fx === 'advanced' ? '<button class="btn small" id="pi-adv">Configuración…</button>' : ''}</div>`}
      ${fx === 'brightness' || fx === 'alpha' ? `<div class="prow"><span class="lab"></span><input type="range" id="pi-fxr" min="${fx === 'alpha' ? 0 : -100}" max="100" style="width:150px"></div>` : ''}
      ${i.kind === 'movieclip' ? `<div class="prow"><span class="lab">Mezcla</span><select id="pi-blend">${opts(BLENDS)}</select></div>
        <div class="prow"><span class="lab"></span><label class="chk"><input type="checkbox" id="pi-cache"> Usar caché de mapa de bits en tiempo de ejecución</label></div>` : ''}`;
  }

  function colorJSON(mode, i) {
    const c = i.color;
    switch (mode) {
      case 'brightness': return { mode, v: c.mode === 'brightness' ? c.v : 0 };
      case 'tint': return { mode, color: c.mode === 'tint' ? c.color : { r: 255, g: 255, b: 255, a: 255 }, amount: c.mode === 'tint' ? c.amount : 50 };
      case 'alpha': return { mode, v: c.mode === 'alpha' ? c.v : 100 };
      case 'advanced': return { mode, m: c.mode === 'advanced' ? c.m : [100, 0, 100, 0, 100, 0, 100, 0] };
      default: return { mode: 'none' };
    }
  }

  function bindInstance(host) {
    const i = () => S.inst;
    const setFx = (patch) => app.cmd(() => ed.set_color_fx(JSON.stringify({ ...colorJSON(i().color.mode, i()), ...patch })));
    $('#pi-kind', host)?.addEventListener('change', (e) => app.cmd(() => ed.set_inst_kind(e.target.value)));
    $('#pi-swap', host)?.addEventListener('click', () => swapDialog());
    $('#pi-name', host)?.addEventListener('change', (e) => app.cmd(() => ed.set_inst_name(e.target.value)));
    const loop = () => app.cmd(() => ed.set_loop($('#pi-loop', host).value, Math.max(0, (+$('#pi-first', host).value || 1) - 1)));
    $('#pi-loop', host)?.addEventListener('change', loop);
    $('#pi-first', host)?.addEventListener('change', loop);
    $('#pi-color', host)?.addEventListener('change', (e) => app.cmd(() => ed.set_color_fx(JSON.stringify(colorJSON(e.target.value, i())))));
    $('#pi-fxv', host)?.addEventListener('change', (e) => setFx({ v: clamp(+e.target.value, i().color.mode === 'alpha' ? 0 : -100, 100) }));
    $('#pi-fxr', host)?.addEventListener('input', (e) => setFx({ v: +e.target.value }));
    $('#pi-amount', host)?.addEventListener('change', (e) => setFx({ amount: clamp(+e.target.value, 0, 100) }));
    $('#pi-tint', host)?.addEventListener('click', (e) => app.openColorPopover(e.currentTarget, { hex: rgbaToHex(i().color.color), allowNone: false, noAlpha: true },
      (hex) => setFx({ color: hexToRgba(hex) })));
    $('#pi-adv', host)?.addEventListener('click', () => advancedDialog());
    $('#pi-blend', host)?.addEventListener('change', (e) => app.cmd(() => ed.set_blend(e.target.value)));
    $('#pi-cache', host)?.addEventListener('change', (e) => app.cmd(() => ed.set_cache(e.target.checked)));
  }

  function updateInstance(host) {
    const i = S.inst;
    if (!i) return;
    const set = app.setVal;
    set($('#pi-kind', host), i.kind);
    $('#pi-of', host).textContent = i.symbolName;
    set($('#pi-name', host), i.name || '');
    set($('#pi-loop', host), i.loop);
    set($('#pi-first', host), i.first + 1);
    set($('#pi-color', host), i.color.mode);
    if (i.color.mode === 'brightness' || i.color.mode === 'alpha') {
      set($('#pi-fxv', host), i.color.v);
      set($('#pi-fxr', host), i.color.v);
    }
    if (i.color.mode === 'tint') {
      app.paintChip($('#pi-tint', host), rgbaToHex(i.color.color));
      set($('#pi-amount', host), i.color.amount);
    }
    set($('#pi-blend', host), i.blend);
    set($('#pi-cache', host), i.cache);
  }

  function advancedDialog() {
    const m = colorJSON('advanced', S.inst).m;
    const ch = ['Rojo', 'Verde', 'Azul', 'Alfa'];
    app.modal('Configuración avanzada de efecto', `
      <div class="adv-grid">${ch.map((c, k) => `<span>${c} = (</span><input type="number" data-adv="${2 * k}" value="${m[2 * k]}" min="-100" max="100">
        <span>% × ${c[0]}) + </span><input type="number" data-adv="${2 * k + 1}" value="${m[2 * k + 1]}" min="-255" max="255">`).join('')}</div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="adv-ok">Aceptar</button></div>`, (b) => {
      $('#adv-ok', b).addEventListener('click', () => {
        const v = $$('[data-adv]', b).map((x) => +x.value || 0);
        app.closeModal();
        app.cmd(() => ed.set_color_fx(JSON.stringify({ mode: 'advanced', m: v })));
      });
    });
  }

  // ───────── Filters ─────────

  function filtersHTML() {
    const i = S.inst;
    if (!i) return '<div class="muted">Selecciona un clip de película o un botón para aplicarle filtros.</div>';
    if (i.kind === 'graphic' || i.kind === 'bitmap') return '<div class="muted">Los filtros solo se aplican a clips de película y botones.</div>';
    const list = i.filters.map((f, k) => `<div class="flt-row${k === filterSel ? ' sel' : ''}" data-flt="${k}">
      <span class="flt-on${f.on ? ' on' : ''}" data-fon="${k}" title="Activar o desactivar">${f.on ? '✓' : '✕'}</span><span>${FILTER_NAMES[f.kind]}</span></div>`).join('');
    return `<div class="flt">
      <div class="flt-side">
        <div class="flt-tools">
          <button class="ib" id="flt-add" title="Añadir filtro"><svg viewBox="0 0 20 20"><path d="M10 4v12M4 10h12"/></svg></button>
          <button class="ib" id="flt-del" title="Quitar filtro" ${i.filters.length ? '' : 'disabled'}><svg viewBox="0 0 20 20"><path d="M4 10h12"/></svg></button>
        </div>
        <div class="flt-list">${list || '<div class="muted" style="padding:4px">Sin filtros</div>'}</div>
      </div>
      <div class="flt-params" id="flt-params">${paramsHTML(i.filters[filterSel])}</div></div>`;
  }

  const num = (k, label, v, min, max, step = 1) => `<label class="fp"><span>${label}</span><input type="number" data-fp="${k}" value="${+(+v).toFixed(2)}" min="${min}" max="${max}" step="${step}"></label>`;
  const chk = (k, label, v) => `<label class="fp chk"><input type="checkbox" data-fp="${k}" ${v ? 'checked' : ''}> ${label}</label>`;
  const col = (k, label, c) => `<label class="fp"><span>${label}</span><div class="chip" data-fc="${k}" data-hex="${rgbaToHex(c)}" data-a="${c.a}"><i style="background:${rgbaToHex(c)}"></i></div></label>`;
  const sel = (k, label, v, list) => `<label class="fp"><span>${label}</span><select data-fp="${k}">${list.map(([x, l]) => `<option value="${x}" ${String(x) === String(v) ? 'selected' : ''}>${l}</option>`).join('')}</select></label>`;

  function paramsHTML(f) {
    if (!f) return '';
    const blur = num('blur_x', 'Desenfoque X', f.blur_x, 0, 100) + num('blur_y', 'Desenfoque Y', f.blur_y, 0, 100);
    const q = sel('quality', 'Calidad', f.quality, QUALITY);
    switch (f.kind) {
      case 'dropshadow': return blur + num('strength', 'Intensidad %', f.strength, 0, 1000) + q + num('angle', 'Ángulo', f.angle, -360, 360) + num('distance', 'Distancia', f.distance, -255, 255)
        + col('color', 'Color', f.color) + chk('knockout', 'Extractor', f.knockout) + chk('inner', 'Sombra interior', f.inner) + chk('hide', 'Ocultar objeto', f.hide);
      case 'blur': return blur + q;
      case 'glow': return blur + num('strength', 'Intensidad %', f.strength, 0, 1000) + q + col('color', 'Color', f.color) + chk('knockout', 'Extractor', f.knockout) + chk('inner', 'Iluminado interior', f.inner);
      case 'bevel': return blur + num('strength', 'Intensidad %', f.strength, 0, 1000) + q + col('shadow', 'Sombra', f.shadow) + col('highlight', 'Resaltado', f.highlight)
        + num('angle', 'Ángulo', f.angle, -360, 360) + num('distance', 'Distancia', f.distance, -255, 255) + chk('knockout', 'Extractor', f.knockout)
        + sel('bevel', 'Tipo', f.bevel, [['inner', 'Interior'], ['outer', 'Exterior'], ['full', 'Completo']]);
      case 'adjustcolor': return num('brightness', 'Brillo', f.brightness, -100, 100) + num('contrast', 'Contraste', f.contrast, -100, 100)
        + num('saturation', 'Saturación', f.saturation, -100, 100) + num('hue', 'Matiz', f.hue, -180, 180);
    }
    return '';
  }

  let filterSig = '';
  function updateFilters() {
    const host = $('#pb-filters');
    if (!host) return;
    const i = S.inst;
    if (i && filterSel >= i.filters.length) filterSel = Math.max(0, i.filters.length - 1);
    const sig = JSON.stringify([i && i.kind, i && i.filters, filterSel]);
    if (sig === filterSig || host.contains(document.activeElement) && document.activeElement.tagName === 'INPUT') return;
    filterSig = sig;
    host.innerHTML = filtersHTML();
  }

  function buildFilters(host) {
    host.addEventListener('click', (e) => {
      const on = e.target.closest('[data-fon]');
      if (on) return app.cmd(() => ed.toggle_filter(+on.dataset.fon));
      const row = e.target.closest('[data-flt]');
      if (row) {
        filterSel = +row.dataset.flt;
        filterSig = '';
        return updateFilters();
      }
      if (e.target.closest('#flt-add')) {
        const r = e.target.closest('#flt-add').getBoundingClientRect();
        return app.contextMenu(r.left, r.bottom + 2, Object.entries(FILTER_NAMES).map(([k, l]) => ({
          label: l, action: () => app.cmd(() => { ed.add_filter(k); filterSel = S.inst ? S.inst.filters.length : 0; }),
        })));
      }
      if (e.target.closest('#flt-del')) return app.cmd(() => ed.remove_filter(filterSel));
      const c = e.target.closest('[data-fc]');
      if (c) {
        app.openColorPopover(c, { hex: c.dataset.hex, alpha: +c.dataset.a, allowNone: false }, (hex, alpha) => {
          c.dataset.hex = hex;
          c.dataset.a = alpha;
          applyParams(host);
        });
      }
    });
    host.addEventListener('change', (e) => {
      if (e.target.closest('[data-fp]')) applyParams(host);
    });
  }

  function applyParams(host) {
    const f = S.inst && S.inst.filters[filterSel];
    if (!f) return;
    const out = { ...f };
    $$('[data-fp]', host).forEach((x) => {
      const k = x.dataset.fp;
      out[k] = x.type === 'checkbox' ? x.checked : (k === 'bevel' ? x.value : +x.value);
    });
    $$('[data-fc]', host).forEach((x) => (out[x.dataset.fc] = hexToRgba(x.dataset.hex, +x.dataset.a)));
    app.cmd(() => ed.set_filter(filterSel, JSON.stringify(out)));
  }

  return {
    setState(st) { S = st; },
    buildLibrary, buildFilters,
    update() {
      if (libSel && !S.library.some((s) => s.id === libSel)) libSel = null;
      // When an instance is selected, the Library shows its symbol (like Flash).
      if (S.inst && S.inst.symbol !== libSel) { libSel = S.inst.symbol; previewFrame = 0; stopPreview(); }
      renderList();
      if (!previewTimer) drawPreview();
      updateFilters();
    },
    instanceSig, instanceHTML, bindInstance, updateInstance,
    convertDialog, newSymbolDialog, swapDialog,
    get selected() { return libSel; },
  };
}
