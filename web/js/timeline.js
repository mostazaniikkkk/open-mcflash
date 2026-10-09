// Timeline with the Flash 8 visual grammar:
//  ● keyframe with content   ○ blank keyframe   ▯ end of span
//  gray = span with content   violet + arrow = motion tween
//  green + arrow = shape tween   dashed = broken tween
//  red flag = label · "//" = comment · anchor = anchor label
//  red = playhead · brackets on the ruler = onion skin markers

export const RULER_H = 20;
const BUTTON_STATES = ['Reposo', 'Sobre', 'Presionado', 'Zona activa'];
const SIZES = { tiny: 5, small: 7, normal: 9, medium: 13, large: 18 };
const SIZE_NAMES = [['tiny', 'Muy pequeño'], ['small', 'Pequeño'], ['normal', 'Normal'], ['medium', 'Mediano'], ['large', 'Grande']];

const PALETTES = {
  dark: {
    bg: '#232323', row: '#2a2a2a', every5: '#303030', line: '#363636', rowLine: '#1d1d1d', folder: '#262626',
    span: '#4b4b4b', spanEmpty: '#2f2f2f', tween: '#5b51a8', shape: '#3f7a46', tweenLine: '#ffffff',
    key: '#ececec', keyHollowFill: '#2a2a2a', endRect: '#cfcfcf',
    ruler: '#2e2e2e', rulerText: '#9a9a9a', rulerTick: '#5a5a5a',
    sel: 'rgba(47,140,255,.55)', selKey: '#ffffff', head: '#e8403a', activeRow: 'rgba(47,140,255,.08)',
    label: '#e8403a', comment: '#4fb35a', text: '#e6e6e6', onion: 'rgba(120,160,255,.25)', marker: '#7aa7ff',
  },
  classic: {
    bg: '#ffffff', row: '#ffffff', every5: '#ececec', line: '#dcdcdc', rowLine: '#c4c4c4', folder: '#f3f3f3',
    span: '#d4d4d4', spanEmpty: '#ffffff', tween: '#ccccff', shape: '#c6f0c0', tweenLine: '#000000',
    key: '#000000', keyHollowFill: '#ffffff', endRect: '#000000',
    ruler: '#f2f2f2', rulerText: '#333333', rulerTick: '#9a9a9a',
    sel: '#000000', selKey: '#ffffff', head: '#ff0000', activeRow: 'rgba(0,0,0,0)',
    label: '#cc0000', comment: '#2a8a2a', text: '#111111', onion: 'rgba(0,0,200,.12)', marker: '#3355cc',
  },
};

const ICON = {
  normal: '<svg viewBox="0 0 20 20"><path d="M4 3h8l4 4v10H4z"/><path d="M12 3v4h4"/></svg>',
  guide: '<svg viewBox="0 0 20 20"><path d="M4 4h12M10 4v13" /><path d="M7 17h6" stroke-dasharray="1.5 1.5"/></svg>',
  motionguide: '<svg viewBox="0 0 20 20"><path d="M3 16c3-10 6 0 9-7s4-4 5-6" stroke-dasharray="2 1.6"/><circle cx="3.5" cy="15.5" r="1.6" class="f"/></svg>',
  mask: '<svg viewBox="0 0 20 20"><rect x="3" y="3" width="14" height="14"/><circle cx="10" cy="10" r="4" class="f"/></svg>',
  masked: '<svg viewBox="0 0 20 20"><rect x="4" y="4" width="12" height="12" stroke-dasharray="2 1.5"/><circle cx="10" cy="10" r="3" class="f"/></svg>',
  guided: '<svg viewBox="0 0 20 20"><path d="M4 3h8l4 4v10H4z"/><path d="M6 14c2-4 4 0 8-5" stroke-dasharray="1.5 1.5"/></svg>',
  folderOpen: '<svg viewBox="0 0 20 20"><path d="M2 6h6l2 2h8v8H2z"/><path d="M2 10h16"/></svg>',
  folderClosed: '<svg viewBox="0 0 20 20"><path d="M2 6h6l2 2h8v8H2z" class="f" opacity=".85"/></svg>',
};

export class Timeline {
  constructor({ canvas, layersEl, app }) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.layersEl = layersEl;
    this.app = app;
    this.scrollX = 0;
    this.scrollY = 0;
    this.w = 0;
    this.h = 0;
    this.state = null;
    this.palette = PALETTES.dark;
    this.renaming = -1;
    this._sig = '';
    this.view = { size: 'normal', short: false, preview: false, ...(app.loadPref('omf.tlview', {})) };
    this.thumbs = new Map();
    this.thumbHash = 0;
    this.drag = null;

    new ResizeObserver(() => this.resize()).observe(canvas.parentElement);
    this.bindGrid();
    this.bindLayers();
    this.buildViewButton();
  }

  get cellW() {
    // In buttons, wide cells with the state names (Up, Over…).
    if (this.state && this.state.editingKind === 'button') return 64;
    return SIZES[this.view.size] || 9;
  }
  rowH(l) {
    return (this.view.preview ? 34 : this.view.short ? 16 : 22) * (l.height || 1);
  }

  setClassic(on) {
    this.palette = on ? PALETTES.classic : PALETTES.dark;
    this.draw();
  }

  setView(patch) {
    Object.assign(this.view, patch);
    this.app.savePref('omf.tlview', this.view);
    this._sig = '';
    this.update(this.state);
  }

  resize() {
    const r = this.canvas.parentElement.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.w = r.width;
    this.h = r.height;
    this.canvas.width = Math.max(1, Math.round(r.width * dpr));
    this.canvas.height = Math.max(1, Math.round(r.height * dpr));
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.clampScroll();
    this.draw();
  }

  update(state) {
    if (!state) return;
    this.state = state;
    this.computeRows();
    this.renderLayers();
    this.clampScroll();
    this.draw();
  }

  /// Visible rows (those inside collapsed folders are not shown).
  computeRows() {
    let y = 0;
    this.rows = [];
    this.state.layers.forEach((l, li) => {
      if (l.collapsed) return;
      const h = this.rowH(l);
      this.rows.push({ li, y, h });
      y += h;
    });
    this.rowsH = y;
  }

  rowOf(li) {
    return this.rows.find((r) => r.li === li);
  }

  clampScroll() {
    if (!this.state) return;
    const maxY = Math.max(0, this.rowsH - (this.h - RULER_H));
    this.scrollY = Math.min(Math.max(0, this.scrollY || 0), maxY);
    this.scrollX = Math.max(0, this.scrollX || 0);
    const rows = this.layersEl.querySelector('.rows');
    if (rows) rows.style.transform = `translateY(${-this.scrollY}px)`;
  }

  ensureVisible(frame) {
    const cw = this.cellW;
    const x = frame * cw;
    if (x < this.scrollX) this.scrollX = Math.max(0, x - cw * 4);
    else if (x + cw > this.scrollX + this.w) this.scrollX = x - this.w + cw * 8;
  }

  frameAt(x) {
    return Math.max(0, Math.floor((x + this.scrollX) / this.cellW));
  }
  rowAt(y) {
    const yy = y - RULER_H + this.scrollY;
    const r = this.rows.find((r) => yy >= r.y && yy < r.y + r.h);
    return r ? r.li : -1;
  }

  // ───────── Layer list (HTML) ─────────

  renderLayers() {
    const st = this.state;
    const sig = JSON.stringify([st.layers.map((l) => [l.name, l.visible, l.locked, l.outline, l.color, l.kind, l.depth, l.open, l.collapsed, l.height, l.masked, l.guided]), st.layer, this.renaming, this.view]);
    if (sig === this._sig) return;
    this._sig = sig;
    const allVisible = st.layers.every((l) => l.visible);
    const allLocked = st.layers.every((l) => l.locked);
    let html = `<div class="head"><span class="t">Capas</span>
      <span class="c" data-all="eye" title="Mostrar/ocultar todas">${allVisible ? '👁' : '<span style="color:#e04a4a">✕</span>'}</span>
      <span class="c" data-all="lock" title="Bloquear/desbloquear todas">${allLocked ? '🔒' : '🔓'}</span>
      <span class="c" data-all="outline" title="Mostrar todas las capas como contornos"><svg viewBox="0 0 12 12"><rect x="2" y="2" width="8" height="8"/></svg></span></div><div class="rows">`;
    st.layers.forEach((l, i) => {
      if (l.collapsed) return;
      const active = i === st.layer;
      const icon = l.kind === 'folder' ? (l.open ? ICON.folderOpen : ICON.folderClosed)
        : l.kind !== 'normal' ? ICON[l.kind] : l.masked ? ICON.masked : l.guided ? ICON.guided : ICON.normal;
      const name = this.renaming === i
        ? `<input type="text" value="${escapeHtml(l.name)}" data-rename="${i}">`
        : `<span class="lname">${escapeHtml(l.name)}</span>`;
      const fold = l.kind === 'folder' ? `<span class="fold" data-fold="${i}" title="Desplegar/plegar">${l.open ? '▾' : '▸'}</span>` : '';
      html += `<div class="layer${active ? ' active' : ''} k-${l.kind}" data-i="${i}" style="height:${this.rowH(l)}px;padding-left:${4 + l.depth * 14}px">
        ${fold}<span class="licon" data-props="${i}" title="Propiedades de capa (doble clic)">${icon}</span>${name}
        <span class="pencil">${active && !l.effLocked && l.kind !== 'folder' ? '<svg viewBox="0 0 20 20" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M3 15l1-3 7-7 2 2-7 7z"/></svg>' : ''}</span>
        <span class="lc${l.visible ? '' : ' x'}" data-eye="${i}" title="Mostrar/ocultar">${l.visible ? '<span class="dot"></span>' : '✕'}</span>
        <span class="lc" data-lock="${i}" title="Bloquear/desbloquear">${l.locked ? '<svg viewBox="0 0 12 12"><rect x="2" y="5.5" width="8" height="5.5" fill="currentColor"/><path d="M3.8 5.5V4a2.2 2.2 0 014.4 0v1.5"/></svg>' : '<span class="dot"></span>'}</span>
        <span class="lc" data-outline="${i}" title="Ver capa como contornos"><span class="sq ${l.outline ? 'on' : 'off'}" style="color:${l.color}"></span></span>
      </div>`;
    });
    html += '</div>';
    this.layersEl.innerHTML = html;
    this.clampScroll();

    const input = this.layersEl.querySelector('input[data-rename]');
    if (input) {
      input.focus();
      input.select();
      const done = (ok) => {
        const i = +input.dataset.rename;
        this.renaming = -1;
        if (ok) this.app.cmd(() => this.app.ed.rename_layer(i, input.value));
        else this.app.refresh();
      };
      input.addEventListener('keydown', (e) => {
        e.stopPropagation();
        if (e.key === 'Enter') done(true);
        if (e.key === 'Escape') done(false);
      });
      input.addEventListener('blur', () => this.renaming >= 0 && done(true));
    }
  }

  bindLayers() {
    const el = this.layersEl;
    let drag = null;
    el.addEventListener('pointerdown', (e) => {
      if (e.target.closest('input') || e.button !== 0) return;
      const app = this.app;
      const all = e.target.closest('[data-all]');
      if (all) {
        const st = this.state;
        const key = all.dataset.all;
        const prop = { eye: 'visible', lock: 'locked', outline: 'outline' }[key];
        const toggle = { eye: 'toggle_visible', lock: 'toggle_lock', outline: 'toggle_outline' }[key];
        const target = !st.layers.every((l) => l[prop]);
        st.layers.forEach((l, i) => {
          if (l[prop] !== target) app.ed[toggle](i);
        });
        app.refresh();
        return;
      }
      const fold = e.target.closest('[data-fold]');
      if (fold) return app.cmd(() => app.ed.toggle_folder(+fold.dataset.fold));
      const eye = e.target.closest('[data-eye]');
      if (eye) return app.cmd(() => app.ed.toggle_visible(+eye.dataset.eye));
      const lock = e.target.closest('[data-lock]');
      if (lock) return app.cmd(() => app.ed.toggle_lock(+lock.dataset.lock));
      const outl = e.target.closest('[data-outline]');
      if (outl) return app.cmd(() => app.ed.toggle_outline(+outl.dataset.outline));
      const row = e.target.closest('.layer');
      if (!row) return;
      const i = +row.dataset.i;
      app.ed.select_cell(i, this.state.frame);
      app.refresh();
      // Pointer capture starts when dragging: this way the double-click reaches the row (rename).
      drag = { i, y: e.clientY, active: false };
    });
    el.addEventListener('pointermove', (e) => {
      if (!drag) return;
      if (!drag.active && Math.abs(e.clientY - drag.y) > 4) {
        drag.active = true;
        el.setPointerCapture(e.pointerId);
      }
      if (!drag.active) return;
      el.querySelectorAll('.layer').forEach((r) => r.classList.remove('drop-before', 'drop-after', 'dragging'));
      el.querySelector(`.layer[data-i="${drag.i}"]`)?.classList.add('dragging');
      const to = this.dropIndex(e.clientY);
      drag.to = to;
      el.querySelector(`.layer[data-i="${to}"]`)?.classList.add(to > drag.i ? 'drop-after' : 'drop-before');
    });
    el.addEventListener('pointerup', () => {
      if (drag?.active && drag.to !== undefined && drag.to !== drag.i) {
        const { i, to } = drag;
        this.app.cmd(() => this.app.ed.drag_layer(i, to));
      } else {
        el.querySelectorAll('.layer').forEach((r) => r.classList.remove('drop-before', 'drop-after', 'dragging'));
      }
      drag = null;
    });
    el.addEventListener('dblclick', (e) => {
      const icon = e.target.closest('[data-props]');
      if (icon) return this.app.layerProps(+icon.dataset.props);
      const row = e.target.closest('.layer');
      if (!row || e.target.closest('.lc') || e.target.closest('[data-fold]')) return;
      this.renaming = +row.dataset.i;
      this._sig = '';
      this.renderLayers();
    });
    el.addEventListener('wheel', (e) => {
      e.preventDefault();
      this.scrollY += e.deltaY;
      this.clampScroll();
      this.draw();
    }, { passive: false });
    el.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      const row = e.target.closest('.layer');
      if (row) {
        this.app.ed.select_cell(+row.dataset.i, this.state.frame);
        this.app.refresh();
      }
      const st = this.state;
      const l = st.layers[st.layer];
      const i = st.layer;
      const ed = this.app.ed;
      this.app.contextMenu(e.clientX, e.clientY, [
        { label: 'Mostrar todo', action: () => this.showAll() },
        { label: 'Bloquear otras capas', action: () => this.lockOthers() },
        { label: 'Ocultar otras capas', action: () => this.hideOthers() },
        '-',
        { label: 'Insertar capa', cmd: 'addLayer' },
        { label: 'Eliminar capa', cmd: 'deleteLayer' },
        '-',
        { label: 'Guía', check: l.kind === 'guide', action: () => this.app.cmd(() => ed.set_layer_kind(i, 'guide')) },
        { label: 'Añadir guía de movimiento', cmd: 'addMotionGuide' },
        '-',
        { label: 'Máscara', check: l.kind === 'mask', action: () => this.app.cmd(() => ed.set_layer_kind(i, l.kind === 'mask' ? 'normal' : 'mask')) },
        { label: 'Mostrar enmascaramiento', disabled: l.kind !== 'mask', action: () => this.showMasking(i) },
        '-',
        { label: 'Insertar carpeta', cmd: 'addFolder' },
        { label: 'Expandir carpeta', disabled: l.kind !== 'folder' || l.open, action: () => this.app.cmd(() => ed.toggle_folder(i)) },
        { label: 'Contraer carpeta', disabled: l.kind !== 'folder' || !l.open, action: () => this.app.cmd(() => ed.toggle_folder(i)) },
        '-',
        { label: 'Cambiar nombre…', action: () => { this.renaming = st.layer; this._sig = ''; this.renderLayers(); } },
        { label: 'Propiedades…', action: () => this.app.layerProps(i) },
      ]);
    });
  }

  dropIndex(clientY) {
    const r = this.layersEl.getBoundingClientRect();
    const li = this.rowAt(clientY - r.top);
    if (li >= 0) return li;
    return this.rows.length ? this.rows[this.rows.length - 1].li : 0;
  }

  forEachLayer(fn) {
    this.state.layers.forEach((l, i) => fn(l, i));
    this.app.refresh();
  }

  showAll() {
    this.forEachLayer((l, i) => {
      if (!l.visible) this.app.ed.toggle_visible(i);
      if (l.locked) this.app.ed.toggle_lock(i);
    });
  }

  lockOthers() {
    const cur = this.state.layer;
    this.forEachLayer((l, i) => {
      if (l.locked !== (i !== cur)) this.app.ed.toggle_lock(i);
    });
  }

  hideOthers() {
    const cur = this.state.layer;
    this.forEachLayer((l, i) => {
      if (l.visible !== (i === cur)) this.app.ed.toggle_visible(i);
    });
  }

  /// Show Masking: locks the mask and its layers, like Flash.
  showMasking(mi) {
    const uid = this.state.layers[mi].uid;
    this.forEachLayer((l, i) => {
      if ((i === mi || l.parent === uid) && !l.locked) this.app.ed.toggle_lock(i);
    });
  }

  // ───────── Frame View button ─────────

  buildViewButton() {
    const b = document.createElement('button');
    b.className = 'ib tl-viewbtn';
    b.title = 'Vista de fotogramas';
    b.innerHTML = '<svg viewBox="0 0 20 20"><path d="M4 6h12M4 10h12M4 14h12"/></svg>';
    this.canvas.parentElement.appendChild(b);
    b.addEventListener('click', (e) => {
      e.stopPropagation();
      const r = b.getBoundingClientRect();
      this.app.contextMenu(r.left - 160, r.bottom, [
        ...SIZE_NAMES.map(([k, label]) => ({ label, check: this.view.size === k, action: () => this.setView({ size: k }) })),
        '-',
        { label: 'Corto', check: this.view.short, action: () => this.setView({ short: !this.view.short }) },
        { label: 'Vista previa', check: this.view.preview, action: () => this.setView({ preview: !this.view.preview }) },
      ]);
    });
  }

  // ───────── Frame grid (canvas) ─────────

  keyAt(li, f) {
    const l = this.state.layers[li];
    return l && f < l.length && l.keys.some((k) => k.f === f);
  }

  inSel(li, f) {
    const s = this.state.frameSel;
    return s && li >= s.l0 && li <= s.l1 && f >= s.f0 && f <= s.f1;
  }

  bindGrid() {
    const c = this.canvas;
    const pos = (e) => {
      const r = c.getBoundingClientRect();
      return { x: e.clientX - r.left, y: e.clientY - r.top };
    };
    c.addEventListener('pointerdown', (e) => {
      if (e.button !== 0 || !this.state) return;
      const st = this.state;
      const { x, y } = pos(e);
      const f = this.frameAt(x);
      this.app.stop();
      c.setPointerCapture(e.pointerId);
      if (y < RULER_H) {
        // Onion skin markers on the ruler.
        const m = this.markerAt(x);
        if (m) {
          this.drag = { type: 'marker', which: m };
          return;
        }
        this.app.ed.select_cell(st.layer, f);
        this.app.refresh();
        this.drag = { type: 'scrub', row: st.layer };
        return;
      }
      let row = this.rowAt(y);
      if (row < 0) row = st.layer;
      if (e.shiftKey && st.frameSel) {
        const s = st.frameSel;
        const al = s.l0 === row ? s.l1 : s.l0;
        const af = s.f0 === f ? s.f1 : s.f0;
        this.app.ed.select_frames(al, af, row, f);
        this.app.refresh();
        return;
      }
      // Dragging a selection that contains keyframes: move frames.
      if (this.inSel(row, f) && this.keyAt(row, f)) {
        this.drag = { type: 'move', f0: f, df: 0 };
        return;
      }
      this.app.ed.select_cell(row, f);
      this.app.refresh();
      this.drag = { type: 'range', row, f };
    });
    c.addEventListener('pointermove', (e) => {
      const d = this.drag;
      if (!d) return;
      const { x, y } = pos(e);
      const f = this.frameAt(x);
      const st = this.state;
      if (d.type === 'scrub') {
        if (f !== st.frame) {
          this.app.ed.select_cell(d.row, f);
          this.app.refresh();
        }
      } else if (d.type === 'marker') {
        const before = d.which === 'before' ? Math.max(0, st.frame - f) : st.onionBefore;
        const after = d.which === 'after' ? Math.max(0, f - st.frame) : st.onionAfter;
        this.app.cmd(() => this.app.ed.set_onion(before, after));
      } else if (d.type === 'range') {
        let row = this.rowAt(y);
        if (row < 0) row = d.row;
        const s = st.frameSel;
        if (!s || s.f1 !== Math.max(f, d.f) || s.f0 !== Math.min(f, d.f) || s.l1 !== Math.max(row, d.row) || s.l0 !== Math.min(row, d.row)) {
          this.app.ed.select_frames(d.row, d.f, row, f);
          this.app.refresh();
        }
      } else if (d.type === 'move') {
        d.df = f - d.f0;
        this.draw();
      }
    });
    c.addEventListener('pointerup', (e) => {
      const d = this.drag;
      this.drag = null;
      if (!d) return;
      if (d.type === 'move') {
        if (d.df) this.app.cmd(() => this.app.ed.move_frames(d.df));
        else {
          const { x, y } = pos(e);
          const row = this.rowAt(y);
          this.app.ed.select_cell(row < 0 ? this.state.layer : row, this.frameAt(x));
          this.app.refresh();
        }
      }
      this.draw();
    });
    c.addEventListener('wheel', (e) => {
      e.preventDefault();
      const horiz = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
      const overflowY = this.state && this.rowsH > this.h - RULER_H;
      if (horiz || !overflowY) this.scrollX += horiz ? (e.deltaX || e.deltaY) : e.deltaY;
      else this.scrollY += e.deltaY;
      this.clampScroll();
      this.draw();
    }, { passive: false });
    c.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      const { x, y } = pos(e);
      if (y >= RULER_H) {
        const row = this.rowAt(y);
        const f = this.frameAt(x);
        if (row >= 0 && !this.inSel(row, f)) {
          this.app.ed.select_cell(row, f);
          this.app.refresh();
        }
      }
      const tw = this.state.tween;
      const kind = tw ? tw.tweenKind : 'none';
      this.app.contextMenu(e.clientX, e.clientY, [
        kind === 'motion'
          ? { label: 'Quitar interpolación', cmd: 'removeTween' }
          : { label: 'Crear interpolación de movimiento', cmd: 'createTween', disabled: !tw },
        kind === 'shape'
          ? { label: 'Quitar interpolación de forma', cmd: 'removeTween' }
          : { label: 'Crear interpolación de forma', cmd: 'createShapeTween', disabled: !tw },
        '-',
        { label: 'Insertar fotograma', key: 'F5', cmd: 'insertFrame' },
        { label: 'Quitar fotogramas', key: 'Mayús+F5', cmd: 'removeFrame' },
        '-',
        { label: 'Insertar fotograma clave', key: 'F6', cmd: 'insertKeyframe' },
        { label: 'Insertar fotograma clave vacío', key: 'F7', cmd: 'insertBlankKeyframe' },
        { label: 'Borrar fotograma clave', key: 'Mayús+F6', cmd: 'clearKeyframe' },
        { label: 'Convertir en fotogramas clave', cmd: 'convertKeyframes' },
        { label: 'Convertir en fotogramas clave vacíos', cmd: 'convertBlankKeyframes' },
        '-',
        { label: 'Cortar fotogramas', key: 'Ctrl+Alt+X', cmd: 'cutFrames' },
        { label: 'Copiar fotogramas', key: 'Ctrl+Alt+C', cmd: 'copyFrames' },
        { label: 'Pegar fotogramas', key: 'Ctrl+Alt+V', cmd: 'pasteFrames', disabled: !this.state.frameClip },
        { label: 'Borrar fotogramas', key: 'Alt+Retroceso', cmd: 'clearFrames' },
        { label: 'Seleccionar todos los fotogramas', key: 'Ctrl+Alt+A', cmd: 'selectAllFrames' },
        '-',
        { label: 'Invertir fotogramas', cmd: 'reverseFrames' },
      ]);
    });
  }

  markerAt(x) {
    const st = this.state;
    if (!st || !(st.onion || st.editMultiple)) return null;
    const cw = this.cellW;
    const xb = (st.frame - st.onionBefore) * cw - this.scrollX + cw / 2;
    const xa = (st.frame + st.onionAfter) * cw - this.scrollX + cw / 2;
    if (Math.abs(x - xb) <= 5) return 'before';
    if (Math.abs(x - xa) <= 5) return 'after';
    return null;
  }

  thumb(li, f, w, h) {
    const key = `${this.thumbHash}:${li}:${f}:${w}x${h}`;
    if (!this.thumbs.has(key)) {
      const data = this.app.ed.render_thumb(li, f, w, h);
      const cv = document.createElement('canvas');
      cv.width = w;
      cv.height = h;
      if (data.length === w * h * 4) cv.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(data), w, h), 0, 0);
      if (this.thumbs.size > 400) this.thumbs.clear();
      this.thumbs.set(key, cv);
    }
    return this.thumbs.get(key);
  }

  draw() {
    const st = this.state;
    if (!st || !this.w || !this.rows) return;
    const ctx = this.ctx;
    const P = this.palette;
    const W = this.w, H = this.h;
    const CW = this.cellW;
    ctx.fillStyle = P.bg;
    ctx.fillRect(0, 0, W, H);
    if (this.view.preview) this.thumbHash = this.app.ed.doc_hash();

    const f0 = Math.floor(this.scrollX / CW);
    const f1 = f0 + Math.ceil(W / CW) + 1;
    const fx = (f) => f * CW - this.scrollX;
    ctx.font = '10px Segoe UI, system-ui, sans-serif';
    ctx.textBaseline = 'middle';

    for (const { li, y: ry, h: RH } of this.rows) {
      const l = st.layers[li];
      const y = RULER_H + ry - this.scrollY;
      if (y + RH < RULER_H || y > H) continue;
      if (l.kind === 'folder') {
        ctx.fillStyle = P.folder;
        ctx.fillRect(0, y, W, RH);
        ctx.fillStyle = P.rowLine;
        ctx.fillRect(0, y + RH - 1, W, 1);
        continue;
      }
      const keys = l.keys;
      let ki = -1;
      for (let f = f0; f < f1; f++) {
        while (ki + 1 < keys.length && keys[ki + 1].f <= f) ki++;
        const x = fx(f);
        let fill = f % 5 === 4 ? P.every5 : P.row;
        if (f < l.length && ki >= 0) {
          const k = keys[ki];
          fill = k.tween ? (k.shape ? P.shape : P.tween) : k.empty ? (f % 5 === 4 ? P.every5 : P.spanEmpty) : P.span;
        }
        ctx.fillStyle = fill;
        ctx.fillRect(x, y, CW, RH);
        if (CW >= 7) {
          ctx.fillStyle = P.line;
          ctx.fillRect(x + CW - 1, y, 1, RH);
        }
      }
      if (li === st.layer) {
        ctx.fillStyle = P.activeRow;
        ctx.fillRect(0, y, W, RH);
      }
      ctx.fillStyle = P.rowLine;
      ctx.fillRect(0, y + RH - 1, W, 1);

      keys.forEach((k, idx) => {
        if (k.f >= l.length) return;
        const end = Math.min(idx + 1 < keys.length ? keys[idx + 1].f : l.length, l.length);
        const last = end - 1;
        if (last < f0 - 1 || k.f > f1) return;
        const cy = y + RH - 7;
        // Sound: the waveform drawn along the span, as in Flash.
        if (k.sound && !this.view.preview) this.drawWave(ctx, k, end, y, RH, fx, f0, f1);
        // Preview: thumbnail of the span's content.
        if (this.view.preview && !k.empty) {
          const tw = Math.min(Math.max(CW * (end - k.f) - 2, 8), 60), th = RH - 4;
          ctx.drawImage(this.thumb(li, k.f, Math.round(tw), Math.round(th)), fx(k.f) + 1, y + 2);
        }
        if (k.tween || k.broken) {
          const xa = fx(k.f) + CW - 1, xb = fx(Math.max(last, k.f)) + CW / 2;
          const ly = y + RH / 2 - 1;
          ctx.strokeStyle = P.tweenLine;
          ctx.lineWidth = 1;
          ctx.setLineDash(k.broken ? [3, 2] : []);
          ctx.beginPath();
          ctx.moveTo(xa, ly + 0.5);
          ctx.lineTo(xb, ly + 0.5);
          ctx.stroke();
          ctx.setLineDash([]);
          if (k.tween && last > k.f) {
            ctx.fillStyle = P.tweenLine;
            ctx.beginPath();
            ctx.moveTo(xb + 2, ly + 0.5);
            ctx.lineTo(xb - 3, ly - 3);
            ctx.lineTo(xb - 3, ly + 4);
            ctx.fill();
          }
        }
        if (last > k.f && !k.tween) {
          const ex = fx(last) + CW / 2 - 3;
          ctx.fillStyle = k.empty ? P.keyHollowFill : P.span;
          ctx.fillRect(ex, cy - 4, 5, 8);
          ctx.strokeStyle = P.endRect;
          ctx.strokeRect(ex + 0.5, cy - 3.5, 4, 7);
        }
        const kx = fx(k.f) + CW / 2 - 0.5;
        // Frame with actions: Flash's "a".
        if (k.actions) {
          ctx.fillStyle = P.text;
          ctx.font = 'bold 9px Segoe UI, system-ui, sans-serif';
          ctx.textAlign = 'center';
          ctx.fillText('a', kx, y + 6);
          ctx.font = '10px Segoe UI, system-ui, sans-serif';
        }
        ctx.beginPath();
        ctx.arc(kx, cy, Math.min(3, CW / 2 - 0.5), 0, Math.PI * 2);
        if (k.empty) {
          ctx.fillStyle = P.keyHollowFill;
          ctx.fill();
          ctx.strokeStyle = P.key;
          ctx.lineWidth = 1;
          ctx.stroke();
        } else {
          ctx.fillStyle = P.key;
          ctx.fill();
        }
        // Labels: red flag (name), "//" (comment) or anchor.
        if (k.label) {
          const lx = fx(k.f) + 1, ly = y + 6;
          ctx.save();
          ctx.beginPath();
          ctx.rect(fx(k.f), y, (end - k.f) * CW - 1, RH);
          ctx.clip();
          let tx = lx + 7;
          if (k.labelKind === 'comment') {
            ctx.fillStyle = P.comment;
            ctx.textAlign = 'left';
            ctx.fillText('//', lx, ly);
            tx = lx + 10;
          } else {
            ctx.fillStyle = k.labelKind === 'anchor' ? '#d9a400' : P.label;
            ctx.fillRect(lx, ly - 4, 1.5, 9);
            ctx.beginPath();
            ctx.moveTo(lx + 1.5, ly - 4);
            ctx.lineTo(lx + 6, ly - 2);
            ctx.lineTo(lx + 1.5, ly);
            ctx.fill();
          }
          ctx.fillStyle = k.labelKind === 'comment' ? P.comment : P.text;
          ctx.textAlign = 'left';
          ctx.fillText(k.label, tx, ly);
          ctx.restore();
        }
      });
    }

    // Frame selection (range).
    const s = st.frameSel;
    if (s) {
      ctx.fillStyle = P.sel;
      ctx.globalAlpha = this.palette === PALETTES.classic ? 0.85 : 0.75;
      for (let li = s.l0; li <= s.l1; li++) {
        const r = this.rowOf(li);
        if (!r) continue;
        const y = RULER_H + r.y - this.scrollY;
        ctx.fillRect(fx(s.f0), y, (s.f1 - s.f0 + 1) * CW - 1, r.h - 1);
      }
      ctx.globalAlpha = 1;
      // Ghost while dragging frames.
      if (this.drag && this.drag.type === 'move' && this.drag.df) {
        ctx.strokeStyle = P.head;
        ctx.setLineDash([3, 2]);
        for (let li = s.l0; li <= s.l1; li++) {
          const r = this.rowOf(li);
          if (!r) continue;
          ctx.strokeRect(fx(s.f0 + this.drag.df) + 0.5, RULER_H + r.y - this.scrollY + 0.5, (s.f1 - s.f0 + 1) * CW - 1, r.h - 2);
        }
        ctx.setLineDash([]);
      }
    }

    // Ruler
    ctx.fillStyle = P.ruler;
    ctx.fillRect(0, 0, W, RULER_H);
    ctx.textAlign = 'center';
    const every = CW < 7 ? 10 : 5;
    for (let f = f0; f < f1; f++) {
      const x = fx(f);
      const major = (f + 1) % every === 0;
      ctx.fillStyle = P.rulerTick;
      if (CW >= 7 || major) ctx.fillRect(x + CW - 1, RULER_H - (major ? 7 : 4), 1, major ? 7 : 4);
      if (st.editingKind === 'button') {
        ctx.fillStyle = P.rulerText;
        ctx.fillText(BUTTON_STATES[f] || String(f + 1), x + CW / 2, 8);
      } else if (f === 0 || major) {
        ctx.fillStyle = P.rulerText;
        ctx.fillText(String(f + 1), x + CW / 2, 8);
      }
    }
    ctx.fillStyle = P.rowLine;
    ctx.fillRect(0, RULER_H - 1, W, 1);

    // Onion skin markers.
    if (st.onion || st.editMultiple) {
      const xb = fx(st.frame - st.onionBefore), xa = fx(st.frame + st.onionAfter) + CW;
      ctx.fillStyle = P.onion;
      ctx.fillRect(xb, 1, xa - xb, RULER_H - 3);
      ctx.strokeStyle = P.marker;
      ctx.lineWidth = 2;
      for (const [x, dir] of [[xb + 1, 1], [xa - 1, -1]]) {
        ctx.beginPath();
        ctx.moveTo(x + 4 * dir, 2);
        ctx.lineTo(x, 2);
        ctx.lineTo(x, RULER_H - 3);
        ctx.lineTo(x + 4 * dir, RULER_H - 3);
        ctx.stroke();
      }
      ctx.lineWidth = 1;
    }

    // Playhead
    const hx = fx(st.frame);
    ctx.fillStyle = P.head;
    ctx.globalAlpha = 0.9;
    ctx.fillRect(hx + CW / 2 - 1, RULER_H, 1, H);
    ctx.globalAlpha = 1;
    const label = st.editingKind === 'button' && BUTTON_STATES[st.frame] ? BUTTON_STATES[st.frame] : String(st.frame + 1);
    const tw = Math.max(CW, ctx.measureText(label).width + 6);
    ctx.fillStyle = P.head;
    ctx.fillRect(hx + CW / 2 - tw / 2 - 0.5, 1, tw, RULER_H - 3);
    ctx.fillStyle = '#fff';
    ctx.textAlign = 'center';
    ctx.fillText(label, hx + CW / 2 - 0.5, 9);
  }
}

// Waveform of a frame sound: each cell shows the chunk of audio that
// plays during that frame (until it ends or the span finishes).
Timeline.prototype.drawWave = function (ctx, k, end, y, RH, fx, f0, f1) {
  const st = this.state;
  const cache = (this.waves ||= new Map());
  const key = k.sound.id;
  if (!cache.has(key)) {
    const info = JSON.parse(this.app.ed.sound_info(key) || 'null');
    if (!info) return;
    cache.set(key, { info, peaks: this.app.ed.sound_peaks(key, Math.max(16, Math.round(info.seconds * 200))) });
  }
  const { info, peaks } = cache.get(key);
  const CW = this.cellW;
  const fps = st.fps || 12;
  const loops = k.sound.loops || 1;
  const total = info.seconds * loops;
  const mid = y + RH / 2, amp = RH / 2 - 2;
  ctx.fillStyle = '#2f6fd1';
  for (let f = Math.max(k.f, f0); f < Math.min(end, f1); f++) {
    const t0 = (f - k.f) / fps;
    if (t0 >= total) break;
    // One bar per cell pixel.
    for (let px = 0; px < CW - 1; px++) {
      const t = (t0 + px / (CW - 1) / fps) % info.seconds;
      const p = peaks[Math.min(peaks.length - 1, Math.floor(t / info.seconds * peaks.length))] || 0;
      const hgt = Math.max(1, p * amp);
      ctx.fillRect(fx(f) + px, mid - hgt, 1, hgt * 2);
    }
  }
};

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}
