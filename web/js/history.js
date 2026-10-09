// History panel and Commands menu (Flash 8): every step that changes the
// document is recorded with the calls that produced it, so it can be replayed,
// saved as a command and run again later.
// "Run Command…" also accepts a script file (.js / .jsfl) that receives
// `omf` (the application) and `ed` (the editor).
// The slider in the left margin undoes or redoes up to the chosen step;
// undone steps appear dimmed until something else is done.

const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

// Calls that change nothing or depend on the session: not recorded.
const IGNORE = new Set([
  'state', 'render', 'width', 'height', 'pixels_ptr', 'doc_coords', 'cursor_at', 'take_message', 'debug_frame', 'to_json',
  'export_swf', 'export_png', 'render_preview', 'preview_ptr', 'preview_width', 'preview_height', 'set_preview_mouse',
  'set_viewport', 'zoom_at', 'pan_by', 'set_zoom', 'fit_stage', 'is_dragging', 'pointer_leave', 'to_screen', 'movie_total',
  'doc_hash', 'symbol_frames', 'render_symbol_thumb', 'render_thumb', 'font_data', 'text_edit_info', 'publish_settings',
  'spell_items', 'commit_count', 'fps', 'set_temp_hand', 'register_font', 'undo', 'redo', 'load_json', 'new_document',
  'pen_undo', 'free', 'constructor', 'bitmap_asset_of', 'import_bitmap', 'undo_depth', 'redo_count', 'undo_floor', 'history_epoch',
  'history_go', 'sound_preview_pcm', 'sound_test_pcm', 'bitmap_png', 'scripts_list', 'script_code', 'check_script', 'as_keywords', 'doc_fonts',
  'text_select',
]);

const TOOL_STEP = {
  select: 'Mover', subselect: 'Mover anclas', transform: 'Transformar', filltransform: 'Transformar relleno', line: 'Línea',
  lasso: 'Lazo', pen: 'Pluma', text: 'Texto', oval: 'Óvalo', rect: 'Rectángulo', polystar: 'PolyStar', pencil: 'Lápiz',
  brush: 'Pincel', ink: 'Bote de tinta', bucket: 'Cubo de pintura', eyedropper: 'Cuentagotas', eraser: 'Borrador',
};
const CALL_STEP = {
  set_fill: 'Color de relleno', set_stroke: 'Color de trazo', set_stroke_width: 'Grosor de trazo', set_fill_gradient: 'Degradado',
  set_fill_bitmap: 'Relleno de mapa de bits', group: 'Agrupar', ungroup: 'Desagrupar', break_apart: 'Separar',
  delete_selection: 'Borrar', duplicate: 'Duplicar', cut: 'Cortar', paste: 'Pegar', flip: 'Voltear', rotate90: 'Girar 90°',
  arrange: 'Organizar', align: 'Alinear', nudge: 'Desplazar', set_sel_bounds: 'Tamaño y posición', smooth_selection: 'Suavizar',
  straighten_selection: 'Enderezar', lines_to_fills: 'Convertir líneas en rellenos', expand_fill: 'Expandir relleno',
  soften_edges: 'Suavizar bordes', optimize_curves: 'Optimizar curvas', insert_frames: 'Insertar fotograma',
  remove_frames: 'Quitar fotogramas', insert_keyframe: 'Insertar fotograma clave', convert_keyframes: 'Convertir en fotogramas clave',
  clear_keyframes: 'Borrar fotograma clave', set_tween_kind: 'Interpolación', add_layer: 'Insertar capa', delete_layer: 'Eliminar capa',
  convert_to_symbol: 'Convertir en símbolo', new_symbol: 'Nuevo símbolo', set_text_style: 'Formato de texto', text_commit: 'Escribir texto',
  set_color_fx: 'Efecto de color', add_filter: 'Añadir filtro', set_blend: 'Mezcla', place_symbol: 'Colocar símbolo',
  spell_replace: 'Revisar ortografía', set_document: 'Documento',
};
const POINTER = new Set(['pointer_down', 'pointer_move', 'pointer_up', 'double_click']);

export function wrapEditor(raw) {
  let rec = null; // { onStep }
  let buffer = [];
  let gesture = null; // tool name while dragging
  let pending = false;
  let replaying = 0;
  const bound = new Map();

  const toDoc = (x, y) => Array.from(raw.doc_coords(x, y));
  const finish = (name) => {
    if (!buffer.length) return;
    rec && rec.onStep({ name, calls: buffer, depth: raw.undo_depth(), epoch: raw.history_epoch() });
    buffer = [];
  };

  const handler = {
    get(t, prop) {
      const v = t[prop];
      if (typeof v !== 'function' || typeof prop !== 'string' || IGNORE.has(prop)) return typeof v === 'function' ? (bound.get(prop) || bound.set(prop, v.bind(t)).get(prop)) : v;
      if (!bound.has('rec:' + prop)) {
        bound.set('rec:' + prop, (...args) => {
          const before = t.commit_count();
          let call;
          if (POINTER.has(prop)) {
            if (prop === 'pointer_move' && !t.is_dragging()) return v.apply(t, args);
            if (prop === 'pointer_down') gesture = JSON.parse(t.state()).tool;
            const [x, y, ...rest] = args;
            call = { m: prop, p: toDoc(x, y), a: rest };
          } else {
            call = { m: prop, a: args.map((a) => (a instanceof Uint8Array ? null : a)) };
          }
          const out = v.apply(t, args);
          buffer.push(call);
          const changed = t.commit_count() !== before;
          // A gesture (press, drag, release) is a single step even if pressing changes something.
          if (POINTER.has(prop) && prop !== 'pointer_up' && prop !== 'double_click') {
            if (changed) pending = true;
          } else if (changed || pending) {
            pending = false;
            finish(POINTER.has(prop) ? TOOL_STEP[gesture] || 'Dibujar' : CALL_STEP[prop] || prop.replace(/_/g, ' '));
          } else if (prop === 'pointer_up' && buffer.length > 400) {
            buffer = []; // gestures with no effect: not accumulated
          }
          return out;
        });
      }
      return bound.get('rec:' + prop);
    },
  };
  const ed = new Proxy(raw, handler);

  /** Replays recorded calls (coordinates are in document space). */
  function replay(calls) {
    replaying++;
    try {
      const dpr = window.devicePixelRatio || 1;
      for (const c of calls) {
        if (c.p) {
          const s = raw.to_screen(c.p[0], c.p[1]);
          ed[c.m](s[0] * dpr, s[1] * dpr, ...c.a);
        } else if (typeof raw[c.m] === 'function') {
          ed[c.m](...c.a);
        }
      }
    } finally {
      replaying--;
    }
  }

  return { ed, replay, listen: (onStep) => (rec = { onStep }), get replaying() { return replaying > 0; } };
}

export function initHistory(app, wrapper) {
  const steps = [];
  let sel = new Set();
  let last = -1;
  const MAX = 100;

  wrapper.listen((step) => {
    // Undone steps are lost when a new step is taken (as in Flash).
    for (let i = steps.length - 1; i >= 0; i--) if (steps[i].depth >= step.depth || steps[i].epoch !== step.epoch) steps.splice(i, 1);
    steps.push(step);
    if (steps.length > MAX) steps.shift();
    sel = new Set();
    render();
  });

  const ed = () => wrapper.ed;
  /** Removes steps that can no longer be reached (new document, undo limit…). */
  function prune() {
    const e = ed();
    const cur = e.undo_depth(), top = cur + e.redo_count(), floor = e.undo_floor(), epoch = e.history_epoch();
    for (let i = steps.length - 1; i >= 0; i--) {
      const s = steps[i];
      if (s.epoch !== epoch || s.depth > top || s.depth < floor) steps.splice(i, 1);
    }
    return cur;
  }

  /** Moves the slider: undoes or redoes up to step `i` (−1: before the first). */
  function goTo(i) {
    if (!steps.length) return;
    const depth = i < 0 ? steps[0].depth - 1 : steps[i].depth;
    ed().history_go(depth);
    app.refresh();
    render();
  }

  const loadCmds = () => app.loadPref('omf.commands', { list: [] }).list;
  const saveCmds = (list) => app.savePref('omf.commands', { list });

  function build(host) {
    host.innerHTML = `<div class="hist-list" id="hist-list"></div>
      <div class="lib-actions">
        <button class="btn small" id="hist-replay" title="Repetir los pasos seleccionados">Repetir</button>
        <button class="ib" id="hist-save" title="Guardar los pasos seleccionados como comando"><svg viewBox="0 0 20 20"><path d="M4 3h10l2 2v12H4z"/><path d="M7 3v4h6V3M7 17v-5h6v5"/></svg></button>
        <div class="grow"></div>
        <button class="ib" id="hist-clear" title="Borrar el historial"><svg viewBox="0 0 20 20"><path d="M5 6h10l-1 11H6zM3.5 6h13M8 6V4h4v2"/></svg></button>
      </div>`;
    const listEl = host.querySelector('#hist-list');
    // Slider: press and drag in the margin.
    listEl.addEventListener('pointerdown', (e) => {
      if (!e.target.closest('.hist-gut')) return;
      e.preventDefault();
      const rowAt = (ev) => document.elementFromPoint(ev.clientX, ev.clientY)?.closest?.('[data-step],[data-hstart]');
      const go = (ev) => {
        const r = rowAt(ev);
        if (r) goTo(r.hasAttribute('data-hstart') ? -1 : +r.dataset.step);
      };
      go(e);
      const move = (ev) => go(ev);
      const up = () => { document.removeEventListener('pointermove', move); document.removeEventListener('pointerup', up); };
      document.addEventListener('pointermove', move);
      document.addEventListener('pointerup', up);
    });
    listEl.addEventListener('click', (e) => {
      if (e.target.closest('.hist-gut')) return;
      const r = e.target.closest('[data-step]');
      if (!r) return;
      const i = +r.dataset.step;
      if (e.shiftKey && last >= 0) {
        sel = new Set();
        for (let k = Math.min(i, last); k <= Math.max(i, last); k++) sel.add(k);
      } else if (e.ctrlKey || e.metaKey) {
        sel.has(i) ? sel.delete(i) : sel.add(i);
        last = i;
      } else {
        sel = new Set([i]);
        last = i;
      }
      render();
    });
    host.querySelector('#hist-replay').addEventListener('click', () => replaySelected());
    host.querySelector('#hist-save').addEventListener('click', () => saveDialog());
    host.querySelector('#hist-clear').addEventListener('click', () => {
      steps.length = 0;
      sel = new Set();
      render();
    });
    render();
  }

  let lastSig = '';
  function render() {
    const list = document.querySelector('#hist-list');
    if (!list) return;
    const cur = prune();
    // The slider sits next to the last step done.
    let at = -1;
    steps.forEach((s, i) => { if (s.depth <= cur) at = i; });
    const sig = JSON.stringify([steps.map((s) => s.depth + s.name), [...sel], at]);
    if (sig === lastSig) return;
    lastSig = sig;
    const row = (i, name, undone) => `<div class="hist-row${sel.has(i) ? ' sel' : ''}${undone ? ' undone' : ''}" ${i < 0 ? 'data-hstart' : `data-step="${i}"`}><span class="hist-gut">${i === at ? '<b class="hist-knob" title="Arrastra para deshacer o rehacer"></b>' : ''}</span><span class="hist-name">${name}</span></div>`;
    list.innerHTML = steps.length
      ? row(-1, '<i class="muted">Documento</i>', false) + steps.map((s, i) => row(i, esc(s.name), s.depth > cur)).join('')
      : '<div class="muted lib-empty">Aquí aparecen los pasos que vas dando.</div>';
    list.querySelector('.hist-knob')?.scrollIntoView({ block: 'nearest' });
  }

  const selected = () => [...sel].sort((a, b) => a - b).filter((i) => i >= 0).map((i) => steps[i]).filter(Boolean);

  function replaySelected() {
    const v = selected();
    if (!v.length) return app.toast('Selecciona uno o más pasos del historial');
    wrapper.replay(v.flatMap((s) => s.calls));
    app.refresh();
  }

  function saveDialog() {
    const v = selected();
    if (!v.length) return app.toast('Selecciona los pasos que quieres guardar');
    app.modal('Guardar como comando', `
      <div class="prow"><span class="lab" style="width:120px">Nombre del comando</span><input type="text" id="cmd-name" value="Comando ${loadCmds().length + 1}" style="width:200px"></div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="cmd-ok">Aceptar</button></div>`, (b) => {
      const inp = b.querySelector('#cmd-name');
      inp.focus();
      inp.select();
      const ok = () => {
        const name = inp.value.trim() || 'Comando';
        saveCmds(loadCmds().filter((c) => c.name !== name).concat([{ name, calls: v.flatMap((s) => s.calls) }]));
        app.closeModal();
        app.toast(`Comando guardado: ${name}`);
      };
      b.querySelector('#cmd-ok').addEventListener('click', ok);
      inp.addEventListener('keydown', (e) => e.key === 'Enter' && ok());
    });
  }

  function runSaved(name) {
    const c = loadCmds().find((x) => x.name === name);
    if (!c) return;
    wrapper.replay(c.calls);
    app.refresh();
  }

  function manageDialog() {
    const draw = (b) => {
      const list = loadCmds();
      b.querySelector('#cm-list').innerHTML = list.length
        ? list.map((c) => `<div class="lib-row" data-cmdname="${esc(c.name)}"><span class="lname">${esc(c.name)}</span><span class="lkind muted">${c.calls.length} pasos</span></div>`).join('')
        : '<div class="muted lib-empty">No hay comandos guardados. Selecciona pasos en el panel Historial y guárdalos.</div>';
    };
    let pick = null;
    app.modal('Administrar comandos guardados', `
      <div class="swap-list" id="cm-list"></div>
      <div class="modal-actions"><button class="btn" id="cm-rename">Cambiar nombre…</button><button class="btn" id="cm-delete">Eliminar</button><div class="grow"></div><button class="btn primary" data-close>Aceptar</button></div>`, (b) => {
      draw(b);
      b.querySelector('#cm-list').addEventListener('click', (e) => {
        const r = e.target.closest('[data-cmdname]');
        if (!r) return;
        pick = r.dataset.cmdname;
        b.querySelectorAll('[data-cmdname]').forEach((x) => x.classList.toggle('sel', x === r));
      });
      b.querySelector('#cm-delete').addEventListener('click', () => {
        if (!pick) return;
        saveCmds(loadCmds().filter((c) => c.name !== pick));
        pick = null;
        draw(b);
      });
      b.querySelector('#cm-rename').addEventListener('click', () => {
        if (!pick) return;
        const row = b.querySelector(`[data-cmdname="${CSS.escape(pick)}"] .lname`);
        row.innerHTML = `<input type="text" value="${esc(pick)}">`;
        const inp = row.querySelector('input');
        inp.focus();
        inp.select();
        const done = () => {
          const v = inp.value.trim();
          if (v && v !== pick) saveCmds(loadCmds().map((c) => (c.name === pick ? { ...c, name: v } : c)));
          pick = null;
          draw(b);
        };
        inp.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') done(); });
        inp.addEventListener('blur', done);
      });
    });
  }

  function runFileDialog() {
    const inp = document.createElement('input');
    inp.type = 'file';
    inp.accept = '.js,.jsfl,.json,.omfcmd';
    inp.hidden = true;
    inp.id = 'run-command-input';
    inp.addEventListener('change', async () => {
      const f = inp.files[0];
      inp.remove();
      if (f) runSource(await f.text(), f.name);
    });
    document.body.appendChild(inp);
    inp.click();
  }

  /** Runs a command: steps saved as JSON or a script. */
  function runSource(src, name = 'comando') {
    try {
      const t = src.trim();
      if (t.startsWith('{') || t.startsWith('[')) {
        const data = JSON.parse(t);
        wrapper.replay(Array.isArray(data) ? data : data.calls || []);
      } else {
        // eslint-disable-next-line no-new-func
        new Function('omf', 'ed', t)(window.omf, wrapper.ed);
      }
      app.refresh();
      app.toast(`Comando ejecutado: ${name}`);
    } catch (err) {
      app.toast(`Error en el comando: ${err.message || err}`);
    }
  }

  function menu() {
    const saved = loadCmds();
    return [
      ...saved.map((c) => ({ label: esc(c.name), action: () => runSaved(c.name) })),
      ...(saved.length ? ['-'] : []),
      { label: 'Administrar comandos guardados…', action: () => manageDialog() },
      { label: 'Ejecutar comando…', action: () => runFileDialog() },
    ];
  }

  return { build, menu, runSource, runSaved, steps, update: render, goTo, get selected() { return selected(); } };
}
