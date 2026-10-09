// Text tool in the UI: the edit box over the canvas
// (with the same font the core draws with), the text section of the
// Property inspector, the Text menu, and loading fonts from the system or from files.
// When part of the text is selected, formatting (font, size, style,
// color, spacing) applies only to that part, as in Flash.

const $ = (q, el = document) => el.querySelector(q);
const $$ = (q, el = document) => [...el.querySelectorAll(q)];
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

export const SIZES = [8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72, 96, 120];
const FONT_LABEL = { _sans: '_sans', _serif: '_serif', _typewriter: '_typewriter' };
const ALIGN_ICON = {
  left: '<svg viewBox="0 0 20 20"><path d="M3 5h14M3 9h9M3 13h14M3 17h9"/></svg>',
  center: '<svg viewBox="0 0 20 20"><path d="M3 5h14M5.5 9h9M3 13h14M5.5 17h9"/></svg>',
  right: '<svg viewBox="0 0 20 20"><path d="M3 5h14M8 9h9M3 13h14M8 17h9"/></svg>',
  justify: '<svg viewBox="0 0 20 20"><path d="M3 5h14M3 9h14M3 13h14M3 17h9"/></svg>',
};

export function initText(app) {
  const { ed } = app;
  let S = null;
  let box = null; // edit box
  const faces = new Map();

  // ───────── Fonts: the edit box uses the same data as the core ─────────

  function cssFamily(info) {
    const fam = `OMF ${info.font}`;
    const realBold = info.bold && !info.fakeBold, realItalic = info.italic && !info.fakeItalic;
    const key = `${info.font}|${realBold}|${realItalic}`;
    if (!faces.has(key)) {
      const bytes = ed.font_data(info.font, info.bold, info.italic);
      const face = new FontFace(fam, bytes, { weight: realBold ? '700' : '400', style: realItalic ? 'italic' : 'normal' });
      faces.set(key, face.load().then((f) => { document.fonts.add(f); if (box) place(); }).catch(() => {}));
    }
    return `"${fam}", sans-serif`;
  }

  // ───────── Edit box ─────────

  function info() {
    return JSON.parse(ed.text_edit_info());
  }

  // Text and cursor positions counting characters (line breaks count as one).
  function walk(fn) {
    const w = document.createTreeWalker(box, NodeFilter.SHOW_TEXT | NodeFilter.SHOW_ELEMENT);
    let n;
    while ((n = w.nextNode())) {
      if (n.nodeType === 3) fn(n, n.data.length);
      else if (n.nodeName === 'BR') fn(n, 1);
      else if (n.nodeName === 'DIV' && n !== box && n.previousSibling) fn(n, 1, true);
    }
  }
  function boxText() {
    let t = '';
    walk((n, len, div) => (t += n.nodeType === 3 ? n.data : '\n'));
    return t.replace(/\n$/, '');
  }
  function offsetOf(node, off) {
    let pos = 0, found = -1;
    if (node === box) {
      // Offset within the box's children: what comes before that child.
      const stop = box.childNodes[off];
      walk((n, len) => { if (found < 0 && (n === stop || (stop && stop.contains(n)))) found = pos; pos += len; });
      return found < 0 ? pos : found;
    }
    walk((n, len) => {
      if (found >= 0) return;
      if (n === node) found = pos + (n.nodeType === 3 ? off : 0);
      else if (node.nodeType === 1 && node.contains(n) && n.nodeType === 3) found = pos;
      pos += len;
    });
    return found < 0 ? pos : found;
  }
  function selOffsets() {
    const s = getSelection();
    if (!box || !s.rangeCount || !box.contains(s.anchorNode)) return null;
    const a = offsetOf(s.anchorNode, s.anchorOffset), b = offsetOf(s.focusNode, s.focusOffset);
    return [Math.min(a, b), Math.max(a, b)];
  }
  function setSel([a, b]) {
    const pick = (target) => {
      let pos = 0, res = null;
      walk((n, len) => {
        if (res || n.nodeType !== 3) { if (!res) pos += n.nodeType === 3 ? 0 : len; return; }
        if (target <= pos + len) res = [n, target - pos];
        pos += len;
      });
      return res || [box, box.childNodes.length];
    };
    const r = document.createRange();
    const [an, ao] = pick(a), [bn, bo] = pick(b);
    r.setStart(an, ao);
    r.setEnd(bn, bo);
    const s = getSelection();
    s.removeAllRanges();
    s.addRange(r);
  }

  /** Box content: with formats, one span per format. */
  let runsSig = '';
  function render(i) {
    const runs = i.runs || [];
    const sig = JSON.stringify([i.text, runs]);
    if (sig === runsSig && box.childNodes.length) return;
    runsSig = sig;
    // No formats and same text: leave it alone (don't lose what is being typed).
    if (!runs.length && !box.querySelector('span') && boxText() === i.text && (i.text || !box.childNodes.length)) return;
    const keep = document.activeElement === box ? selOffsets() : null;
    if (!runs.length) {
      box.textContent = i.text;
    } else {
      const chars = [...i.text];
      let pos = 0, html = '';
      for (const r of runs) {
        const part = chars.slice(pos, pos + r.len).join('');
        pos += r.len;
        const n = parseInt(r.color.slice(1), 16);
        const st = `font-family:${cssFamily(r).replace(/"/g, "'")};font-size:${r.size}px;font-weight:${r.bold ? 700 : 400};font-style:${r.italic ? 'italic' : 'normal'};letter-spacing:${r.letter}px;color:rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${(r.alpha / 255).toFixed(3)})`;
        html += `<span style="${st}">${esc(part)}</span>`;
      }
      box.innerHTML = html;
    }
    if (keep) setSel(keep);
  }

  function open() {
    const i = info();
    if (!i) return;
    box = document.createElement('div');
    box.className = 'text-edit';
    box.dataset.id = i.id;
    box.contentEditable = 'plaintext-only';
    box.spellcheck = false;
    runsSig = '';
    $('#viewport').appendChild(box);
    place(i);
    box.addEventListener('input', () => {
      const keep = selOffsets();
      ed.text_set(boxText());
      if (keep) ed.text_select(keep[0], keep[1]);
      app.refresh();
    });
    box.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Escape') {
        e.preventDefault();
        commit();
      } else if (e.key === 'Enter' && box.dataset.single === '1') {
        e.preventDefault();
      }
    });
    box.addEventListener('pointerdown', (e) => e.stopPropagation());
    box.focus();
    // Cursor at the end, as when clicking a field in Flash.
    const r = document.createRange();
    r.selectNodeContents(box);
    r.collapse(false);
    const sel = getSelection();
    sel.removeAllRanges();
    sel.addRange(r);
  }

  function place(i = info()) {
    if (!box || !i) return;
    const s = box.style;
    // The matrix is relative to the canvas, which with rulers does not start at the viewport corner.
    const cv = $('#stage');
    const m = [...i.matrix];
    m[4] += cv.offsetLeft;
    m[5] += cv.offsetTop;
    s.transform = `matrix(${m.join(',')})`;
    s.fontFamily = cssFamily(i);
    s.fontSize = i.size + 'px';
    s.fontWeight = i.bold ? '700' : '400';
    s.fontStyle = i.italic ? 'italic' : 'normal';
    s.lineHeight = i.lineHeight + i.leading + 'px';
    s.letterSpacing = i.letter + 'px';
    s.textAlign = i.align;
    s.fontKerning = i.kerning === false ? 'none' : 'normal';
    render(i);
    const a = (i.alpha / 255).toFixed(3);
    const n = parseInt(i.color.slice(1), 16);
    s.color = `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${a})`;
    const single = i.kind !== 'static' && i.lines === 'single';
    box.dataset.single = single ? '1' : '';
    if (i.width) {
      s.width = Math.max(4, i.width - 4) + 'px';
      s.whiteSpace = single || i.lines === 'nowrap' ? 'pre' : 'pre-wrap';
    } else {
      s.width = 'auto';
      s.whiteSpace = 'pre';
    }
    box.classList.toggle('fixed', !!i.width);
  }

  function close() {
    box?.remove();
    box = null;
  }

  // The selection inside the box goes to the core: formatting applies to it
  // and the panel shows the format at its start.
  let lastSel = '';
  document.addEventListener('selectionchange', () => {
    if (!box || document.activeElement !== box) return;
    const o = selOffsets();
    if (!o) return;
    const k = o.join();
    if (k === lastSel) return;
    lastSel = k;
    ed.text_select(o[0], o[1]);
    app.refresh();
  });

  function commit() {
    if (!box) return;
    ed.text_set(boxText());
    close();
    ed.text_commit();
    app.refresh();
  }

  function sync() {
    if (S.textEdit) {
      const id = JSON.parse(ed.text_edit_info())?.id;
      if (box && String(id) !== box.dataset.id) close();
      if (!box) open();
      else place();
    } else if (box) {
      close();
    }
  }

  // ───────── Property inspector ─────────

  const opt = (v, l, cur) => `<option value="${esc(v)}" ${String(v) === String(cur) ? 'selected' : ''}>${esc(l)}</option>`;

  function propsSig() {
    const t = S.textStyle;
    return `text:${t.kind}:${S.fonts.length}`;
  }

  // When the panel is touched, the box loses focus but the text selection is kept.
  const keepSel = () => {
    if (!box) return;
    const o = lastSel.split(',').map(Number);
    if (o.length === 2 && o[1] > o[0]) ed.text_select(o[0], o[1]);
  };

  function propsHTML() {
    const t = S.textStyle;
    const dyn = t.kind !== 'static';
    return `
      <div class="ptitle">Texto</div>
      <div class="prow"><select id="pt-kind">${opt('static', 'Texto estático', t.kind)}${opt('dynamic', 'Texto dinámico', t.kind)}${opt('input', 'Texto de entrada', t.kind)}</select></div>
      <div class="prow"><select id="pt-font" style="width:150px">${S.fonts.map((f) => opt(f, FONT_LABEL[f] || f, t.font)).join('')}</select>
        <button class="ib" id="pt-morefonts" title="Más fuentes (del sistema o de un archivo)"><svg viewBox="0 0 20 20"><path d="M10 4v12M4 10h12"/></svg></button></div>
      <div class="prow"><input type="number" id="pt-size" min="1" max="2000" step="1" style="width:56px" title="Tamaño en puntos">
        <div class="chip" id="pt-color" title="Color del texto"><i></i></div>
        <button class="ib tb" id="pt-bold" title="Negrita (Ctrl+Mayús+B)"><b>B</b></button>
        <button class="ib tb" id="pt-italic" title="Cursiva (Ctrl+Mayús+I)"><i style="font-family:serif">I</i></button></div>
      <div class="prow">${['left', 'center', 'right', 'justify'].map((a) => `<button class="ib" data-talign="${a}" title="${{ left: 'Izquierda', center: 'Centrar', right: 'Derecha', justify: 'Justificar' }[a]}">${ALIGN_ICON[a]}</button>`).join('')}</div>
      <div class="prow"><span class="lab">Espaciado</span><input type="number" id="pt-letter" min="-60" max="60" step="0.5" title="Espaciado entre caracteres"></div>
      <div class="prow"><span class="lab">Línea</span><input type="number" id="pt-leading" min="-360" max="720" step="1" title="Espaciado de línea"></div>
      <div class="prow"><span class="lab"></span><label class="chk" title="Usa el ajuste entre pares de letras de la fuente (por ejemplo «AV»)"><input type="checkbox" id="pt-kern"> Ajuste automático (kerning)</label></div>
      ${dyn ? `
      <div class="prow"><span class="lab">Fuente</span><button class="btn small" id="pt-embed" title="Incorporar los contornos de la fuente en la película">Incorporar…</button><span class="muted" id="pt-embed-st"></span></div>
      <div class="prow"><span class="lab">Nombre</span><input type="text" id="pt-name" placeholder="<Nombre de instancia>" style="width:140px"></div>
      <div class="prow"><span class="lab">Líneas</span><select id="pt-lines">${opt('single', 'Una línea', t.lines)}${opt('multi', 'Multilínea', t.lines)}${opt('nowrap', 'Multilínea sin ajuste', t.lines)}</select></div>
      <div class="prow"><span class="lab"></span><label class="chk"><input type="checkbox" id="pt-selectable"> Seleccionable</label><label class="chk" style="margin-left:8px"><input type="checkbox" id="pt-border"> Mostrar borde</label></div>
      <div class="prow"><span class="lab">Variable</span><input type="text" id="pt-var" style="width:140px"></div>
      ${t.kind === 'input' ? '<div class="prow"><span class="lab">Máx. caract.</span><input type="number" id="pt-max" min="0" max="65535"></div>' : ''}` : ''}`;
  }

  const style = (patch) => {
    keepSel();
    ed.set_text_style(JSON.stringify(patch));
    app.refresh();
  };

  function embedDialog() {
    const t = S.textStyle;
    app.modal('Incorporación de caracteres', `
      <p class="muted" style="max-width:340px">Con los contornos incorporados, el texto dinámico se ve con la misma fuente en cualquier equipo, se puede girar y se suaviza. Se incluyen los caracteres del campo y los latinos básicos.</p>
      <div class="prow"><label class="chk"><input type="checkbox" id="em-on" ${t.embed ? 'checked' : ''}> Incorporar contornos de fuente</label></div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="em-ok">Aceptar</button></div>`, (b) => {
      $('#em-ok', b).addEventListener('click', () => {
        const on = $('#em-on', b).checked;
        app.closeModal();
        style({ embed: on });
      });
    });
  }

  function bindProps(host) {
    const on = (q, ev, fn) => $(q, host)?.addEventListener(ev, fn);
    on('#pt-kind', 'change', (e) => style({ kind: e.target.value }));
    on('#pt-font', 'change', (e) => style({ font: e.target.value }));
    on('#pt-size', 'change', (e) => style({ size: +e.target.value || 12 }));
    on('#pt-bold', 'click', () => style({ bold: !S.textStyle.bold }));
    on('#pt-italic', 'click', () => style({ italic: !S.textStyle.italic }));
    on('#pt-letter', 'change', (e) => style({ letter: +e.target.value || 0 }));
    on('#pt-leading', 'change', (e) => style({ leading: +e.target.value || 0 }));
    on('#pt-name', 'change', (e) => style({ name: e.target.value }));
    on('#pt-lines', 'change', (e) => style({ lines: e.target.value }));
    on('#pt-selectable', 'change', (e) => style({ selectable: e.target.checked }));
    on('#pt-border', 'change', (e) => style({ border: e.target.checked }));
    on('#pt-var', 'change', (e) => style({ var: e.target.value }));
    on('#pt-max', 'change', (e) => style({ maxChars: Math.max(0, +e.target.value || 0) }));
    on('#pt-kern', 'change', (e) => style({ kerning: e.target.checked }));
    on('#pt-embed', 'click', () => embedDialog());
    on('#pt-color', 'click', (e) => {
      const keep = lastSel;
      app.openColorPopover(e.currentTarget, { hex: S.textColor, alpha: S.textAlpha, allowNone: false },
        (hex, alpha) => { lastSel = keep; keepSel(); app.cmd(() => ed.set_text_color(hex, alpha)); });
    });
    on('#pt-morefonts', 'click', () => moreFonts());
    $$('[data-talign]', host).forEach((b) => b.addEventListener('click', () => style({ align: b.dataset.talign })));
  }

  function updateProps(host) {
    const t = S.textStyle;
    app.setVal($('#pt-kind', host), t.kind);
    app.setVal($('#pt-font', host), t.font);
    app.setVal($('#pt-size', host), t.size);
    app.setVal($('#pt-letter', host), t.letter);
    app.setVal($('#pt-leading', host), t.leading);
    app.setVal($('#pt-name', host), t.name || '');
    app.setVal($('#pt-lines', host), t.lines);
    app.setVal($('#pt-selectable', host), t.selectable);
    app.setVal($('#pt-border', host), t.border);
    app.setVal($('#pt-var', host), t.var || '');
    app.setVal($('#pt-max', host), t.max_chars);
    app.setVal($('#pt-kern', host), t.kerning !== false);
    const st = $('#pt-embed-st', host);
    if (st) st.textContent = t.embed ? 'incorporada' : 'de dispositivo';
    $('#pt-bold', host)?.classList.toggle('on', t.bold);
    $('#pt-italic', host)?.classList.toggle('on', t.italic);
    $$('[data-talign]', host).forEach((b) => b.classList.toggle('on', b.dataset.talign === t.align));
    app.paintChip($('#pt-color', host), S.textColor, S.textAlpha);
  }

  // ───────── More fonts ─────────

  const styleOf = (s) => ({ bold: /bold|black|heavy|semibold|negrita/i.test(s), italic: /italic|oblique|cursiva/i.test(s) });

  async function moreFonts() {
    const local = typeof window.queryLocalFonts === 'function';
    app.modal('Más fuentes', `
      <p class="muted" style="max-width:360px">Las fuentes incluidas son libres (Arimo, Tinos y Cousine, equivalentes a Arial, Times y Courier). Puedes añadir las de tu equipo o un archivo .ttf/.otf; se usan en esta sesión y se guardan por su nombre en el documento.</p>
      <div class="prow">${local ? '<button class="btn" id="mf-local">Fuentes del sistema…</button>' : ''}
        <label class="btn" style="display:inline-flex;align-items:center">Archivo de fuente…<input type="file" id="mf-file" accept=".ttf,.otf" multiple hidden></label></div>
      <div id="mf-list" class="swap-list" style="display:none"></div>
      <div class="modal-actions"><button class="btn" data-close>Cerrar</button></div>`, (b) => {
      $('#mf-file', b).addEventListener('change', async (e) => {
        let last = null;
        for (const f of e.target.files) {
          const base = f.name.replace(/\.(ttf|otf)$/i, '');
          const st = styleOf(base);
          const family = base.replace(/[-_ ]?(bold|italic|oblique|regular|black|light)+$/i, '').replace(/[-_]/g, ' ').trim() || base;
          try {
            last = ed.register_font(new Uint8Array(await f.arrayBuffer()), family, st.bold, st.italic);
          } catch (err) {
            app.toast(String(err));
          }
        }
        if (last) {
          app.closeModal();
          style({ font: last });
          app.toast(`Fuente añadida: ${last}`);
        }
      });
      $('#mf-local', b)?.addEventListener('click', async () => {
        let fonts;
        try {
          fonts = await window.queryLocalFonts();
        } catch {
          return app.toast('El navegador no dio permiso para leer las fuentes del sistema');
        }
        const fams = [...new Set(fonts.map((f) => f.family))].sort((a, c) => a.localeCompare(c));
        const list = $('#mf-list', b);
        list.style.display = '';
        list.innerHTML = fams.map((f) => `<div class="lib-row" data-fam="${esc(f)}" style="font-family:'${esc(f)}'"><span class="lname">${esc(f)}</span></div>`).join('');
        list.addEventListener('click', async (e) => {
          const row = e.target.closest('[data-fam]');
          if (!row) return;
          const fam = row.dataset.fam;
          for (const f of fonts.filter((x) => x.family === fam)) {
            const st = styleOf(f.style);
            try {
              ed.register_font(new Uint8Array(await (await f.blob()).arrayBuffer()), fam, st.bold, st.italic);
            } catch {}
          }
          app.closeModal();
          style({ font: fam });
        });
      });
    });
  }

  // ───────── Text menu ─────────

  function menu(extra = []) {
    const t = S.textStyle;
    const has = S.textEdit || S.textSel || S.tool === 'text';
    return [
      { label: 'Fuente', sub: S.fonts.map((f) => ({ label: FONT_LABEL[f] || f, check: t.font === f, action: () => style({ font: f }) })).concat(['-', { label: 'Más fuentes…', action: () => moreFonts() }]) },
      { label: 'Tamaño', sub: SIZES.map((n) => ({ label: String(n), check: t.size === n, action: () => style({ size: n }) })) },
      { label: 'Estilo', sub: [
        { label: 'Texto normal', check: !t.bold && !t.italic, action: () => style({ bold: false, italic: false }) },
        { label: 'Negrita', key: 'Ctrl+Mayús+B', check: t.bold, action: () => style({ bold: !t.bold }) },
        { label: 'Cursiva', key: 'Ctrl+Mayús+I', check: t.italic, action: () => style({ italic: !t.italic }) },
      ] },
      { label: 'Alinear', sub: [
        { label: 'Alinear a la izquierda', key: 'Ctrl+Mayús+L', check: t.align === 'left', action: () => style({ align: 'left' }) },
        { label: 'Centrar', key: 'Ctrl+Mayús+C', check: t.align === 'center', action: () => style({ align: 'center' }) },
        { label: 'Alinear a la derecha', key: 'Ctrl+Mayús+R', check: t.align === 'right', action: () => style({ align: 'right' }) },
        { label: 'Justificar', key: 'Ctrl+Mayús+J', check: t.align === 'justify', action: () => style({ align: 'justify' }) },
      ] },
      { label: 'Espaciado entre caracteres', sub: [
        { label: 'Aumentar', key: 'Ctrl+Alt+→', action: () => style({ letter: t.letter + 0.5 }) },
        { label: 'Disminuir', key: 'Ctrl+Alt+←', action: () => style({ letter: t.letter - 0.5 }) },
        { label: 'Restablecer', key: 'Ctrl+Alt+↑', action: () => style({ letter: 0 }) },
      ] },
    ].map((it) => (it === '-' || it.sub || has ? it : { ...it, disabled: true })).concat(extra);
  }

  // Formatting shortcuts (only act on text).
  const SHORTCUTS = {
    'Ctrl+Shift+B': () => style({ bold: !S.textStyle.bold }),
    'Ctrl+Shift+I': () => style({ italic: !S.textStyle.italic }),
    'Ctrl+Shift+L': () => style({ align: 'left' }),
    'Ctrl+Shift+C': () => style({ align: 'center' }),
    'Ctrl+Shift+R': () => style({ align: 'right' }),
    'Ctrl+Shift+J': () => style({ align: 'justify' }),
    'Ctrl+Alt+Right': () => style({ letter: S.textStyle.letter + 0.5 }),
    'Ctrl+Alt+Left': () => style({ letter: S.textStyle.letter - 0.5 }),
    'Ctrl+Alt+Up': () => style({ letter: 0 }),
  };

  return {
    setState(st) { S = st; },
    sync, commit, menu, moreFonts,
    get editing() { return !!box; },
    shortcut(combo) {
      const fn = SHORTCUTS[combo];
      if (!fn || !(S.textEdit || S.textSel || S.tool === 'text')) return false;
      fn();
      return true;
    },
    propsSig, propsHTML, bindProps, updateProps,
  };
}
