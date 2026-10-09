import init, { Editor } from '../pkg/omf_core.js';
import { ICONS, CURSORS } from './icons.js';
import { Timeline } from './timeline.js';
import { initSymbols, KIND_ICON } from './library.js';
import { GradientBar, gradCss, DEFAULT_GRADIENTS, rgbaHex, hexRgba } from './gradient.js';
import { initText } from './textui.js';
import { initIO } from './io.js';
import { wrapEditor, initHistory } from './history.js';
import { initSpell } from './spell.js';
import { initRulers } from './rulers.js';
import { initActions } from './actions.js';
import { initAudio, EFFECTS, envPreset, envGain } from './audio.js';
import { loadRuffle } from './ruffle.js';

const $ = (q, el = document) => el.querySelector(q);
const $$ = (q, el = document) => [...el.querySelectorAll(q)];
// In the single-file build (OMF.html) the .wasm is embedded.
const wasm = await init(globalThis.OMF_WASM ? { module_or_path: globalThis.OMF_WASM } : undefined);
// Editor calls go through the History panel log.
const recorder = wrapEditor(new Editor());
const ed = recorder.ed;
let S = null;
let docName = 'Sin título-1';
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);

// ───────────────────────── Tools (Flash 8 toolbox order) ─────────────────────────

const EXTRA_ICONS = {
  subselect: '<svg viewBox="0 0 20 20"><path d="M5 2.5v14l3.6-3.4 2.4 5.3 2.2-1-2.4-5.2 4.9-.3z"/></svg>',
  filltransform: '<svg viewBox="0 0 20 20"><rect x="3" y="3" width="14" height="14"/><path d="M3 10h14M10 3v14" stroke-dasharray="1.5 1.5"/></svg>',
  lasso: '<svg viewBox="0 0 20 20"><ellipse cx="11" cy="8" rx="6.5" ry="4.5"/><path d="M6 11c-1 2-1 4 1 5"/></svg>',
  pen: '<svg viewBox="0 0 20 20"><path d="M10 2.5l4.5 6.5L12 16H8L5.5 9z"/><path d="M10 2.5v6"/><circle cx="10" cy="10" r="1.3"/><path d="M8 18h4"/></svg>',
  text: '<svg viewBox="0 0 20 20"><path class="f" d="M4 3h12v3.5h-1.5L14 5h-3v10.5l2 .5V17H7v-1l2-.5V5H6l-.5 1.5H4z"/></svg>',
  polystar: '<svg viewBox="0 0 20 20"><path d="M10 2.5l2.2 4.6 5 .7-3.6 3.5.9 5-4.5-2.4-4.5 2.4.9-5L2.8 7.8l5-.7z"/></svg>',
};

// [id, name, shortcut, implemented]
const TOOLS = [
  ['select', 'Selección', 'V', true], ['subselect', 'Selección directa', 'A', true],
  ['transform', 'Transformación libre', 'Q', true], ['filltransform', 'Transformación de relleno', 'F', true],
  ['line', 'Línea', 'N', true], ['lasso', 'Lazo', 'L', true],
  ['pen', 'Pluma', 'P', true], ['text', 'Texto', 'T', true],
  ['oval', 'Óvalo', 'O', true], ['rect', 'Rectángulo', 'R', true],
  ['pencil', 'Lápiz', 'Y', true], ['brush', 'Pincel', 'B', true],
  ['ink', 'Bote de tinta', 'S', true], ['bucket', 'Cubo de pintura', 'K', true],
  ['eyedropper', 'Cuentagotas', 'I', true], ['eraser', 'Borrador', 'E', true],
];
const VIEW_TOOLS = [['hand', 'Mano', 'H', true], ['zoom', 'Zoom', 'Z', true]];
// PolyStar shares a slot with the Rectangle (chosen from its menu, as in Flash 8).
const ALL_TOOLS = [...TOOLS, ...VIEW_TOOLS, ['polystar', 'PolyStar', '', true]];
const TOOL_KEYS = Object.fromEntries(ALL_TOOLS.filter(([, , k]) => k).map(([id, , k]) => [k, id]));
const toolIcon = (id) => ICONS[id] || EXTRA_ICONS[id] || '';

const HINTS = {
  select: 'Clic: relleno o tramo · doble clic: todo lo conectado o entrar en un grupo · arrastra un borde para doblarlo o una esquina para moverla · el rectángulo corta las formas',
  subselect: 'Arrastra anclas, manejadores o segmentos · Mayús suma anclas · Alt+clic convierte el punto · Alt+arrastrar manejador lo rompe · Supr borra anclas',
  transform: 'Tiradores: escalar (Mayús = proporcional, Alt = desde el punto de transformación) · fuera de una esquina: rotar (Mayús = 15°) · arrastra el círculo para mover el pivote',
  line: 'Arrastra para trazar · Mayús restringe a 45°',
  pen: 'Clic = esquina · arrastrar = curva · Alt al arrastrar rompe el manejador · clic en el primer punto cierra · Intro/Esc termina · sobre un trazado seleccionado: añade/quita puntos',
  pencil: 'Dibuja a mano alzada con el color de trazo · el modo (Enderezar, Suavizar, Tinta) está en Opciones',
  brush: 'Pinta rellenos con el color de relleno · modo y forma de la punta en Opciones',
  rect: 'Arrastra · Mayús = cuadrado · Alt = desde el centro',
  oval: 'Arrastra · Mayús = círculo · Alt = desde el centro',
  bucket: 'Rellena formas o áreas cerradas por líneas · cierre de huecos en Opciones',
  ink: 'Clic en una forma para aplicar el color y grosor de trazo',
  eyedropper: 'Clic en una forma para tomar sus colores',
  eraser: 'Arrastra para borrar lo que tocas · modos y grifo en Opciones',
  hand: 'Arrastra para desplazar la vista (también Espacio)',
  zoom: 'Clic para acercar · Alt+clic para alejar',
  filltransform: 'Clic en un relleno degradado · arrastra el centro, el cuadrado (ancho), el círculo (giro o tamaño) o el triángulo (foco)',
  lasso: 'Rodea lo que quieras seleccionar (corta las formas) · modo polígono en Opciones: clic por vértice, doble clic o Intro para cerrar',
  text: 'Clic: texto de ancho automático · arrastrar: ancho fijo · clic en un texto para editarlo · Esc termina',
  polystar: 'Arrastra desde el centro · Mayús fija el giro · lados y estrella en Opciones',
};

function buildToolbar() {
  const tb = $('#toolbar');
  const btn = ([id, name, key, ok]) =>
    `<div class="tool${ok ? '' : ' disabled'}" data-tool="${id}" title="${name} (${key})${ok ? '' : ' — próximamente'}">${toolIcon(id)}</div>`;
  tb.innerHTML = `
    <div class="tb-label">Herram.</div>
    <div class="tb-group">${TOOLS.map(btn).join('')}</div>
    <div class="tb-label">Ver</div>
    <div class="tb-group">${VIEW_TOOLS.map(btn).join('')}</div>
    <div class="tb-label">Colores</div>
    <div class="cw-row" data-well="stroke" title="Color de trazo">${ICONS.pencil}<div class="chip" id="well-stroke"><i></i></div></div>
    <div class="cw-row" data-well="fill" title="Color de relleno">${ICONS.bucket}<div class="chip" id="well-fill"><i></i></div></div>
    <div class="well-mini">
      <button class="ib" data-cmd="defaultColors" title="Blanco y negro">${ICONS.bw}</button>
      <button class="ib" data-cmd="noColor" title="Sin color (en el color activo)">${ICONS.noColor}</button>
      <button class="ib" data-cmd="swapColors" title="Intercambiar colores">${ICONS.swap}</button>
    </div>
    <div class="tb-label">Opciones</div>
    <div class="tb-opts" id="tb-opts"></div>`;
  tb.addEventListener('click', (e) => {
    const t = e.target.closest('.tool');
    if (t && !t.classList.contains('disabled')) setTool(t.dataset.tool);
    else if (t) toast(`${t.title.split(' (')[0]}: llegará en la siguiente fase`);
  });
  // Rectangle and PolyStar: right-click or press and hold shows the other tool.
  const rectSlot = $('[data-tool="rect"]', tb);
  rectSlot.classList.add('flyout');
  const flyout = () => {
    const r = rectSlot.getBoundingClientRect();
    contextMenu(r.right + 2, r.top, [
      { label: 'Herramienta Rectángulo', key: 'R', check: S.tool === 'rect', action: () => setTool('rect') },
      { label: 'Herramienta PolyStar', check: S.tool === 'polystar', action: () => setTool('polystar') },
    ]);
  };
  rectSlot.addEventListener('contextmenu', (e) => { e.preventDefault(); flyout(); });
  let hold = 0;
  rectSlot.addEventListener('pointerdown', () => { hold = setTimeout(flyout, 450); });
  rectSlot.addEventListener('pointerup', () => clearTimeout(hold));
  rectSlot.addEventListener('pointerleave', () => clearTimeout(hold));
  for (const w of ['stroke', 'fill']) {
    $(`[data-well="${w}"]`).addEventListener('click', (e) => {
      activeWell = w;
      openWellPicker(w, e.currentTarget.querySelector('.chip'));
    });
  }
}

let activeWell = 'fill';

function setTool(id) {
  if (id === 'rect' || id === 'polystar') {
    const slot = $('#toolbar .flyout');
    slot.dataset.tool = id;
    slot.innerHTML = toolIcon(id);
    slot.title = id === 'rect' ? 'Rectángulo (R)' : 'PolyStar';
  }
  ed.set_tool(id);
  refresh();
}

// ───────────────────────── Stage rendering ─────────────────────────

const canvas = $('#stage');
const ctx = canvas.getContext('2d');
let renderQueued = false;

function requestRender() {
  if (renderQueued) return;
  renderQueued = true;
  requestAnimationFrame(() => {
    renderQueued = false;
    drawStage();
  });
}

function drawStage() {
  ed.render();
  const w = ed.width(), h = ed.height();
  if (canvas.width !== w || canvas.height !== h) return;
  const img = new ImageData(new Uint8ClampedArray(wasm.memory.buffer, ed.pixels_ptr(), w * h * 4), w, h);
  ctx.putImageData(img, 0, 0);
  drawHints();
  rulers?.draw();
}

// Shape hints: lettered circles (yellow at the start, green at the end).
function drawHints() {
  const marks = S && S.hintMarks;
  if (!marks) return;
  const dpr = window.devicePixelRatio || 1;
  ctx.save();
  ctx.scale(dpr, dpr);
  ctx.font = 'bold 10px Segoe UI, system-ui, sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  marks.points.forEach(([x, y], i) => {
    ctx.beginPath();
    ctx.arc(x, y, 7, 0, Math.PI * 2);
    ctx.fillStyle = marks.end ? '#4fc34a' : '#f2d21b';
    ctx.fill();
    ctx.strokeStyle = '#000';
    ctx.lineWidth = 1;
    ctx.stroke();
    ctx.fillStyle = '#000';
    ctx.fillText(String.fromCharCode(97 + i), x, y + 0.5);
  });
  ctx.restore();
}

const viewport = $('#viewport');
// The canvas fills the viewport minus the rulers (if shown).
new ResizeObserver(() => {
  const dpr = window.devicePixelRatio || 1;
  const r = canvas.getBoundingClientRect();
  const w = Math.max(1, Math.round(r.width * dpr)), h = Math.max(1, Math.round(r.height * dpr));
  canvas.width = w;
  canvas.height = h;
  ed.set_viewport(w, h, dpr);
  drawStage();
  if (S) updateZoomUI();
}).observe(canvas);

// ───────────────────────── Pointer on the Stage ─────────────────────────

const dprXY = (e) => {
  const r = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  return [(e.clientX - r.left) * dpr, (e.clientY - r.top) * dpr];
};

canvas.addEventListener('pointerdown', (e) => {
  closeMenus();
  if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur();
  stop();
  canvas.setPointerCapture(e.pointerId);
  const [x, y] = dprXY(e);
  ed.pointer_down(x, y, e.button, e.shiftKey, e.altKey);
  refresh();
  e.preventDefault();
});
canvas.addEventListener('pointermove', (e) => {
  const [x, y] = dprXY(e);
  const c = ed.doc_coords(x, y);
  $('#st-coords').textContent = `x: ${c[0].toFixed(1)}  y: ${c[1].toFixed(1)}`;
  rulers.mark(x / (window.devicePixelRatio || 1), y / (window.devicePixelRatio || 1));
  ed.pointer_move(x, y, e.shiftKey, e.altKey);
  // The Pen draws the "rubber band" to the pointer even when not dragging.
  if (ed.is_dragging() || S.penActive || S.simpleButtons) requestRender();
  const cur = ed.cursor_at(x, y);
  canvas.style.cursor = sizedCursor(cur) || CURSORS[cur] || EXTRA_CURSORS[cur] || cur;
});
// Click on the "Scene 1 › Group" path: returns to that level.
$('#crumbs').addEventListener('click', (e) => {
  const c = e.target.closest('.crumb');
  if (!c) return;
  const d = +c.dataset.depth, nsym = S.symPath.length;
  if (d <= nsym) {
    // Scene or symbol: closes the groups and symbols below it.
    ed.leave_all_groups();
    for (let n = nsym - d; n > 0; n--) ed.leave_symbol();
  } else {
    let n = S.scopes.length - (d - nsym);
    while (n-- > 0) ed.leave_group();
  }
  refresh();
});

// Double-click: enter groups/objects, select connected shapes, exit the group.
canvas.addEventListener('dblclick', (e) => {
  const [x, y] = dprXY(e);
  ed.double_click(x, y);
  refresh();
});
canvas.addEventListener('pointerleave', () => {
  ed.pointer_leave();
  rulers.mark(null);
  if (S.penActive) requestRender();
});

const EXTRA_CURSORS = { lasso: 'crosshair', 'skew-x': 'col-resize', 'skew-y': 'row-resize' };

// Brush and Eraser show their real size in the cursor, as in Flash.
const cursorCache = new Map();
function sizedCursor(name) {
  const painting = (S.tool === 'brush' && name === 'crosshair') || name === 'eraser';
  if (!painting) return null;
  const size = Math.round(S.tool === 'brush' ? S.brushSize : S.eraserSize);
  const tip = S.tool === 'brush' ? S.brushTip : S.eraserTip;
  if (size > 120) return null;
  const key = `${tip}:${size}`;
  if (!cursorCache.has(key)) {
    const d = Math.max(size, 3) + 4, c = d / 2, r = Math.max(size, 3) / 2;
    const shape = {
      square: `<rect x='${c - r}' y='${c - r}' width='${2 * r}' height='${2 * r}'/>`,
      oval: `<ellipse cx='${c}' cy='${c}' rx='${r}' ry='${Math.max(r * 0.35, 1)}'/>`,
      slash: `<path d='M${c - r} ${c + r}L${c + r} ${c - r}'/>`,
    }[tip] || `<circle cx='${c}' cy='${c}' r='${r}'/>`;
    const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${d}' height='${d}'><g fill='none' stroke='white' stroke-width='3'>${shape}</g><g fill='none' stroke='black' stroke-width='1'>${shape}</g></svg>`;
    cursorCache.set(key, `url("data:image/svg+xml,${encodeURIComponent(svg)}") ${Math.round(c)} ${Math.round(c)}, crosshair`);
  }
  return cursorCache.get(key);
}
canvas.addEventListener('pointerup', (e) => {
  if (!ed.is_dragging()) return;
  const [x, y] = dprXY(e);
  ed.pointer_up(x, y, e.shiftKey, e.altKey);
  refresh();
});
canvas.addEventListener('pointercancel', (e) => {
  const [x, y] = dprXY(e);
  if (ed.is_dragging()) ed.pointer_up(x, y, false, false);
  refresh();
});
canvas.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  const hasSel = !!S.sel;
  contextMenu(e.clientX, e.clientY, [
    { label: 'Cortar', key: 'Ctrl+X', cmd: 'cut', disabled: !hasSel },
    { label: 'Copiar', key: 'Ctrl+C', cmd: 'copy', disabled: !hasSel },
    { label: 'Pegar', key: 'Ctrl+V', cmd: 'paste', disabled: !S.clipboard },
    { label: 'Pegar in situ', key: 'Ctrl+Mayús+V', cmd: 'pasteInPlace', disabled: !S.clipboard },
    '-',
    { label: 'Seleccionar todo', key: 'Ctrl+A', cmd: 'selectAll' },
    { label: 'Anular todas las selecciones', key: 'Ctrl+Mayús+A', cmd: 'deselect' },
    '-',
    { label: 'Organizar', sub: ARRANGE_MENU, disabled: !hasSel },
    { label: 'Transformar', sub: TRANSFORM_MENU, disabled: !hasSel },
    { label: 'Forma', sub: [
      { label: 'Suavizar', cmd: 'smooth' }, { label: 'Enderezar', cmd: 'straighten' },
      { label: 'Optimizar…', cmd: 'optimizeDialog' }, '-',
      { label: 'Convertir líneas en rellenos', cmd: 'linesToFills' },
      { label: 'Expandir relleno…', cmd: 'expandDialog' },
      { label: 'Suavizar bordes de relleno…', cmd: 'softenDialog' },
    ], disabled: !hasSel },
    '-',
    { label: 'Agrupar', key: 'Ctrl+G', cmd: 'group', disabled: !hasSel },
    { label: 'Desagrupar', key: 'Ctrl+Mayús+G', cmd: 'ungroup', disabled: !S.selGroup },
    { label: 'Separar', key: 'Ctrl+B', cmd: 'breakApart', disabled: !(S.selGroup || S.selObject || S.selInstance) },
    ...(S.selInstance ? [
      '-',
      { label: 'Editar', cmd: 'editSymbols' },
      { label: 'Editar en contexto', cmd: 'editSelected' },
      { label: 'Intercambiar símbolo…', cmd: 'swapSymbol', disabled: !S.inst },
    ] : [{ label: 'Editar selección', cmd: 'editSelected', disabled: !(S.selGroup || S.selObject) }]),
    { label: 'Convertir en símbolo…', key: 'F8', cmd: 'convertSymbol', disabled: !hasSel },
    ...(S.scopes.length ? [{ label: 'Salir del grupo', cmd: 'leaveGroup' }] : []),
    ...(S.symPath.length ? [{ label: 'Editar documento', key: 'Ctrl+E', cmd: 'editDocument' }] : []),
    '-',
    { label: 'Documento…', key: 'Ctrl+J', cmd: 'docProps' },
  ]);
});
canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  const [x, y] = dprXY(e);
  const dpr = window.devicePixelRatio || 1;
  if (e.ctrlKey || e.metaKey) ed.zoom_at(x, y, Math.exp(-e.deltaY * 0.0025));
  else if (e.shiftKey) ed.pan_by(-e.deltaY * dpr, 0);
  else ed.pan_by(-e.deltaX * dpr, -e.deltaY * dpr);
  requestRender();
  updateZoomUI();
}, { passive: false });

// ───────────────────────── State → UI ─────────────────────────

let autosaveTimer = 0;
function refresh() {
  S = JSON.parse(ed.state());
  timeline.update(S);
  updateToolbar();
  updateOptionsBar();
  updatePanels();
  updateReadouts();
  updateZoomUI();
  hist && hist.update();
  requestRender();
  const msg = ed.take_message();
  if (msg) toast(msg);
  clearTimeout(autosaveTimer);
  autosaveTimer = setTimeout(autosave, 800);
}

function cmd(fn) {
  fn();
  refresh();
}

function updateToolbar() {
  $$('#toolbar .tool').forEach((t) => t.classList.toggle('active', t.dataset.tool === S.tool));
  paintChip($('#well-stroke'), S.strokeOn ? S.stroke : null, S.strokeAlpha);
  paintFill($('#well-fill'));
  $$('.cw-row').forEach((r) => r.style.background = r.dataset.well === activeWell ? '#3d3d3d' : '');
  const opts = $('#tb-opts');
  const sig = [S.tool, S.pencilMode, S.brushMode, S.brushTip, S.eraserMode, S.eraserTip, S.faucet, S.gap, S.objectMode, S.snapObjects, S.lockFill, S.lassoPoly, S.xformMode].join();
  if (opts.dataset.sig === sig) return;
  opts.dataset.sig = sig;
  const modeBtn = (tool) => {
    const m = MODES[tool];
    const cur = m.items.find(([v]) => v === S[m.key]);
    return `<button class="ib" data-modemenu="${tool}" title="${m.label}: ${cur ? cur[1] : ''}">${MODE_ICONS[tool](S[m.key])}</button>`;
  };
  const lockBtn = `<button class="ib${S.lockFill ? ' on' : ''}" data-cmd="lockFill" title="Bloquear relleno: el degradado se ajusta al escenario y continúa entre formas">${OPT_ICONS.lock}</button>`;
  const tipBtn = (tool) => `<button class="ib" data-tipmenu="${tool}" title="Forma de la punta">${TIP_ICONS[S[tool + 'Tip']]}</button>`;
  const o = {
    pencil: modeBtn('pencil'),
    brush: modeBtn('brush') + tipBtn('brush') + lockBtn,
    eraser: modeBtn('eraser') + `<button class="ib${S.faucet ? ' on' : ''}" data-cmd="toggleFaucet" title="Grifo: borra de un clic un relleno o un trazo entero">${OPT_ICONS.faucet}</button>` + tipBtn('eraser'),
    bucket: modeBtn('bucket') + lockBtn,
    lasso: `<button class="ib${S.lassoPoly ? ' on' : ''}" data-cmd="lassoPoly" title="Modo polígono">${OPT_ICONS.polygon}</button>`,
    polystar: `<button class="ib" data-cmd="polystarDialog" title="Opciones de PolyStar">${OPT_ICONS.polyopts}</button>`,
    zoom: `<button class="ib" data-cmd="zoomIn" title="Acercar">${ICONS.zoom}</button><button class="ib" data-cmd="zoomOut" title="Alejar"><svg viewBox="0 0 20 20"><circle cx="8.5" cy="8.5" r="5"/><path d="M12.2 12.2L17 17M6.5 8.5h4"/></svg></button>`,
    rect: `<button class="ib" data-cmd="rectRadius" title="Radio de esquina del rectángulo"><svg viewBox="0 0 20 20"><path d="M4 16V9a5 5 0 015-5h7"/></svg></button>`,
    transform: XFORM_MODES.map(([m, label, icon]) => `<button class="ib${S.xformMode === m ? ' on' : ''}" data-xmode="${m}" title="${label}">${icon}</button>`).join('') + `<button class="ib" data-cmd="rot90ccw" title="Girar 90° a la izquierda"><svg viewBox="0 0 20 20"><path d="M6 6a6 6 0 11-1.5 6"/><path d="M3 3v4h4"/></svg></button><button class="ib" data-cmd="rot90cw" title="Girar 90° a la derecha"><svg viewBox="0 0 20 20"><path d="M14 6a6 6 0 101.5 6"/><path d="M17 3v4h-4"/></svg></button><button class="ib" data-cmd="flipH" title="Voltear horizontal"><svg viewBox="0 0 20 20"><path d="M10 2v16" stroke-dasharray="2 2"/><path d="M8 5L3 15h5zM12 5l5 10h-5z"/></svg></button><button class="ib" data-cmd="flipV" title="Voltear vertical"><svg viewBox="0 0 20 20"><path d="M2 10h16" stroke-dasharray="2 2"/><path d="M5 8L15 3v5zM5 12l10 5v-5z"/></svg></button>`,
  };
  // Flash buttons in "Options": Snap, Object Drawing (J), Smooth and Straighten.
  const magnet = `<button class="ib${S.snapObjects ? ' on' : ''}" data-cmd="snapObjects" title="Ajustar a objetos (Ctrl+Mayús+/)">${OPT_ICONS.magnet}</button>`;
  const objMode = `<button class="ib${S.objectMode ? ' on' : ''}" data-cmd="toggleObjectMode" title="Dibujo de objetos (J)">${OPT_ICONS.object}</button>`;
  const drawing = ['line', 'pen', 'pencil', 'brush', 'rect', 'oval'].includes(S.tool);
  let html = o[S.tool] || '';
  if (drawing) html = objMode + html;
  if (S.tool === 'select') html = magnet + `<button class="ib" data-cmd="smooth" title="Suavizar">${OPT_ICONS.smooth}</button><button class="ib" data-cmd="straighten" title="Enderezar">${OPT_ICONS.straighten}</button>`;
  else if (drawing || S.tool === 'transform') html += magnet;
  opts.innerHTML = html;
  $$('[data-modemenu]', opts).forEach((b) => b.addEventListener('click', (e) => {
    e.stopPropagation();
    openModeMenu(b.dataset.modemenu, b);
  }));
  $$('[data-xmode]', opts).forEach((b) => b.addEventListener('click', () => cmd(() => ed.set_xform_mode(S.xformMode === b.dataset.xmode ? 'free' : b.dataset.xmode))));
  $$('[data-tipmenu]', opts).forEach((b) => b.addEventListener('click', (e) => {
    e.stopPropagation();
    openTipMenu(b.dataset.tipmenu, b);
  }));
}

// Flash 8 tool modes ("Options" buttons and top bar).
const MODES = {
  pencil: { key: 'pencilMode', set: 'set_pencil_mode', label: 'Modo del lápiz',
    items: [['straighten', 'Enderezar'], ['smooth', 'Suavizar'], ['ink', 'Tinta']] },
  brush: { key: 'brushMode', set: 'set_brush_mode', label: 'Modo del pincel',
    items: [['normal', 'Pintar normal'], ['fills', 'Pintar rellenos'], ['behind', 'Pintar detrás'], ['selection', 'Pintar selección'], ['inside', 'Pintar dentro']] },
  eraser: { key: 'eraserMode', set: 'set_eraser_mode', label: 'Modo del borrador',
    items: [['normal', 'Borrar normal'], ['fills', 'Borrar rellenos'], ['lines', 'Borrar líneas'], ['selected', 'Borrar rellenos seleccionados'], ['inside', 'Borrar dentro']] },
  bucket: { key: 'gap', set: 'set_gap', label: 'Tamaño del hueco',
    items: [[0, 'No cerrar huecos'], [1, 'Cerrar huecos pequeños'], [2, 'Cerrar huecos medianos'], [3, 'Cerrar huecos grandes']] },
};
const TIPS = [['round', 'Redonda'], ['square', 'Cuadrada'], ['oval', 'Óvalo horizontal'], ['slash', 'Diagonal']];
const svg20 = (b) => `<svg viewBox="0 0 20 20">${b}</svg>`;
const TIP_ICONS = {
  round: svg20('<circle cx="10" cy="10" r="5" class="f"/>'),
  square: svg20('<rect x="5" y="5" width="10" height="10"/>'),
  oval: svg20('<ellipse cx="10" cy="10" rx="7" ry="2.5" class="f"/>'),
  slash: svg20('<path d="M5 15L15 5" stroke-width="2.5"/>'),
};
const PAINT_MODE = (m) => svg20({
  normal: '<rect x="3" y="3" width="9" height="9"/><path d="M8 17c3 0 4-2 4-4l5-6" stroke-width="2.4"/>',
  fills: '<rect x="3" y="3" width="9" height="9" class="f" opacity=".5"/><path d="M8 17c3 0 4-2 4-4l5-6" stroke-width="2.4"/>',
  behind: '<path d="M4 15c3 0 4-2 4-4l5-6" stroke-width="2.4" opacity=".5"/><rect x="8" y="8" width="9" height="9" class="f"/>',
  selection: '<rect x="3" y="3" width="10" height="10" stroke-dasharray="2 1.5"/><path d="M8 17c3 0 4-2 4-4l5-6" stroke-width="2.4"/>',
  inside: '<circle cx="9" cy="10" r="6"/><path d="M6 10c1.5 0 2.5-1 3-2l2-1" stroke-width="2.4"/>',
  lines: '<path d="M3 16L16 3"/><path d="M5 9h10" stroke-width="2.6" opacity=".55"/>',
  selected: '<rect x="3" y="3" width="10" height="10" stroke-dasharray="2 1.5"/><path d="M6 15l9-9" stroke-width="2.6" opacity=".55"/>',
}[m] || '');
const MODE_ICONS = {
  pencil: (m) => svg20({
    straighten: '<path d="M3 15l5-9 4 7 5-9"/>',
    smooth: '<path d="M3 14c3-10 6 6 14-6"/>',
    ink: '<path d="M3 13c1-2 2 1 3-1s2-3 3-1 2 2 3 0 2-3 5-2"/>',
  }[m]),
  brush: PAINT_MODE,
  eraser: PAINT_MODE,
  bucket: (g) => svg20(`<path d="M10 4a6 6 0 1 0 6 6" stroke-dasharray="${['', '1 1', '1 2.5', '1 4'][g]}"/><circle cx="10" cy="10" r="2" class="f"/>`),
};
const XFORM_MODES = [
  ['rotate', 'Rotar y sesgar', svg20('<path d="M5 9a5 5 0 019-3"/><path d="M14 2v4h-4"/><path d="M4 17l3-5h9l-3 5z"/>')],
  ['scale', 'Escalar', svg20('<rect x="3" y="9" width="8" height="8"/><path d="M9 3h8v8M17 3l-7 7"/>')],
  ['distort', 'Distorsionar', svg20('<path d="M4 4l13 2-3 11-11-3z"/><rect class="f" x="15.5" y="15.5" width="3" height="3"/>')],
  ['envelope', 'Envoltura', svg20('<path d="M3 5c4-2 10 2 14 0v10c-4 2-10-2-14 0z"/><circle class="f" cx="10" cy="5" r="1.4"/>')],
];
const OPT_ICONS = {
  lock: svg20('<rect x="4.5" y="9" width="11" height="8" rx="1"/><path d="M7 9V6.5a3 3 0 016 0V9"/>'),
  polygon: svg20('<path d="M4 15l2-10 9 2 2 8z"/><circle class="f" cx="4" cy="15" r="1.3"/><circle class="f" cx="6" cy="5" r="1.3"/><circle class="f" cx="15" cy="7" r="1.3"/>'),
  polyopts: svg20('<path d="M10 3l2 4.2 4.6.6-3.3 3.2.8 4.6L10 13.4 5.9 15.6l.8-4.6L3.4 7.8 8 7.2z"/>'),
  magnet: svg20('<path d="M5 4v6a5 5 0 0010 0V4h-3.5v6a1.5 1.5 0 01-3 0V4z"/><path d="M5 7h3.5M11.5 7H15" stroke-width="2.2"/>'),
  object: svg20('<rect x="3.5" y="3.5" width="13" height="13" stroke-dasharray="2 1.5"/><circle cx="10" cy="10" r="3.6" class="f"/>'),
  smooth: svg20('<path d="M3 14c3-9 7 4 14-6"/><path d="M3 5h2M15 15h2" opacity=".5"/>'),
  straighten: svg20('<path d="M3 15l5-9 4 7 5-9"/>'),
  faucet: svg20('<path d="M3 7h9v4H3z"/><path d="M12 8h3a2 2 0 012 2v2"/><path d="M7 4v3M5 4h4"/><path class="f" d="M17 14c.8 1.2 1.2 2 1.2 2.6a1.2 1.2 0 01-2.4 0c0-.6.4-1.4 1.2-2.6z"/>'),
};

function openModeMenu(tool, anchor) {
  const m = MODES[tool];
  const r = anchor.getBoundingClientRect();
  contextMenu(r.right + 4, r.top, m.items.map(([v, label]) => ({
    label, check: S[m.key] === v, action: () => cmd(() => ed[m.set](v)),
  })));
}

function openTipMenu(tool, anchor) {
  const r = anchor.getBoundingClientRect();
  contextMenu(r.right + 4, r.top, TIPS.map(([v, label]) => ({
    label, check: S[tool + 'Tip'] === v, action: () => cmd(() => ed[`set_${tool}_tip`](v)),
  })));
}

const modeSelect = (tool) => `<label>${MODES[tool].label.replace(/ del? .*/, '')} <select data-modesel="${tool}">${MODES[tool].items.map(([v, l]) => `<option value="${v}">${l}</option>`).join('')}</select></label>`;
const tipSelect = (tool) => `<label>Punta <select data-tipsel="${tool}">${TIPS.map(([v, l]) => `<option value="${v}">${l}</option>`).join('')}</select></label>`;

function paintChip(el, hex, alpha = 255) {
  if (!el) return;
  const i = el.querySelector('i') || el.appendChild(document.createElement('i'));
  el.classList.toggle('none-swatch', !hex);
  el.classList.toggle('checker', !!hex && alpha < 255);
  i.style.background = hex ? hexA(hex, alpha) : 'transparent';
}

/** Thumbnail (data URL) of a Library bitmap. */
const thumbs = new Map();
function bitmapThumb(id, n = 40) {
  if (!thumbs.has(id)) {
    const px = ed.render_symbol_thumb(id, 0, n, n);
    const c = document.createElement('canvas');
    c.width = c.height = n;
    if (px.length === n * n * 4) c.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(px), n, n), 0, 0);
    thumbs.set(id, c.toDataURL());
  }
  return thumbs.get(id);
}

/** Swatch of the active fill: color, gradient or bitmap. */
function paintFill(el) {
  if (!el) return;
  const bsym = S.fillOn && S.fillBmp && S.library.find((x) => x.bitmap === S.fillBmp);
  if (bsym) {
    const i = el.querySelector('i') || el.appendChild(document.createElement('i'));
    el.classList.remove('none-swatch', 'checker');
    i.style.background = `center / cover url(${bitmapThumb(bsym.id)})`;
  } else if (S.fillOn && S.fillGrad) {
    const i = el.querySelector('i') || el.appendChild(document.createElement('i'));
    el.classList.remove('none-swatch', 'checker');
    i.style.background = gradCss(S.fillGrad);
  } else paintChip(el, S.fillOn ? S.fill : null, S.fillAlpha);
}

const hexA = (hex, a = 255) => {
  const n = parseInt(hex.slice(1), 16);
  return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${(a / 255).toFixed(3)})`;
};

// Contextual options bar (Photopea style)
function updateOptionsBar() {
  const bar = $('#optionsbar');
  const sig = S.tool;
  if (bar.dataset.sig !== sig) {
    bar.dataset.sig = sig;
    const t = ALL_TOOLS.find((x) => x[0] === S.tool);
    let html = `<div class="tool-name">${toolIcon(S.tool)}<span>${t ? t[1] : ''}</span></div>`;
    const strokeW = `<label>Trazo <input type="number" id="ob-sw" min="0.1" max="200" step="0.5"></label>`;
    const chips = (which) => which.map((w) => `<label>${w === 'stroke' ? 'Color trazo' : 'Relleno'} <div class="chip" data-obchip="${w}"><i></i></div></label>`).join('');
    switch (S.tool) {
      case 'line': html += strokeW + chips(['stroke']); break;
      case 'pencil': html += modeSelect('pencil') + strokeW + chips(['stroke']); break;
      case 'pen': html += chips(['stroke', 'fill']) + strokeW + `<span class="muted">El relleno se aplica al cerrar el trazado</span>`; break;
      case 'polystar': html += chips(['stroke', 'fill']) + strokeW + `<button class="btn small" data-cmd="polystarDialog">Opciones…</button><span class="muted" id="ob-poly"></span>`; break;
      case 'lasso': html += `<label><input type="checkbox" id="ob-lassopoly"> Modo polígono</label><span class="muted">Doble clic o Intro cierra el polígono</span>`; break;
      case 'filltransform': html += `<span class="muted">Haz clic en un relleno degradado para ver su cuadro de degradado</span>`; break;
      case 'text': html += `<span class="muted">Formato y color en el panel Propiedades y en el menú Texto</span>`; break;
      case 'subselect': html += `<button class="btn small" data-cmd="clear" title="Supr">Borrar anclas</button><span class="muted">Alt+clic en un ancla: esquina ↔ suave</span>`; break;
      case 'ink': html += chips(['stroke']) + strokeW; break;
      case 'rect': html += chips(['stroke', 'fill']) + strokeW + `<label>Radio de esquina <input type="number" id="ob-radius" min="0" max="999" step="1"></label>`; break;
      case 'oval': html += chips(['stroke', 'fill']) + strokeW; break;
      case 'brush': html += modeSelect('brush') + tipSelect('brush') + `<label>Tamaño <input type="range" id="ob-brush-r" min="1" max="100"><input type="number" id="ob-brush" min="1" max="200"></label>` + chips(['fill']); break;
      case 'eraser': html += modeSelect('eraser') + tipSelect('eraser') + `<label>Tamaño <input type="range" id="ob-eraser-r" min="1" max="100"><input type="number" id="ob-eraser" min="1" max="200"></label><label><input type="checkbox" id="ob-faucet"> Grifo</label>`; break;
      case 'bucket': html += modeSelect('bucket') + chips(['fill']); break;
      case 'select': case 'transform':
        html += `<button class="btn small" data-cmd="front" title="Traer al frente (Ctrl+Mayús+↑)">Al frente</button>
          <button class="btn small" data-cmd="back" title="Enviar al fondo (Ctrl+Mayús+↓)">Al fondo</button><span class="sep"></span>
          <button class="btn small" data-cmd="flipH">Voltear H</button><button class="btn small" data-cmd="flipV">Voltear V</button>
          <button class="btn small" data-cmd="rot90cw">Girar 90°</button>`;
        break;
      case 'zoom': case 'hand':
        html += `<button class="btn small" data-cmd="zoom100">100%</button><button class="btn small" data-cmd="zoomFit">Mostrar fotograma</button>`;
        break;
    }
    bar.innerHTML = html;
    bind('#ob-sw', 'change', (e) => cmd(() => ed.set_stroke_width(+e.target.value || 1)));
    bind('#ob-radius', 'change', (e) => cmd(() => ed.set_rect_radius(+e.target.value || 0)));
    bind('#ob-brush', 'change', (e) => cmd(() => ed.set_brush_size(+e.target.value || 1)));
    bind('#ob-brush-r', 'input', (e) => cmd(() => ed.set_brush_size(+e.target.value)));
    bind('#ob-eraser', 'change', (e) => cmd(() => ed.set_eraser_size(+e.target.value || 1)));
    bind('#ob-eraser-r', 'input', (e) => cmd(() => ed.set_eraser_size(+e.target.value)));
    bind('#ob-faucet', 'change', (e) => cmd(() => ed.set_faucet(e.target.checked)));
    bind('#ob-lassopoly', 'change', (e) => cmd(() => ed.set_lasso_polygon(e.target.checked)));
    $$('[data-modesel]', bar).forEach((s) => s.addEventListener('change', () => {
      const m = MODES[s.dataset.modesel];
      cmd(() => ed[m.set](m.key === 'gap' ? +s.value : s.value));
      s.blur();
    }));
    $$('[data-tipsel]', bar).forEach((s) => s.addEventListener('change', () => {
      cmd(() => ed[`set_${s.dataset.tipsel}_tip`](s.value));
      s.blur();
    }));
    $$('[data-obchip]', bar).forEach((c) => c.addEventListener('click', () => {
      activeWell = c.dataset.obchip;
      openWellPicker(c.dataset.obchip, c);
    }));
  }
  setVal('#ob-sw', S.strokeWidth);
  setVal('#ob-radius', S.rectRadius);
  setVal('#ob-brush', S.brushSize);
  setVal('#ob-brush-r', S.brushSize);
  setVal('#ob-eraser', S.eraserSize);
  setVal('#ob-eraser-r', S.eraserSize);
  setVal('#ob-faucet', S.faucet);
  setVal('#ob-lassopoly', S.lassoPoly);
  if ($('#ob-poly')) $('#ob-poly').textContent = `${S.polystar.style === 'star' ? 'Estrella' : 'Polígono'} de ${S.polystar.sides} lados`;
  $$('[data-modesel]', bar).forEach((s) => setVal(s, String(S[MODES[s.dataset.modesel].key])));
  $$('[data-tipsel]', bar).forEach((s) => setVal(s, S[s.dataset.tipsel + 'Tip']));
  $$('[data-obchip]', bar).forEach((c) => {
    const w = c.dataset.obchip;
    if (w === 'fill') paintFill(c);
    else paintChip(c, S[w + 'On'] ? S[w] : null, S[w + 'Alpha']);
  });
  $('#st-hint').textContent = HINTS[S.tool] || '';
}

function bind(q, ev, fn) {
  const el = typeof q === 'string' ? $(q) : q;
  if (el) el.addEventListener(ev, fn);
}

function setVal(q, v) {
  const el = typeof q === 'string' ? $(q) : q;
  if (!el || el === document.activeElement) return;
  if (el.type === 'checkbox') el.checked = !!v;
  else el.value = typeof v === 'number' ? +v.toFixed(2) : v;
}

const SCENE_ICON = '<svg viewBox="0 0 20 20"><path d="M3 7h14v9H3zM3 7l2-3h3l-2 3M8 7l2-3h3l-2 3M13 7l2-3h2"/></svg>';
const GROUP_ICON = '<svg viewBox="0 0 20 20"><rect x="3" y="3" width="8" height="8"/><circle cx="13" cy="13" r="4"/></svg>';

function updateReadouts() {
  // Edit path as in Flash: "Scene 1 › Group › …"
  const ns = S.symPath.length;
  const crumbs = [`<span class="crumb" data-depth="0">${SCENE_ICON} ${(S.scenes[S.scene] || 'Escena 1').replace(/</g, '&lt;')}</span>`]
    .concat(S.symPath.map((p, i) => `<span class="sep">›</span><span class="crumb sym" data-depth="${i + 1}">${KIND_ICON[p.kind]} ${p.name.replace(/</g, '&lt;')}</span>`))
    .concat(S.scopes.map((n, i) => `<span class="sep">›</span><span class="crumb" data-depth="${ns + i + 1}">${GROUP_ICON} ${n}</span>`));
  const html = crumbs.join('');
  if ($('#crumbs').innerHTML !== html) $('#crumbs').innerHTML = html;
  $('#scope-back').disabled = !S.scopes.length && !ns;
  $('#ro-frame').textContent = S.frame + 1;
  $('#ro-fps').textContent = S.fps.toFixed(1);
  $('#ro-time').textContent = (S.frame / S.fps).toFixed(1) + 's';
  $('#onionbtn').classList.toggle('on', S.onion);
  $('#onionoutl').classList.toggle('on', S.onionOutlines);
  $('#editmulti').classList.toggle('on', S.editMultiple);
  // The icon is only changed when the state changes: rebuilding it on every frame
  // during playback made clicks get lost (the button changed under the mouse).
  const pb = $('#playbtn');
  if (pb.dataset.playing !== String(playing)) {
    pb.dataset.playing = String(playing);
    pb.classList.toggle('on', playing);
    pb.innerHTML = playing
      ? '<svg viewBox="0 0 20 20"><rect x="5" y="4" width="3.5" height="12"/><rect x="11.5" y="4" width="3.5" height="12"/></svg>'
      : '<svg viewBox="0 0 20 20"><path d="M6 4l10 6-10 6z" class="f"/></svg>';
  }
  $('#st-doc').textContent = `${S.width} × ${S.height} px · ${S.total} fotogramas`;
  $('#tabname').textContent = docName + '.omf';
  $('#docname').textContent = docName + '.omf';
  document.title = `${docName} — Open McFlash`;
}

function updateZoomUI() {
  if (!S) return;
  S.zoom = JSON.parse(ed.state()).zoom;
  const sel = $('#zoomsel');
  const z = Math.round(S.zoom);
  const opt = [...sel.options].find((o) => o.value === String(z));
  if (opt) sel.value = String(z);
  else {
    const c = sel.querySelector('[value=custom]');
    c.textContent = `${z}%`;
    sel.value = 'custom';
  }
}

$('#zoomsel').addEventListener('change', (e) => {
  const v = e.target.value;
  if (v === 'fit') ed.fit_stage();
  else if (v !== 'custom') ed.set_zoom(+v);
  requestRender();
  updateZoomUI();
  e.target.blur();
});

// ───────────────────────── Panels (right side, Photopea style) ─────────────────────────

const PANEL_DEFS = [
  ['props', 'Propiedades'],
  ['library', 'Biblioteca'],
  ['filters', 'Filtros'],
  ['mixer', 'Mezclador de colores'],
  ['swatches', 'Muestras de color'],
  ['align', 'Alinear'],
  ['scene', 'Escena'],
  ['history', 'Historial'],
];
let panelState = loadPref('omf.panels', { props: true, library: true, filters: true, mixer: true, swatches: true, align: true, scene: true, history: false });
let sym = null;
let txt = null;
let io = null;
let rulers = null;
let acts = null;
let audio = null;
let hist = null;
let spell = null;

function buildPanels() {
  const host = $('#panels');
  host.innerHTML = PANEL_DEFS.map(([id, title]) => `
    <div class="panel${panelState[id] === 'collapsed' ? ' collapsed' : ''}" data-panel="${id}"${panelState[id] === false ? ' hidden' : ''}>
      <div class="panel-head">${title}</div>
      <div class="panel-body" id="pb-${id}"></div>
    </div>`).join('');
  $$('.panel-head', host).forEach((h) => h.addEventListener('click', () => {
    const p = h.parentElement;
    p.classList.toggle('collapsed');
    panelState[p.dataset.panel] = p.classList.contains('collapsed') ? 'collapsed' : true;
    savePref('omf.panels', panelState);
  }));
  sym.buildLibrary($('#pb-library'));
  sym.buildFilters($('#pb-filters'));
  buildMixer();
  buildSwatches();
  buildAlign();
  buildScenePanel();
  hist.build($('#pb-history'));
}

function buildMixer() {
  $('#pb-mixer').innerHTML = `
    <div class="mixer">
      <div class="chips">
        <div class="chip" data-mix="stroke" title="Trazo"><i></i></div>
        <div class="chip" data-mix="fill" title="Relleno"><i></i></div>
      </div>
      <div class="fields">
        <div class="prow"><span class="lab">Tipo</span><select id="mx-type"><option value="none">Ninguno</option><option value="solid">Sólido</option><option value="linear">Lineal</option><option value="radial">Radial</option><option value="bitmap">Mapa de bits</option></select></div>
        <div class="prow"><span class="lab">Color</span><input type="color" id="mx-picker" style="width:30px;height:22px;padding:0;border:1px solid #000;background:none"><input type="text" id="mx-hex" maxlength="7"></div>
        <div class="prow"><span class="lab">Alfa</span><input type="range" id="mx-alpha-r" min="0" max="100"><input type="number" id="mx-alpha" min="0" max="100" style="width:50px">%</div>
        <div class="prow" id="mx-none-row"><span class="lab"></span><label style="display:flex;gap:5px;align-items:center;color:var(--muted)"><input type="checkbox" id="mx-none"> Sin color</label></div>
      </div>
    </div>
    <div id="mx-bmp" hidden><div class="bmp-grid" id="mx-bmps"></div><button class="btn small" data-cmd="importLibrary">Importar…</button></div>
    <div id="mx-grad" hidden>
      <div id="mx-gbar"></div>
      <div class="prow"><span class="lab">Desbordar</span><select id="mx-spread"><option value="pad">Extender</option><option value="reflect">Reflejar</option><option value="repeat">Repetir</option></select></div>
      <div class="prow"><span class="lab"></span><label class="chk"><input type="checkbox" id="mx-linear"> RGB lineal</label></div>
    </div>`;
  gbar = new GradientBar($('#mx-gbar'), {
    onChange: (g) => applyGrad(g),
    onPick: () => updateMixer(),
  });
  bind('#mx-type', 'change', (e) => {
    const v = e.target.value;
    activeWell = 'fill';
    if (v === 'bitmap') {
      const first = S.library.find((x) => x.kind === 'bitmap');
      if (first) cmd(() => ed.set_fill_bitmap(first.bitmap));
      else { toast('Importa una imagen a la biblioteca para rellenar con ella'); refresh(); }
    } else if (v === 'none') cmd(() => ed.set_fill(S.fill, S.fillAlpha, false));
    else if (v === 'solid') cmd(() => ed.set_fill(S.fill, S.fillAlpha, true));
    else applyGrad({ ...(S.fillGrad || { stops: [{ t: 0, c: hexRgba('#ffffff') }, { t: 1, c: hexRgba('#000000') }] }), kind: v });
    e.target.blur();
  });
  $('#mx-bmps').addEventListener('click', (e) => {
    const t = e.target.closest('[data-asset]');
    if (t) cmd(() => ed.set_fill_bitmap(+t.dataset.asset));
  });
  bind('#mx-spread', 'change', (e) => S.fillGrad && applyGrad({ ...S.fillGrad, spread: e.target.value }));
  bind('#mx-linear', 'change', (e) => S.fillGrad && applyGrad({ ...S.fillGrad, linear_rgb: e.target.checked }));
  $$('[data-mix]').forEach((c) => c.addEventListener('click', () => { activeWell = c.dataset.mix; refresh(); }));
  const apply = (hex, alphaPct, none) => {
    const a = Math.round((alphaPct / 100) * 255);
    // With a gradient, the Color Mixer color is that of the selected pointer.
    if (activeWell === 'fill' && S.fillGrad && !none) return gbar.setColor(hexRgba(hex, a));
    if (activeWell === 'fill') ed.set_fill(hex, a, !none);
    else ed.set_stroke(hex, a, !none);
    refresh();
  };
  const cur = () => {
    if (activeWell === 'fill' && S.fillGrad && gbar.selected) return { hex: rgbaHex(gbar.selected.c), a: Math.round(gbar.selected.c.a / 2.55) };
    return { hex: S[activeWell], a: Math.round(S[activeWell + 'Alpha'] / 2.55) };
  };
  bind('#mx-picker', 'input', (e) => apply(e.target.value, cur().a, false));
  bind('#mx-hex', 'change', (e) => {
    const v = normHex(e.target.value);
    if (v) apply(v, cur().a, false);
  });
  bind('#mx-alpha', 'change', (e) => apply(cur().hex, clamp(+e.target.value, 0, 100), false));
  bind('#mx-alpha-r', 'input', (e) => apply(cur().hex, +e.target.value, false));
  bind('#mx-none', 'change', (e) => apply(cur().hex, cur().a, e.target.checked));
}

let gbar = null;
function applyGrad(g) {
  ed.set_fill_gradient(JSON.stringify({ kind: g.kind, stops: g.stops, spread: g.spread || 'pad', focal: g.focal || 0, linear_rgb: !!g.linear_rgb }));
  refresh();
}

function updateMixer() {
  $$('[data-mix]').forEach((c) => {
    const w = c.dataset.mix;
    if (w === 'fill') paintFill(c);
    else paintChip(c, S[w + 'On'] ? S[w] : null, S[w + 'Alpha']);
    c.classList.toggle('sel', w === activeWell);
  });
  const grad = activeWell === 'fill' && S.fillOn && S.fillGrad;
  const bmp = activeWell === 'fill' && S.fillOn && S.fillBmp;
  setVal('#mx-type', activeWell !== 'fill' ? (S.strokeOn ? 'solid' : 'none') : !S.fillOn ? 'none' : grad ? S.fillGrad.kind : bmp ? 'bitmap' : 'solid');
  $('#mx-type').disabled = activeWell !== 'fill';
  $('#mx-grad').hidden = !grad;
  $('#mx-bmp').hidden = !bmp;
  $('#mx-none-row').hidden = !!grad || !!bmp;
  if (bmp) {
    const items = S.library.filter((x) => x.kind === 'bitmap');
    const sig = JSON.stringify([items.map((x) => x.bitmap), S.fillBmp]);
    const grid = $('#mx-bmps');
    if (grid.dataset.sig !== sig) {
      grid.dataset.sig = sig;
      grid.innerHTML = items.map((x) => `<img data-asset="${x.bitmap}" class="${x.bitmap === S.fillBmp ? 'sel' : ''}" title="${x.name.replace(/"/g, '&quot;')}" src="${bitmapThumb(x.id)}">`).join('');
    }
  }
  if (grad) {
    if (JSON.stringify(gbar.g?.stops) !== JSON.stringify(S.fillGrad.stops) || gbar.g?.kind !== S.fillGrad.kind) gbar.set(S.fillGrad);
    setVal('#mx-spread', S.fillGrad.spread || 'pad');
    setVal('#mx-linear', !!S.fillGrad.linear_rgb);
    const c = gbar.selected?.c || S.fillGrad.stops[0].c;
    const ha = Math.round(c.a / 2.55);
    setVal('#mx-picker', rgbaHex(c));
    setVal('#mx-hex', rgbaHex(c).toUpperCase());
    setVal('#mx-alpha', ha);
    setVal('#mx-alpha-r', ha);
    return;
  }
  const hex = S[activeWell], a = Math.round(S[activeWell + 'Alpha'] / 2.55);
  setVal('#mx-picker', hex);
  setVal('#mx-hex', hex.toUpperCase());
  setVal('#mx-alpha', a);
  setVal('#mx-alpha-r', a);
  setVal('#mx-none', !S[activeWell + 'On']);
}

// Flash default palette: 216 web-safe colors laid out in 2×3 blocks of 6×6.
const WEB = ['00', '33', '66', '99', 'CC', 'FF'];
function flashSwatches() {
  const rows = [];
  const grays = ['000000', '333333', '666666', '999999', 'CCCCCC', 'FFFFFF', 'FF0000', '00FF00', '0000FF', 'FFFF00', '00FFFF', 'FF00FF'];
  for (let row = 0; row < 12; row++) {
    const cells = [];
    for (let col = 0; col < 18; col++) {
      const r = Math.floor(col / 6) + (row >= 6 ? 3 : 0);
      const g = row % 6;
      const b = col % 6;
      cells.push(WEB[r] + WEB[g] + WEB[b]);
    }
    rows.push(cells);
  }
  return { grays, rows };
}
const SW = flashSwatches();

function swatchGridHTML(withGrays = true) {
  let html = '<div class="swatches">';
  if (withGrays) {
    for (let i = 0; i < 18; i++) {
      const c = SW.grays[i] || '000000';
      html += `<div data-sw="#${c}" style="background:#${c}" title="#${c}"></div>`;
    }
  }
  for (const r of SW.rows) for (const c of r) html += `<div data-sw="#${c}" style="background:#${c}" title="#${c}"></div>`;
  return html + '</div>';
}

function buildSwatches() {
  $('#pb-swatches').innerHTML = swatchGridHTML() + `<div class="grad-swatches">${DEFAULT_GRADIENTS.map((g, i) => `<div data-gsw="${i}" title="Degradado ${g.kind === 'radial' ? 'radial' : 'lineal'}" style="background:${gradCss(g)}"></div>`).join('')}</div>`;
  $('#pb-swatches').addEventListener('click', (e) => {
    const gs = e.target.closest('[data-gsw]');
    if (gs) {
      activeWell = 'fill';
      return applyGrad(DEFAULT_GRADIENTS[+gs.dataset.gsw]);
    }
    const s = e.target.closest('[data-sw]');
    if (!s) return;
    if (activeWell === 'fill') ed.set_fill(s.dataset.sw, S.fillAlpha, true);
    else ed.set_stroke(s.dataset.sw, S.strokeAlpha, true);
    refresh();
  });
}

function buildAlign() {
  $('#pb-align').innerHTML = `
    <div class="align-row">
      <button class="ib" data-align="left" title="Alinear a la izquierda">${ICONS.alignL}</button>
      <button class="ib" data-align="hcenter" title="Centrar horizontalmente">${ICONS.alignHC}</button>
      <button class="ib" data-align="right" title="Alinear a la derecha">${ICONS.alignR}</button>
      <span style="width:10px"></span>
      <button class="ib" data-align="top" title="Alinear arriba">${ICONS.alignT}</button>
      <button class="ib" data-align="vcenter" title="Centrar verticalmente">${ICONS.alignVC}</button>
      <button class="ib" data-align="bottom" title="Alinear abajo">${ICONS.alignB}</button>
    </div>
    <label style="display:flex;gap:6px;align-items:center;color:var(--muted)"><input type="checkbox" id="al-stage"> Respecto al escenario</label>`;
  $('#pb-align').addEventListener('click', (e) => {
    const b = e.target.closest('[data-align]');
    if (b) cmd(() => ed.align(b.dataset.align, $('#al-stage').checked));
  });
}

function updatePanels() {
  acts?.sync();
  txt.setState(S);
  txt.sync();
  sym.setState(S);
  sym.update();
  updateScenePanel();
  updateMixer();
  updateProps();
}

function updateProps() {
  const host = $('#pb-props');
  const sel = S.sel;
  const fi = S.frameInfo;
  const tw = S.tween;
  const showText = S.textEdit || S.textSel || (S.tool === 'text' && !sel);
  const sounds = S.library.filter((x) => x.kind === 'sound');
  const sig = showText ? `${txt.propsSig()}:${!!sel}:${sel && sel.count}` : sel ? `sel:${sel.kind}:${sel.radius !== null}:${sel.closed}:${sym.instanceSig()}` : `doc:${!!fi}:${tw ? tw.tweenKind : ''}:${sounds.map((x) => x.id + x.name).join()}:${!!(fi && fi.sound)}`;
  if (host.dataset.sig !== sig) {
    host.dataset.sig = sig;
    if (showText) {
      host.innerHTML = txt.propsHTML() + (sel ? `<div class="grid2" style="margin-top:8px">
          <span>An:</span><input type="number" id="pp-w" step="1"><span>X:</span><input type="number" id="pp-x" step="1">
          <span>Al:</span><input type="number" id="pp-h" step="1"><span>Y:</span><input type="number" id="pp-y" step="1"></div>` : '');
      txt.bindProps(host);
      const geo = () => ed.set_sel_bounds(+$('#pp-x').value, +$('#pp-y').value, +$('#pp-w').value, +$('#pp-h').value);
      ['#pp-x', '#pp-y', '#pp-w', '#pp-h'].forEach((q) => bind(q, 'change', () => cmd(geo)));
    } else if (sel) {
      host.innerHTML = sym.instanceHTML() + `
        <div class="ptitle">${S.inst ? 'Posición y tamaño' : sel.kind} <span class="badge" id="pp-count"></span></div>
        <div class="grid2">
          <span>An:</span><input type="number" id="pp-w" step="1">
          <span>X:</span><input type="number" id="pp-x" step="1">
          <span>Al:</span><input type="number" id="pp-h" step="1">
          <span>Y:</span><input type="number" id="pp-y" step="1">
        </div>
        ${S.selInstance ? '' : '<div class="prow"><span class="lab">Trazo</span><div class="chip" id="pp-stroke"><i></i></div><input type="number" id="pp-sw" min="0.1" step="0.5" style="width:56px"></div>'}
        ${sel.closed && !S.selInstance ? '<div class="prow"><span class="lab">Relleno</span><div class="chip" id="pp-fill"><i></i></div></div>' : ''}
        ${sel.radius !== null ? '<div class="prow"><span class="lab">Radio</span><input type="number" id="pp-radius" min="0" step="1"></div>' : ''}`;
      const geo = () => ed.set_sel_bounds(+$('#pp-x').value, +$('#pp-y').value, +$('#pp-w').value, +$('#pp-h').value);
      ['#pp-x', '#pp-y', '#pp-w', '#pp-h'].forEach((q) => bind(q, 'change', () => cmd(geo)));
      bind('#pp-sw', 'change', (e) => cmd(() => ed.set_stroke_width(+e.target.value || 1)));
      bind('#pp-radius', 'change', (e) => cmd(() => ed.set_rect_radius(+e.target.value || 0)));
      bind('#pp-stroke', 'click', (e) => { activeWell = 'stroke'; openWellPicker('stroke', e.currentTarget, true); });
      bind('#pp-fill', 'click', (e) => { activeWell = 'fill'; openWellPicker('fill', e.currentTarget, true); });
      sym.bindInstance(host);
    } else {
      host.innerHTML = `
        <div class="ptitle">Documento</div>
        <div class="prow"><span class="lab">Tamaño</span><button class="btn small" data-cmd="docProps" id="pp-size"></button></div>
        <div class="prow"><span class="lab">Fondo</span><div class="chip" id="pp-bg"><i></i></div></div>
        <div class="prow"><span class="lab">Velocidad</span><input type="number" id="pp-fps" min="1" max="120" step="1"> fps</div>
        ${fi && tw ? `<div style="height:1px;background:var(--line2);margin:10px 0"></div>
        <div class="ptitle">Fotograma <span class="badge" id="pp-frame"></span></div>
        <div class="prow"><span class="lab">Etiqueta</span><input type="text" id="pp-label" placeholder="Sin etiqueta" style="width:100px"></div>
        <div class="prow"><span class="lab">Tipo</span><select id="pp-labelkind"><option value="name">Nombre</option><option value="comment">Comentario</option><option value="anchor">Ancla</option></select></div>
        <div class="prow"><span class="lab">Interpolar</span><select id="pp-tween"><option value="none">Ninguna</option><option value="motion">Movimiento</option><option value="shape">Forma</option></select></div>
        ${tw.tweenKind !== 'none' ? `<div class="prow"><span class="lab">Aceleración</span><input type="range" id="pp-ease-r" min="-100" max="100" style="width:70px"><input type="number" id="pp-ease" min="-100" max="100" style="width:46px"><button class="btn small" data-cmd="easeDialog" title="Aceleración personalizada">Editar…</button></div>` : ''}
        ${tw.tweenKind === 'motion' ? `
          <div class="prow"><span class="lab">Rotar</span><select id="pp-rot"><option value="auto">Automático</option><option value="none">Ninguno</option><option value="cw">Horario</option><option value="ccw">Antihorario</option></select><input type="number" id="pp-spins" min="0" max="99" style="width:40px"> veces</div>
          <div class="prow"><span class="lab"></span><label class="chk"><input type="checkbox" id="pp-scale"> Escala</label></div>
          <div class="prow"><span class="lab"></span><label class="chk"><input type="checkbox" id="pp-orient"> Orientar según trayectoria</label></div>
          <div class="prow"><span class="lab"></span><label class="chk"><input type="checkbox" id="pp-snap"> Ajustar</label></div>
          <div class="prow"><span class="lab"></span><label class="chk" title="Los símbolos gráficos siguen su animación a lo largo de la interpolación"><input type="checkbox" id="pp-syncg"> Sincronizar</label></div>` : ''}
        ${tw.tweenKind === 'shape' ? `<div class="prow"><span class="lab">Consejos</span><button class="btn small" data-cmd="addShapeHint" title="Ctrl+Mayús+H">Añadir</button><button class="btn small" data-cmd="removeHints">Quitar todos</button></div>` : ''}
        <div class="prow"><span class="lab">Sonido</span><select id="pp-sound" style="width:150px"><option value="">Ninguno</option>${sounds.map((x) => `<option value="${x.id}">${x.name.replace(/</g, '&lt;')}</option>`).join('')}</select></div>
        ${fi.sound ? `<div class="prow"><span class="lab">Efecto</span><select id="pp-sfx" style="width:150px">${EFFECTS.map(([v, l]) => `<option value="${v}">${l}</option>`).join('')}</select><button class="btn small" data-cmd="soundEnvDialog" title="Editar envolvente">Editar…</button></div>
        <div class="prow"><span class="lab">Sinc.</span><select id="pp-sync"><option value="event">Evento</option><option value="start">Inicio</option><option value="stop">Detener</option><option value="stream">Flujo</option></select><input type="number" id="pp-loops" min="1" max="9999" style="width:52px" title="Repetir"> veces</div>` : ''}` : ''}`;
      bind('#pp-fps', 'change', (e) => cmd(() => ed.set_document(S.width, S.height, S.bg, clamp(+e.target.value || 12, 1, 120))));
      bind('#pp-bg', 'click', (e) => openColorPopover(e.currentTarget, { hex: S.bg, alpha: 255, allowNone: false, noAlpha: true }, (hex) => cmd(() => ed.set_document(S.width, S.height, hex, S.fps))));
      bind('#pp-tween', 'change', (e) => cmd(() => ed.set_tween_kind(e.target.value)));
      bind('#pp-ease', 'change', (e) => cmd(() => ed.set_ease(+e.target.value)));
      bind('#pp-ease-r', 'input', (e) => cmd(() => ed.set_ease(+e.target.value)));
      const opts = () => cmd(() => ed.set_tween_options($('#pp-rot').value, clamp(+$('#pp-spins').value || 0, 0, 99), $('#pp-scale').checked, $('#pp-orient').checked, $('#pp-snap').checked));
      ['#pp-rot', '#pp-spins', '#pp-scale', '#pp-orient', '#pp-snap'].forEach((q) => bind(q, 'change', opts));
      bind('#pp-syncg', 'change', (e) => cmd(() => ed.set_tween_sync(e.target.checked)));
      const label = () => cmd(() => ed.set_label($('#pp-label').value, $('#pp-labelkind').value));
      bind('#pp-label', 'change', label);
      bind('#pp-labelkind', 'change', label);
      const sound = () => {
        const id = $('#pp-sound').value;
        const cur = S.frameInfo && S.frameInfo.sound;
        const same = cur && +id === cur.id;
        const effect = same ? ($('#pp-sfx')?.value || cur.effect || 'none') : 'none';
        cmd(() => ed.set_frame_sound(id ? JSON.stringify({
          id: +id, sync: $('#pp-sync')?.value || cur?.sync || 'event', loops: clamp(+($('#pp-loops')?.value || 1), 1, 9999),
          effect, env: same && effect === 'custom' ? cur.env || [] : [], in: same ? cur.in || 0 : 0, out: same ? cur.out || 0 : 0,
        }) : 'null'));
      };
      ['#pp-sound', '#pp-sync', '#pp-loops'].forEach((q) => bind(q, 'change', sound));
      bind('#pp-sfx', 'change', (e) => (e.target.value === 'custom' ? soundEnvDialog() : sound()));
    }
  }
  if (showText) {
    txt.updateProps(host);
    if (sel) { setVal('#pp-x', sel.x); setVal('#pp-y', sel.y); setVal('#pp-w', sel.w); setVal('#pp-h', sel.h); }
  } else if (sel) {
    $('#pp-count').textContent = sel.count > 1 ? `(${sel.count} objetos)` : '';
    setVal('#pp-x', sel.x); setVal('#pp-y', sel.y); setVal('#pp-w', sel.w); setVal('#pp-h', sel.h);
    setVal('#pp-sw', sel.strokeWidth);
    setVal('#pp-radius', sel.radius ?? 0);
    paintChip($('#pp-stroke'), sel.stroke, sel.strokeAlpha);
    if (sel.grad && $('#pp-fill')) $('#pp-fill i').style.background = gradCss(sel.grad);
    else paintChip($('#pp-fill'), sel.fill, sel.fillAlpha);
    sym.updateInstance(host);
  } else {
    $('#pp-size').textContent = `${S.width} × ${S.height} píxeles`;
    paintChip($('#pp-bg'), S.bg);
    setVal('#pp-fps', S.fps);
    if (fi && tw) {
      $('#pp-frame').textContent = fi.isKey ? `clave ${fi.key + 1}` : `${S.frame + 1} (clave en ${fi.key + 1})`;
      setVal('#pp-tween', tw.tweenKind);
      setVal('#pp-ease', fi.ease);
      setVal('#pp-ease-r', fi.ease);
      setVal('#pp-label', tw.label);
      setVal('#pp-labelkind', tw.labelKind);
      setVal('#pp-rot', tw.rotate);
      setVal('#pp-spins', tw.spins);
      setVal('#pp-scale', tw.scale);
      setVal('#pp-orient', tw.orient);
      setVal('#pp-snap', tw.snap);
      setVal('#pp-syncg', !!tw.sync);
      setVal('#pp-sound', fi.sound ? String(fi.sound.id) : '');
      if (fi.sound) {
        setVal('#pp-sync', fi.sound.sync);
        setVal('#pp-loops', fi.sound.loops);
        setVal('#pp-sfx', fi.sound.effect || 'none');
      }
    }
  }
}

// ───────────────────────── Flash-style color picker ─────────────────────────

const pop = $('#popover');
let popClose = null;

function openWellPicker(which, anchor, applyOnly = false) {
  openColorPopover(anchor, {
    hex: S[which], alpha: S[which + 'Alpha'], none: !S[which + 'On'], allowNone: true,
  }, (hex, alpha, none) => cmd(() => which === 'fill' ? ed.set_fill(hex, alpha, !none) : ed.set_stroke(hex, alpha, !none)));
}

function openColorPopover(anchor, { hex, alpha = 255, none = false, allowNone = true, noAlpha = false }, onPick) {
  closeMenus();
  pop.innerHTML = `
    <div class="pop-top">
      <div class="preview checker"><i></i></div>
      <input type="text" id="pc-hex" maxlength="7" value="${hex.toUpperCase()}">
      ${noAlpha ? '' : `<label class="muted" style="display:flex;align-items:center;gap:3px">Alfa <input type="number" id="pc-alpha" min="0" max="100" value="${Math.round(alpha / 2.55)}">%</label>`}
      <div class="grow"></div>
      ${allowNone ? `<button class="ib" id="pc-none" title="Sin color">${ICONS.noColor}</button>` : ''}
      <label class="ib" title="Más colores…" style="position:relative;overflow:hidden"><svg viewBox="0 0 20 20"><circle cx="10" cy="10" r="7"/><circle cx="7" cy="8" r="1.3" class="f"/><circle cx="11" cy="6" r="1.3" class="f"/><circle cx="14" cy="10" r="1.3" class="f"/></svg><input type="color" id="pc-native" value="${hex}" style="position:absolute;inset:0;opacity:0;cursor:pointer"></label>
    </div>
    ${swatchGridHTML()}`;
  const prev = pop.querySelector('.preview i');
  const show = (h) => (prev.style.background = h ? hexA(h, alphaVal()) : 'transparent');
  const alphaVal = () => (noAlpha ? 255 : Math.round(clamp(+$('#pc-alpha', pop).value, 0, 100) * 2.55));
  show(none ? null : hex);
  const pick = (h, isNone = false) => {
    onPick(h, alphaVal(), isNone);
    hidePopover();
  };
  pop.addEventListener('mouseover', (e) => {
    const s = e.target.closest('[data-sw]');
    if (s) { show(s.dataset.sw); $('#pc-hex', pop).value = s.dataset.sw; }
  });
  pop.onclick = (e) => {
    const s = e.target.closest('[data-sw]');
    if (s) pick(s.dataset.sw);
    if (e.target.closest('#pc-none')) pick(hex, true);
  };
  $('#pc-hex', pop).addEventListener('keydown', (e) => {
    e.stopPropagation();
    if (e.key === 'Enter') { const v = normHex(e.target.value); if (v) pick(v); }
    if (e.key === 'Escape') hidePopover();
  });
  $('#pc-alpha', pop)?.addEventListener('keydown', (e) => {
    e.stopPropagation();
    if (e.key === 'Enter') pick(normHex($('#pc-hex', pop).value) || hex);
  });
  $('#pc-native', pop).addEventListener('change', (e) => pick(e.target.value));
  pop.hidden = false;
  const r = anchor.getBoundingClientRect();
  const pw = pop.offsetWidth, ph = pop.offsetHeight;
  let x = r.left, y = r.bottom + 4;
  if (x + pw > innerWidth - 8) x = innerWidth - pw - 8;
  if (y + ph > innerHeight - 8) y = r.top - ph - 4;
  pop.style.left = Math.max(8, x) + 'px';
  pop.style.top = Math.max(8, y) + 'px';
  setTimeout(() => {
    popClose = (e) => { if (!pop.contains(e.target)) hidePopover(); };
    document.addEventListener('pointerdown', popClose, true);
  });
}

function hidePopover() {
  pop.hidden = true;
  if (popClose) document.removeEventListener('pointerdown', popClose, true);
  popClose = null;
}

// ───────────────────────── Commands ─────────────────────────

let playing = false;
let playRAF = 0;

const CMDS = {
  newDoc: () => newDocDialog(),
  open: () => $('#fileopen').click(),
  save: () => download(`${docName}.omf`, new Blob([ed.to_json()], { type: 'application/json' })),
  exportSWF: () => {
    download(`${docName}.swf`, new Blob([ed.export_swf()], { type: 'application/x-shockwave-flash' }));
    toast('Película exportada en formato SWF (versión 8)');
  },
  exportPNG: () => download(`${docName}-fotograma${S.frame + 1}.png`, new Blob([ed.export_png(1)], { type: 'image/png' })),
  undo: () => ed.undo(), redo: () => ed.redo(),
  cut: () => ed.cut(), copy: () => ed.copy(), paste: () => ed.paste(false), pasteInPlace: () => ed.paste(true),
  duplicate: () => ed.duplicate(), clear: () => ed.delete_selection(),
  selectAll: () => { if (S.tool !== 'select' && S.tool !== 'transform') ed.set_tool('select'); ed.select_all(); },
  deselect: () => ed.deselect(),
  // F5/F6/F7 act on the range selected in the Timeline, as in Flash.
  insertFrame: () => ed.insert_frames(), removeFrame: () => ed.remove_frames(),
  insertKeyframe: () => (singleCell() ? ed.insert_keyframe(false) : ed.convert_keyframes(false)),
  insertBlankKeyframe: () => (singleCell() ? ed.insert_keyframe(true) : ed.convert_keyframes(true)),
  convertKeyframes: () => ed.convert_keyframes(false), convertBlankKeyframes: () => ed.convert_keyframes(true),
  clearKeyframe: () => ed.clear_keyframes(),
  createTween: () => ed.set_tween_kind('motion'), createShapeTween: () => ed.set_tween_kind('shape'),
  removeTween: () => ed.set_tween_kind('none'),
  copyFrames: () => ed.copy_frames(), cutFrames: () => ed.cut_frames(), pasteFrames: () => ed.paste_frames(),
  clearFrames: () => ed.clear_frames(), selectAllFrames: () => ed.select_all_frames(), reverseFrames: () => ed.reverse_frames(),
  addShapeHint: () => ed.add_shape_hint(), removeHints: () => ed.remove_all_hints(),
  toggleHints: () => ed.set_show_hints(!S.showHints),
  easeDialog: () => easeDialog(),
  soundEnvDialog: () => soundEnvDialog(),
  addLayer: () => ed.add_layer(), deleteLayer: () => ed.delete_layer(),
  addMotionGuide: () => ed.add_motion_guide(), addFolder: () => ed.add_folder(),
  layerPropsCmd: () => layerProps(S.layer),
  onionOutlines: () => ed.set_onion_outlines(!S.onionOutlines),
  editMultiple: () => ed.set_edit_multiple(!S.editMultiple),
  onionMarkers: () => onionMarkersMenu(),
  sceneAdd: () => ed.scene_add(), sceneDuplicate: () => ed.scene_duplicate(), sceneDelete: () => ed.scene_delete(S.scene),
  scenePanel: () => run('togglePanel', 'scene'),
  sceneMenu: () => sceneMenu(),
  sceneUp: () => ed.scene_move(S.scene, Math.max(0, S.scene - 1)),
  sceneDown: () => ed.scene_move(S.scene, Math.min(S.scenes.length - 1, S.scene + 1)),
  layerUp: () => ed.move_layer(S.layer, Math.max(0, S.layer - 1)),
  layerDown: () => ed.move_layer(S.layer, Math.min(S.layers.length - 1, S.layer + 1)),
  togglePlay: () => (playing ? stop() : play()),
  firstFrame: () => { stop(); ed.set_frame(0); timeline.ensureVisible(0); },
  lastFrame: () => { stop(); ed.set_frame(S.total - 1); timeline.ensureVisible(S.total - 1); },
  prevFrame: () => { stop(); ed.step_frame(-1, false); },
  nextFrame: () => { stop(); ed.step_frame(1, false); },
  centerFrame: () => { timeline.scrollX = Math.max(0, S.frame * 9 - timeline.w / 2); timeline.draw(); },
  toggleOnion: () => ed.toggle_onion(),
  testMovie: () => testMovie(),
  zoomIn: () => ed.zoom_at(canvas.width / 2, canvas.height / 2, 2),
  zoomOut: () => ed.zoom_at(canvas.width / 2, canvas.height / 2, 0.5),
  zoom100: () => ed.set_zoom(100), zoom200: () => ed.set_zoom(200), zoom400: () => ed.set_zoom(400),
  zoomFit: () => ed.fit_stage(),
  docProps: () => docDialog(),
  flipH: () => ed.flip(true), flipV: () => ed.flip(false),
  rot90cw: () => ed.rotate90(true), rot90ccw: () => ed.rotate90(false),
  group: () => ed.group(), ungroup: () => ed.ungroup(), breakApart: () => ed.break_apart(),
  editSelected: () => ed.edit_selected(),
  editAll: () => ed.leave_all_groups(),
  leaveGroup: () => (S.scopes.length ? ed.leave_group() : ed.leave_symbol()),
  convertSymbol: () => sym.convertDialog(),
  newSymbol: () => sym.newSymbolDialog(),
  swapSymbol: () => sym.swapDialog(),
  // Ctrl+E: inside a symbol returns to the document; with an instance selected, edits its symbol.
  editSymbols: () => {
    if (S.symPath.length && !S.inst) return ed.edit_document();
    const id = S.inst ? S.inst.symbol : sym.selected;
    if (id) ed.edit_symbol(id);
    else symbolsMenu();
  },
  editDocument: () => ed.edit_document(),
  symbolsMenu: () => symbolsMenu(),
  simpleButtons: () => ed.set_simple_buttons(!S.simpleButtons),
  libraryPanel: () => run('togglePanel', 'library'),
  historyPanel: () => run('togglePanel', 'history'),
  actionsPanel: () => acts.toggle('actions'),
  outputPanel: () => acts.toggle('output'),
  simpleActions: () => {
    prefs.simpleActions = !prefs.simpleActions;
    savePref('omf.prefs', prefs);
    toast(prefs.simpleActions ? 'Acciones de fotograma simples: activadas' : 'Acciones de fotograma simples: desactivadas');
  },
  importStage: () => io.importDialog(true),
  importLibrary: () => io.importDialog(false),
  publishSettings: () => io.settingsDialog(),
  publish: () => io.publish(),
  previewHTML: () => io.previewHTML(),
  lockFill: () => ed.set_lock_fill(!S.lockFill),
  lassoPoly: () => ed.set_lasso_polygon(!S.lassoPoly),
  polystarDialog: () => polystarDialog(),
  xformDistort: () => { setTool('transform'); ed.set_xform_mode('distort'); },
  xformEnvelope: () => { setTool('transform'); ed.set_xform_mode('envelope'); },
  xformScale: () => { setTool('transform'); ed.set_xform_mode('scale'); },
  xformRotate: () => { setTool('transform'); ed.set_xform_mode('rotate'); },
  xformFree: () => { setTool('transform'); ed.set_xform_mode('free'); },
  filtersPanel: () => run('togglePanel', 'filters'),
  smooth: () => ed.smooth_selection(), straighten: () => ed.straighten_selection(),
  linesToFills: () => ed.lines_to_fills(),
  optimizeDialog: () => optimizeDialog(), expandDialog: () => expandDialog(), softenDialog: () => softenDialog(),
  toggleObjectMode: () => ed.set_object_mode(!S.objectMode),
  snapObjects: () => ed.set_snap('objects', !S.snapObjects),
  snapGrid: () => ed.set_snap('grid', !S.snapGrid),
  snapPixels: () => ed.set_snap('pixels', !S.snapPixels),
  toggleGrid: () => ed.set_grid(!S.showGrid, S.gridSize, S.gridColor),
  gridDialog: () => gridDialog(),
  toggleRulers: () => {
    rulers.set(!rulers.on);
    prefs.rulers = rulers.on;
    savePref('omf.prefs', prefs);
  },
  toggleGuides: () => setGuides({ show: !S.guides.show }),
  lockGuides: () => setGuides({ lock: !S.guides.lock }),
  snapGuides: () => setGuides({ snap: !S.guides.snap }),
  guidesDialog: () => guidesDialog(),
  clearGuides: () => ed.clear_guides(),
  front: () => ed.arrange('front'), back: () => ed.arrange('back'),
  forward: () => ed.arrange('forward'), backward: () => ed.arrange('backward'),
  alignL: () => ed.align('left', false), alignHC: () => ed.align('hcenter', false), alignR: () => ed.align('right', false),
  alignT: () => ed.align('top', false), alignVC: () => ed.align('vcenter', false), alignB: () => ed.align('bottom', false),
  defaultColors: () => ed.default_colors(),
  swapColors: () => ed.swap_colors(),
  toggleFaucet: () => ed.set_faucet(!S.faucet),
  fullscreen: () => toggleFullscreen(),
  noColor: () => (activeWell === 'fill' ? ed.set_fill(S.fill, S.fillAlpha, false) : ed.set_stroke(S.stroke, S.strokeAlpha, false)),
  rectRadius: () => promptDialog('Configuración de rectángulo', 'Radio de esquina (px)', S.rectRadius, (v) => cmd(() => ed.set_rect_radius(+v || 0))),
  toggleTimeline: () => {
    const tl = $('#timeline');
    tl.hidden = !tl.hidden;
    $('#tl-splitter').hidden = tl.hidden;
    $('#tl-toggle').classList.toggle('on', !tl.hidden);
  },
  classicTimeline: () => {
    prefs.classic = !prefs.classic;
    savePref('omf.prefs', prefs);
    timeline.setClassic(prefs.classic);
  },
  togglePanel: (id) => {
    panelState[id] = panelState[id] === false ? true : false;
    savePref('omf.panels', panelState);
    $(`[data-panel="${id}"]`).hidden = panelState[id] === false;
  },
  shortcuts: () => shortcutsDialog(),
  about: () => aboutDialog(),
};

function run(name, arg) {
  const fn = CMDS[name];
  if (!fn) return;
  fn(arg);
  refresh();
}

document.addEventListener('click', (e) => {
  const b = e.target.closest('[data-cmd]');
  if (b && !b.disabled && !b.closest('#menus') && !b.closest('.menu-dropdown')) run(b.dataset.cmd);
});

const singleCell = () => {
  const s = S.frameSel;
  return !s || (s.l0 === s.l1 && s.f0 === s.f1);
};

// "Modify Onion Markers" button (as in Flash).
function onionMarkersMenu() {
  const b = $('#onionmarks').getBoundingClientRect();
  contextMenu(b.left, b.top - 120, [
    { label: 'Papel cebolla 2', check: S.onionBefore === 2 && S.onionAfter === 2, action: () => cmd(() => ed.set_onion(2, 2)) },
    { label: 'Papel cebolla 5', check: S.onionBefore === 5 && S.onionAfter === 5, action: () => cmd(() => ed.set_onion(5, 5)) },
    { label: 'Todo el papel cebolla', action: () => cmd(() => ed.set_onion(S.total, S.total)) },
  ]);
}

// "Edit Symbols" button on the edit bar: Library list.
function symbolsMenu() {
  const b = $('#sym-btn').getBoundingClientRect();
  if (!S.library.length) return toast('La biblioteca no tiene símbolos');
  contextMenu(b.left, b.bottom + 2, [...S.library].sort((a, c) => a.name.localeCompare(c.name, 'es', { numeric: true }))
    .map((s) => ({ label: s.name.replace(/</g, '&lt;'), action: () => cmd(() => ed.edit_symbol(s.id)) })));
}

// "Edit Scene" button on the edit bar.
function sceneMenu() {
  const b = $('#scene-btn').getBoundingClientRect();
  contextMenu(b.left, b.bottom + 2, S.scenes.map((n, i) => ({ label: n, check: i === S.scene, action: () => cmd(() => ed.scene_go(i)) })));
}

// Layer Properties (double-click on the layer icon).
function layerProps(i) {
  const l = S.layers[i];
  if (!l) return;
  let color = l.color;
  const kind = l.kind === 'normal' ? (l.masked ? 'masked' : l.guided ? 'guided' : 'normal') : l.kind;
  const radio = (v, label) => `<label class="chk"><input type="radio" name="lp-kind" value="${v}" ${kind === v ? 'checked' : ''}> ${label}</label>`;
  modal('Propiedades de capa', `
    <div class="prow"><span class="lab" style="width:90px">Nombre</span><input type="text" id="lp-name" value="${l.name.replace(/"/g, '&quot;')}" style="width:200px"></div>
    <div class="prow"><span class="lab" style="width:90px"></span><label class="chk"><input type="checkbox" id="lp-vis" ${l.visible ? 'checked' : ''}> Mostrar</label><label class="chk" style="margin-left:12px"><input type="checkbox" id="lp-lock" ${l.locked ? 'checked' : ''}> Bloquear</label></div>
    <div class="prow" style="align-items:flex-start"><span class="lab" style="width:90px">Tipo</span><div style="display:grid;gap:4px">
      ${radio('normal', 'Normal')}${radio('guide', 'Guía')}${radio('guided', 'Guiada')}${radio('motionguide', 'Guía de movimiento')}${radio('mask', 'Máscara')}${radio('masked', 'Enmascarada')}${radio('folder', 'Carpeta')}</div></div>
    <div class="prow"><span class="lab" style="width:90px">Contorno</span><div class="chip" id="lp-color"><i></i></div><label class="chk" style="margin-left:10px"><input type="checkbox" id="lp-outline" ${l.outline ? 'checked' : ''}> Ver capa como contornos</label></div>
    <div class="prow"><span class="lab" style="width:90px">Altura</span><select id="lp-height"><option value="1">100%</option><option value="2">200%</option><option value="3">300%</option></select></div>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="lp-ok">Aceptar</button></div>`, (b) => {
    paintChip($('#lp-color', b), color);
    $('#lp-height', b).value = String(l.height || 1);
    $('#lp-color', b).addEventListener('click', (e) => openColorPopover(e.currentTarget, { hex: color, allowNone: false, noAlpha: true }, (h) => { color = h; paintChip($('#lp-color'), color); }));
    $('#lp-ok', b).addEventListener('click', () => {
      ed.set_layer_props(i, $('#lp-name', b).value, $('#lp-vis', b).checked, $('#lp-lock', b).checked,
        $('input[name=lp-kind]:checked', b).value, color, $('#lp-outline', b).checked, +$('#lp-height', b).value);
      closeModal();
      refresh();
    });
  });
}

// Custom Ease: Bézier curve with two draggable control points.
function easeDialog() {
  const tw = S.tween;
  let c = (tw && tw.easeCurve) ? [...tw.easeCurve] : [0.33, 0.33, 0.67, 0.67];
  const N = 220, PAD = 16;
  modal('Personalizar aceleración', `
    <canvas id="ez-c" width="${N + PAD * 2}" height="${N + PAD * 2}" style="background:#1d1d1d;border:1px solid #000;cursor:crosshair;touch-action:none"></canvas>
    <div class="prow" style="margin-top:8px"><span class="muted">Presets</span>
      <button class="btn small" data-ez="0.33,0.33,0.67,0.67">Lineal</button>
      <button class="btn small" data-ez="0.42,0,1,1">Entrada</button>
      <button class="btn small" data-ez="0,0,0.58,1">Salida</button>
      <button class="btn small" data-ez="0.42,0,0.58,1">Entrada y salida</button></div>
    <p class="muted" style="max-width:260px">Eje horizontal: tiempo · eje vertical: avance de la interpolación.</p>
    <div class="modal-actions"><button class="btn" id="ez-reset">Restablecer</button><button class="btn" data-close>Cancelar</button><button class="btn primary" id="ez-ok">Aceptar</button></div>`, (b) => {
    const cv = $('#ez-c', b), g = cv.getContext('2d');
    const P = (x, y) => [PAD + x * N, PAD + (1 - y) * N];
    const draw = () => {
      g.clearRect(0, 0, cv.width, cv.height);
      g.strokeStyle = '#333';
      for (let i = 0; i <= 4; i++) {
        g.beginPath(); g.moveTo(...P(i / 4, 0)); g.lineTo(...P(i / 4, 1)); g.stroke();
        g.beginPath(); g.moveTo(...P(0, i / 4)); g.lineTo(...P(1, i / 4)); g.stroke();
      }
      g.strokeStyle = '#2f8cff';
      g.beginPath(); g.moveTo(...P(0, 0)); g.lineTo(...P(c[0], c[1])); g.moveTo(...P(1, 1)); g.lineTo(...P(c[2], c[3])); g.stroke();
      g.strokeStyle = '#fff';
      g.lineWidth = 2;
      g.beginPath(); g.moveTo(...P(0, 0)); g.bezierCurveTo(...P(c[0], c[1]), ...P(c[2], c[3]), ...P(1, 1)); g.stroke();
      g.lineWidth = 1;
      g.fillStyle = '#2f8cff';
      for (const [x, y] of [[c[0], c[1]], [c[2], c[3]]]) { g.beginPath(); g.arc(...P(x, y), 5, 0, Math.PI * 2); g.fill(); }
    };
    let dragging = -1;
    const at = (e) => {
      const r = cv.getBoundingClientRect();
      return [clamp((e.clientX - r.left - PAD) / N, 0, 1), clamp(1 - (e.clientY - r.top - PAD) / N, -0.5, 1.5)];
    };
    cv.addEventListener('pointerdown', (e) => {
      const [x, y] = at(e);
      dragging = Math.hypot(x - c[0], y - c[1]) < Math.hypot(x - c[2], y - c[3]) ? 0 : 2;
      cv.setPointerCapture(e.pointerId);
    });
    cv.addEventListener('pointermove', (e) => {
      if (dragging < 0) return;
      const [x, y] = at(e);
      c[dragging] = x;
      c[dragging + 1] = y;
      draw();
    });
    cv.addEventListener('pointerup', () => (dragging = -1));
    $$('[data-ez]', b).forEach((x) => x.addEventListener('click', () => { c = x.dataset.ez.split(',').map(Number); draw(); }));
    $('#ez-ok', b).addEventListener('click', () => { ed.set_ease_curve(...c); closeModal(); refresh(); });
    $('#ez-reset', b).addEventListener('click', () => { ed.set_ease_curve(-1, 0, 0, 0); closeModal(); refresh(); });
    draw();
  });
}

// Edit Envelope (custom sound effect), as in Flash 8:
// left channel on top and right below, with their handles (up to 8) and the
// in and out points on the ruler.
function soundEnvDialog() {
  const fs = S.frameInfo && S.frameInfo.sound;
  if (!fs) return toast('Pon un sonido en el fotograma clave');
  const info = JSON.parse(ed.sound_info(fs.id));
  if (!info) return;
  const len = Math.max(1, Math.round(info.seconds * 44100));
  let effect = fs.effect || 'none';
  let env = fs.env && fs.env.length ? fs.env.map((p) => ({ ...p })) : envPreset(effect, len);
  if (!env.length) env = [{ pos: 0, l: 32768, r: 32768 }];
  let inp = fs.in || 0, out = fs.out || len;
  const W = 520, LANE = 100, RULER = 18, H = RULER + LANE * 2 + 6;
  modal('Editar envolvente', `
    <div class="prow"><span class="lab">Efecto</span><select id="ev-fx" style="width:260px">${EFFECTS.map(([v, l]) => `<option value="${v}">${l}</option>`).join('')}</select></div>
    <canvas id="ev-c" width="${W}" height="${H}" style="background:#fff;border:1px solid #000;cursor:crosshair;touch-action:none;display:block"></canvas>
    <p class="muted" style="max-width:${W}px">Clic en una línea para añadir un tirador (máx. 8); arrástralo fuera para quitarlo. Las marcas grises de la regla son los puntos de entrada y salida.</p>
    <div class="modal-actions"><button class="btn" id="ev-play">Probar</button><button class="btn" id="ev-stop">Detener</button><div class="grow"></div><button class="btn" data-close>Cancelar</button><button class="btn primary" id="ev-ok">Aceptar</button></div>`, (b) => {
    const cv = $('#ev-c', b), g = cv.getContext('2d');
    const fx = $('#ev-fx', b);
    fx.value = effect;
    const peaks = ed.sound_peaks(fs.id, W);
    const X = (pos) => (pos / len) * W;
    const P = (x) => clamp(Math.round((x / W) * len), 0, len);
    const laneY = (k) => RULER + 3 + k * LANE;
    const Y = (k, v) => laneY(k) + (1 - v / 32768) * (LANE - 6) + 3;
    const draw = () => {
      g.clearRect(0, 0, W, H);
      g.fillStyle = '#eee';
      g.fillRect(0, 0, W, RULER);
      g.fillStyle = '#555';
      g.font = '10px sans-serif';
      for (let s = 0; s <= info.seconds; s += info.seconds > 8 ? 2 : info.seconds > 3 ? 1 : 0.5) {
        const x = X(s * 44100);
        g.fillRect(x, RULER - 5, 1, 5);
        g.fillText(s.toFixed(s % 1 ? 1 : 0), x + 2, 10);
      }
      for (let k = 0; k < 2; k++) {
        const y0 = laneY(k);
        g.fillStyle = '#f7f7f7';
        g.fillRect(0, y0, W, LANE - 2);
        g.fillStyle = '#7da7d9';
        for (let x = 0; x < peaks.length; x++) {
          const h = Math.max(1, peaks[x] * (LANE - 10));
          g.fillRect(x, y0 + (LANE - 2 - h) / 2, 1, h);
        }
        // Outside the in and out points: shaded.
        g.fillStyle = 'rgba(0,0,0,.25)';
        g.fillRect(0, y0, X(inp), LANE - 2);
        g.fillRect(X(out), y0, W - X(out), LANE - 2);
        const key = k ? 'r' : 'l';
        g.strokeStyle = '#1b5fd1';
        g.lineWidth = 1.5;
        g.beginPath();
        g.moveTo(0, Y(k, env[0][key]));
        for (const p of env) g.lineTo(X(p.pos), Y(k, p[key]));
        g.lineTo(W, Y(k, env[env.length - 1][key]));
        g.stroke();
        g.fillStyle = '#fff';
        for (const p of env) {
          g.fillRect(X(p.pos) - 3.5, Y(k, p[key]) - 3.5, 7, 7);
          g.strokeRect(X(p.pos) - 3.5, Y(k, p[key]) - 3.5, 7, 7);
        }
      }
      g.fillStyle = '#888';
      for (const p of [inp, out]) {
        const x = X(p);
        g.beginPath(); g.moveTo(x - 5, 0); g.lineTo(x + 5, 0); g.lineTo(x, RULER - 2); g.fill();
      }
    };
    let drag = null;
    const at = (e) => {
      const r = cv.getBoundingClientRect();
      return [e.clientX - r.left, e.clientY - r.top];
    };
    const custom = () => { effect = 'custom'; fx.value = 'custom'; };
    cv.addEventListener('pointerdown', (e) => {
      const [x, y] = at(e);
      cv.setPointerCapture(e.pointerId);
      if (y < RULER) {
        drag = { kind: Math.abs(x - X(inp)) <= Math.abs(x - X(out)) ? 'in' : 'out' };
        return;
      }
      const k = y >= laneY(1) ? 1 : 0, key = k ? 'r' : 'l';
      let i = env.findIndex((p) => Math.abs(X(p.pos) - x) < 6 && Math.abs(Y(k, p[key]) - y) < 8);
      if (i < 0) {
        if (env.length >= 8) return toast('Una envolvente tiene como mucho 8 tiradores');
        const pos = P(x);
        const [gl, gr] = envGain(env, pos);
        env.push({ pos, l: Math.round(gl * 32768), r: Math.round(gr * 32768) });
        env.sort((a, b2) => a.pos - b2.pos);
        i = env.findIndex((p) => p.pos === pos);
      }
      drag = { kind: 'env', i, k, key };
      custom();
      move(e);
    });
    const move = (e) => {
      if (!drag) return;
      const [x, y] = at(e);
      if (drag.kind === 'in') inp = Math.min(P(x), out - 1);
      else if (drag.kind === 'out') out = Math.max(P(x), inp + 1);
      else {
        const p = env[drag.i];
        const lo = drag.i > 0 ? env[drag.i - 1].pos : 0, hi = drag.i < env.length - 1 ? env[drag.i + 1].pos : len;
        p.pos = clamp(P(x), lo, hi);
        const v = 1 - (y - laneY(drag.k) - 3) / (LANE - 6);
        p[drag.key] = Math.round(clamp(v, 0, 1) * 32768);
        drag.gone = env.length > 1 && (y < RULER - 20 || y > H + 20 || x < -20 || x > W + 20);
      }
      draw();
    };
    cv.addEventListener('pointermove', move);
    cv.addEventListener('pointerup', () => {
      if (drag && drag.gone) env.splice(drag.i, 1);
      drag = null;
      draw();
    });
    fx.addEventListener('change', () => {
      effect = fx.value;
      if (effect !== 'custom') env = envPreset(effect, len);
      if (!env.length) env = [{ pos: 0, l: 32768, r: 32768 }];
      draw();
    });
    const shown = () => (effect === 'none' ? [] : env);
    $('#ev-play', b).addEventListener('click', () => { audio.stop(fs.id); audio.play(fs.id, { env: shown(), inp, out: out < len ? out : 0 }); });
    $('#ev-stop', b).addEventListener('click', () => audio.stop(fs.id));
    $('#ev-ok', b).addEventListener('click', () => {
      audio.stop(fs.id);
      closeModal();
      cmd(() => ed.set_frame_sound(JSON.stringify({ id: fs.id, sync: fs.sync, loops: fs.loops, effect, env: effect === 'custom' ? env : [], in: inp, out: out < len ? out : 0 })));
    });
    draw();
  });
}

// Scene panel (Shift+F2).
function buildScenePanel() {
  const host = $('#pb-scene');
  if (!host) return;
  host.addEventListener('click', (e) => {
    const row = e.target.closest('[data-scene]');
    if (row && !e.target.closest('input')) cmd(() => ed.scene_go(+row.dataset.scene));
  });
  host.addEventListener('dblclick', (e) => {
    const row = e.target.closest('[data-scene]');
    if (!row) return;
    const i = +row.dataset.scene;
    row.innerHTML = `<input type="text" value="${S.scenes[i].replace(/"/g, '&quot;')}" style="width:100%">`;
    const inp = row.querySelector('input');
    inp.focus();
    inp.select();
    const done = (ok) => { if (ok) ed.scene_rename(i, inp.value); host.dataset.sig = ''; refresh(); };
    inp.addEventListener('keydown', (ev) => { ev.stopPropagation(); if (ev.key === 'Enter') done(true); if (ev.key === 'Escape') done(false); });
    inp.addEventListener('blur', () => done(true));
  });
}

function updateScenePanel() {
  const host = $('#pb-scene');
  if (!host || host.querySelector('input')) return;
  const sig = JSON.stringify([S.scenes, S.scene]);
  if (host.dataset.sig === sig) return;
  host.dataset.sig = sig;
  host.innerHTML = `<div class="scene-list">${S.scenes.map((n, i) => `<div class="scene-row${i === S.scene ? ' active' : ''}" data-scene="${i}">${SCENE_ICON}<span>${n.replace(/</g, '&lt;')}</span></div>`).join('')}</div>
    <div class="scene-actions">
      <button class="ib" data-cmd="sceneDuplicate" title="Duplicar escena"><svg viewBox="0 0 20 20"><rect x="3" y="6" width="10" height="10"/><path d="M7 3h10v10"/></svg></button>
      <button class="ib" data-cmd="sceneAdd" title="Añadir escena"><svg viewBox="0 0 20 20"><path d="M10 4v12M4 10h12"/></svg></button>
      <button class="ib" data-cmd="sceneUp" title="Subir escena"><svg viewBox="0 0 20 20"><path d="M10 15V5M6 9l4-4 4 4"/></svg></button>
      <button class="ib" data-cmd="sceneDown" title="Bajar escena"><svg viewBox="0 0 20 20"><path d="M10 5v10M6 11l4 4 4-4"/></svg></button>
      <div class="grow"></div>
      <button class="ib" data-cmd="sceneDelete" title="Eliminar escena"><svg viewBox="0 0 20 20"><path d="M5 6h10l-1 11H6zM3.5 6h13M8 6V4h4v2"/></svg></button>
    </div>`;
}

function play() {
  if (playing) return;
  if (S.total <= 1) return toast('Solo hay un fotograma: añade fotogramas con F5 o F6');
  playing = true;
  let last = performance.now();
  let acc = 0;
  const tick = (now) => {
    if (!playing) return;
    acc += now - last;
    last = now;
    const step = 1000 / ed.fps();
    if (acc >= step) {
      acc %= step;
      ed.step_frame(1, true);
      S = JSON.parse(ed.state());
      audio.onFrame(S.frame, S.fps);
      if (prefs.simpleActions) simpleActions(S.frame);
      timeline.ensureVisible(S.frame);
      timeline.update(S);
      updateReadouts();
      if (S.frameInfo !== undefined) updateProps();
      requestRender();
    }
    playRAF = requestAnimationFrame(tick);
  };
  audio.onFrame(S.frame, S.fps);
  playRAF = requestAnimationFrame(tick);
  updateReadouts();
}

/** Control > Enable Simple Frame Actions: stop, play and goto during playback. */
function simpleActions(frame) {
  for (const a of JSON.parse(ed.simple_actions_at(frame))) {
    if (a.op === 'stop') {
      stop();
      return;
    }
    if (a.op === 'goto') {
      ed.set_frame(a.frame);
      S = JSON.parse(ed.state());
      if (!a.play) {
        stop();
        return;
      }
    }
  }
}

// Full screen with Keyboard Lock: this way the browser lets Ctrl+N, Ctrl+W and Ctrl+T through.
async function toggleFullscreen() {
  try {
    if (document.fullscreenElement) {
      navigator.keyboard?.unlock?.();
      await document.exitFullscreen();
    } else {
      await document.documentElement.requestFullscreen();
      await navigator.keyboard?.lock?.(['KeyN', 'KeyW', 'KeyT']);
      toast('Pantalla completa: Ctrl+N crea un documento nuevo. Mantén Esc para salir.');
    }
  } catch {
    toast('El navegador no permitió la pantalla completa');
  }
}

function stop() {
  if (!playing) return;
  playing = false;
  audio?.onStop();
  cancelAnimationFrame(playRAF);
  if (S) updateReadouts();
}

// ───────────────────────── Menus (the Flash 8 ones) ─────────────────────────

const ARRANGE_MENU = [
  { label: 'Traer al frente', key: 'Ctrl+Mayús+↑', cmd: 'front' },
  { label: 'Traer hacia delante', key: 'Ctrl+↑', cmd: 'forward' },
  { label: 'Enviar hacia atrás', key: 'Ctrl+↓', cmd: 'backward' },
  { label: 'Enviar al fondo', key: 'Ctrl+Mayús+↓', cmd: 'back' },
];
const TRANSFORM_MENU = [
  { label: 'Transformación libre', key: 'Q', cmd: 'xformFree' },
  { label: 'Distorsionar', cmd: 'xformDistort' },
  { label: 'Envoltura', cmd: 'xformEnvelope' },
  { label: 'Escalar', cmd: 'xformScale' },
  { label: 'Rotar y sesgar', cmd: 'xformRotate' },
  '-',
  { label: 'Girar 90° en sentido horario', key: 'Ctrl+Mayús+9', cmd: 'rot90cw' },
  { label: 'Girar 90° en sentido antihorario', key: 'Ctrl+Mayús+7', cmd: 'rot90ccw' },
  '-',
  { label: 'Voltear verticalmente', cmd: 'flipV' },
  { label: 'Voltear horizontalmente', cmd: 'flipH' },
];
const ALIGN_MENU = [
  { label: 'Izquierda', key: 'Ctrl+Alt+1', cmd: 'alignL' },
  { label: 'Centro horizontal', key: 'Ctrl+Alt+2', cmd: 'alignHC' },
  { label: 'Derecha', key: 'Ctrl+Alt+3', cmd: 'alignR' },
  '-',
  { label: 'Superior', key: 'Ctrl+Alt+4', cmd: 'alignT' },
  { label: 'Centro vertical', key: 'Ctrl+Alt+5', cmd: 'alignVC' },
  { label: 'Inferior', key: 'Ctrl+Alt+6', cmd: 'alignB' },
];
const soon = (label) => ({ label, disabled: true });

const MENUS = () => [
  ['Archivo', [
    { label: 'Nuevo…', key: 'Ctrl+Alt+N', cmd: 'newDoc' },
    { label: 'Abrir…', key: 'Ctrl+O', cmd: 'open' },
    '-',
    { label: 'Guardar', key: 'Ctrl+S', cmd: 'save' },
    '-',
    { label: 'Importar', sub: [
      { label: 'Importar al escenario…', key: 'Ctrl+R', cmd: 'importStage' },
      { label: 'Importar a biblioteca…', cmd: 'importLibrary' },
    ] },
    { label: 'Exportar', sub: [
      { label: 'Exportar imagen (PNG)…', cmd: 'exportPNG' },
      { label: 'Exportar película (SWF)…', key: 'Ctrl+Alt+Mayús+S', cmd: 'exportSWF' },
    ] },
    '-',
    { label: 'Configuración de publicación…', key: 'Ctrl+Mayús+F12', cmd: 'publishSettings' },
    { label: 'Vista previa de publicación', sub: [
      { label: 'Flash', key: 'F12', cmd: 'testMovie' },
      { label: 'HTML', key: 'Ctrl+F12', cmd: 'previewHTML' },
    ] },
    { label: 'Publicar', key: 'Mayús+F12', cmd: 'publish' },
  ]],
  ['Edición', [
    { label: 'Deshacer', key: 'Ctrl+Z', cmd: 'undo', disabled: !S.canUndo },
    { label: 'Rehacer', key: 'Ctrl+Y', cmd: 'redo', disabled: !S.canRedo },
    '-',
    { label: 'Cortar', key: 'Ctrl+X', cmd: 'cut', disabled: !S.sel },
    { label: 'Copiar', key: 'Ctrl+C', cmd: 'copy', disabled: !S.sel },
    { label: 'Pegar en el centro', key: 'Ctrl+V', cmd: 'paste', disabled: !S.clipboard },
    { label: 'Pegar in situ', key: 'Ctrl+Mayús+V', cmd: 'pasteInPlace', disabled: !S.clipboard },
    { label: 'Borrar', key: 'Supr', cmd: 'clear', disabled: !S.sel },
    '-',
    { label: 'Duplicar', key: 'Ctrl+D', cmd: 'duplicate', disabled: !S.sel },
    { label: 'Seleccionar todo', key: 'Ctrl+A', cmd: 'selectAll' },
    { label: 'Anular todas las selecciones', key: 'Ctrl+Mayús+A', cmd: 'deselect' },
    '-',
    { label: 'Editar selección', cmd: 'editSelected', disabled: !(S.selGroup || S.selObject) },
    { label: 'Editar en contexto', cmd: 'editSelected', disabled: !S.selInstance },
    { label: 'Editar todo', cmd: 'editAll', disabled: !S.scopes.length },
    '-',
    S.symPath.length ? { label: 'Editar documento', key: 'Ctrl+E', cmd: 'editDocument' } : { label: 'Editar símbolos', key: 'Ctrl+E', cmd: 'editSymbols', disabled: !S.library.length },
    '-',
    { label: 'Línea de tiempo', sub: [
      { label: 'Quitar fotogramas', key: 'Mayús+F5', cmd: 'removeFrame' },
      '-',
      { label: 'Cortar fotogramas', key: 'Ctrl+Alt+X', cmd: 'cutFrames' },
      { label: 'Copiar fotogramas', key: 'Ctrl+Alt+C', cmd: 'copyFrames' },
      { label: 'Pegar fotogramas', key: 'Ctrl+Alt+V', cmd: 'pasteFrames', disabled: !S.frameClip },
      { label: 'Borrar fotogramas', key: 'Alt+Retroceso', cmd: 'clearFrames' },
      { label: 'Borrar fotograma clave', key: 'Mayús+F6', cmd: 'clearKeyframe' },
      { label: 'Seleccionar todos los fotogramas', key: 'Ctrl+Alt+A', cmd: 'selectAllFrames' },
    ] },
    '-',
    { label: 'Métodos abreviados de teclado…', cmd: 'shortcuts' },
  ]],
  ['Ver', [
    { label: 'Acercar', key: 'Ctrl+=', cmd: 'zoomIn' },
    { label: 'Alejar', key: 'Ctrl+-', cmd: 'zoomOut' },
    { label: 'Aumentar', sub: [
      { label: '100%', key: 'Ctrl+1', cmd: 'zoom100' },
      { label: '200%', cmd: 'zoom200' },
      { label: '400%', key: 'Ctrl+4', cmd: 'zoom400' },
      { label: 'Mostrar fotograma', key: 'Ctrl+2', cmd: 'zoomFit' },
    ] },
    '-',
    { label: 'Papel cebolla', check: S.onion, cmd: 'toggleOnion' },
    { label: 'Línea de tiempo', key: 'Ctrl+Alt+T', check: !$('#timeline').hidden, cmd: 'toggleTimeline' },
    { label: 'Línea de tiempo clásica (colores de Flash 8)', check: !!prefs.classic, cmd: 'classicTimeline' },
    '-',
    { label: 'Reglas', key: 'Ctrl+Alt+Mayús+R', check: rulers.on, cmd: 'toggleRulers' },
    { label: 'Cuadrícula', sub: [
      { label: 'Mostrar cuadrícula', key: "Ctrl+'", check: S.showGrid, cmd: 'toggleGrid' },
      { label: 'Editar cuadrícula…', key: 'Ctrl+Alt+G', cmd: 'gridDialog' },
    ] },
    { label: 'Guías', sub: [
      { label: 'Mostrar guías', key: 'Ctrl+;', check: S.guides.show, cmd: 'toggleGuides' },
      { label: 'Bloquear guías', key: 'Ctrl+Alt+;', check: S.guides.lock, cmd: 'lockGuides' },
      { label: 'Editar guías…', key: 'Ctrl+Alt+Mayús+G', cmd: 'guidesDialog' },
      { label: 'Borrar guías', cmd: 'clearGuides', disabled: !S.guides.count },
    ] },
    { label: 'Ajuste', sub: [
      { label: 'Ajustar a objetos', key: 'Ctrl+Mayús+/', check: S.snapObjects, cmd: 'snapObjects' },
      { label: 'Ajustar a cuadrícula', key: "Ctrl+Mayús+'", check: S.snapGrid, cmd: 'snapGrid' },
      { label: 'Ajustar a guías', key: 'Ctrl+Mayús+;', check: S.guides.snap, cmd: 'snapGuides' },
      { label: 'Ajustar a píxeles', check: S.snapPixels, cmd: 'snapPixels' },
    ] },
    '-',
    { label: 'Mostrar consejos de forma', key: 'Ctrl+Alt+H', check: S.showHints, cmd: 'toggleHints' },
    '-',
    { label: 'Pantalla completa (activa Ctrl+N y Ctrl+W)', key: 'Ctrl+Alt+Mayús+F', check: !!document.fullscreenElement, cmd: 'fullscreen' },
  ]],
  ['Insertar', [
    { label: 'Nuevo símbolo…', key: 'Ctrl+F8', cmd: 'newSymbol' },
    '-',
    { label: 'Línea de tiempo', sub: [
      { label: 'Capa', cmd: 'addLayer' },
      { label: 'Carpeta de capas', cmd: 'addFolder' },
      { label: 'Guía de movimiento', cmd: 'addMotionGuide' },
      '-',
      { label: 'Fotograma', key: 'F5', cmd: 'insertFrame' },
      { label: 'Fotograma clave', key: 'F6', cmd: 'insertKeyframe' },
      { label: 'Fotograma clave vacío', key: 'F7', cmd: 'insertBlankKeyframe' },
    ] },
    '-',
    { label: 'Interpolación de movimiento', cmd: 'createTween', disabled: !S.frameInfo },
    { label: 'Interpolación de forma', cmd: 'createShapeTween', disabled: !S.frameInfo },
    '-',
    { label: 'Escena', cmd: 'sceneAdd' },
  ]],
  ['Modificar', [
    { label: 'Documento…', key: 'Ctrl+J', cmd: 'docProps' },
    '-',
    { label: 'Convertir en símbolo…', key: 'F8', cmd: 'convertSymbol', disabled: !S.sel },
    { label: 'Separar', key: 'Ctrl+B', cmd: 'breakApart', disabled: !(S.selGroup || S.selObject || S.selInstance) },
    '-',
    { label: 'Símbolo', sub: [
      { label: 'Intercambiar símbolo…', cmd: 'swapSymbol', disabled: !S.inst },
      { label: 'Duplicar símbolo', action: () => cmd(() => S.inst && ed.lib_duplicate(S.inst.symbol)), disabled: !S.inst },
    ] },
    '-',
    { label: 'Forma', sub: [
      { label: 'Suavizar', cmd: 'smooth', disabled: !S.sel },
      { label: 'Enderezar', cmd: 'straighten', disabled: !S.sel },
      { label: 'Optimizar…', key: 'Ctrl+Alt+Mayús+C', cmd: 'optimizeDialog', disabled: !S.sel },
      '-',
      { label: 'Convertir líneas en rellenos', cmd: 'linesToFills', disabled: !S.sel },
      { label: 'Expandir relleno…', cmd: 'expandDialog', disabled: !S.sel },
      { label: 'Suavizar bordes de relleno…', cmd: 'softenDialog', disabled: !S.sel },
      '-',
      { label: 'Añadir consejo de forma', key: 'Ctrl+Mayús+H', cmd: 'addShapeHint' },
      { label: 'Quitar todos los consejos', cmd: 'removeHints' },
    ] },
    { label: 'Línea de tiempo', sub: [
      { label: 'Invertir fotogramas', cmd: 'reverseFrames' },
      { label: 'Convertir en fotogramas clave', cmd: 'convertKeyframes' },
      { label: 'Convertir en fotogramas clave vacíos', cmd: 'convertBlankKeyframes' },
      '-',
      { label: 'Propiedades de capa…', cmd: 'layerPropsCmd' },
      { label: 'Capa arriba', cmd: 'layerUp' },
      { label: 'Capa abajo', cmd: 'layerDown' },
    ] },
    { label: 'Transformar', sub: TRANSFORM_MENU },
    { label: 'Organizar', sub: ARRANGE_MENU },
    { label: 'Alinear', sub: ALIGN_MENU },
    '-',
    { label: 'Agrupar', key: 'Ctrl+G', cmd: 'group', disabled: !S.sel },
    { label: 'Desagrupar', key: 'Ctrl+Mayús+G', cmd: 'ungroup', disabled: !S.selGroup },
  ]],
  ['Texto', txt.menu([
    '-',
    { label: 'Revisar ortografía…', action: () => spell.check() },
    { label: 'Configuración de revisión ortográfica…', action: () => spell.settingsDialog() },
  ])],
  ['Comandos', hist.menu()],
  ['Control', [
    { label: playing ? 'Detener' : 'Reproducir', key: 'Intro', cmd: 'togglePlay' },
    { label: 'Rebobinar', key: 'Ctrl+Alt+R', cmd: 'firstFrame' },
    { label: 'Ir al final', cmd: 'lastFrame' },
    '-',
    { label: 'Avanzar un fotograma', key: '.', cmd: 'nextFrame' },
    { label: 'Retroceder un fotograma', key: ',', cmd: 'prevFrame' },
    '-',
    { label: 'Probar película', key: 'Ctrl+Intro', cmd: 'testMovie' },
    '-',
    { label: 'Habilitar acciones de fotograma simples', key: 'Ctrl+Alt+F', check: !!prefs.simpleActions, cmd: 'simpleActions' },
    { label: 'Habilitar botones simples', key: 'Ctrl+Alt+B', check: S.simpleButtons, cmd: 'simpleButtons' },
  ]],
  ['Ventana', [
    { label: 'Línea de tiempo', key: 'Ctrl+Alt+T', check: !$('#timeline').hidden, cmd: 'toggleTimeline' },
    { label: 'Herramientas', check: true, disabled: true },
    { label: 'Propiedades', key: 'Ctrl+F3', check: panelState.props !== false, action: () => run('togglePanel', 'props') },
    '-',
    { label: 'Alinear', key: 'Ctrl+K', check: panelState.align !== false, action: () => run('togglePanel', 'align') },
    { label: 'Mezclador de colores', key: 'Mayús+F9', check: panelState.mixer !== false, action: () => run('togglePanel', 'mixer') },
    { label: 'Muestras de color', key: 'Ctrl+F9', check: panelState.swatches !== false, action: () => run('togglePanel', 'swatches') },
    { label: 'Acciones', key: 'F9', check: acts.visible('actions'), cmd: 'actionsPanel' },
    { label: 'Salida', key: 'F2', check: acts.visible('output'), cmd: 'outputPanel' },
    { label: 'Biblioteca', key: 'Ctrl+L', check: panelState.library !== false, cmd: 'libraryPanel' },
    { label: 'Filtros', check: panelState.filters !== false, cmd: 'filtersPanel' },
    { label: 'Otros paneles', sub: [
      { label: 'Escena', key: 'Mayús+F2', check: panelState.scene !== false, cmd: 'scenePanel' },
      { label: 'Historial', key: 'Ctrl+F10', check: panelState.history !== false, cmd: 'historyPanel' },
    ] },
  ]],
  ['Ayuda', [
    { label: 'Métodos abreviados de teclado', cmd: 'shortcuts' },
    '-',
    { label: 'Acerca de Open McFlash', cmd: 'about' },
  ]],
];

const menusEl = $('#menus');
let openMenu = null;

function buildMenubar() {
  menusEl.innerHTML = MENUS().map(([name], i) => `<div class="menu-btn" data-m="${i}">${name}</div>`).join('');
  menusEl.addEventListener('pointerdown', (e) => {
    const b = e.target.closest('.menu-btn');
    if (!b) return;
    e.stopPropagation();
    if (openMenu === b) closeMenus();
    else showMenu(b);
  });
  menusEl.addEventListener('pointerover', (e) => {
    const b = e.target.closest('.menu-btn');
    if (b && openMenu && openMenu !== b) showMenu(b);
  });
}

function showMenu(btn) {
  closeMenus();
  stop();
  openMenu = btn;
  btn.classList.add('open');
  const items = MENUS()[+btn.dataset.m][1];
  const r = btn.getBoundingClientRect();
  dropdown(items, r.left, r.bottom, 0);
}

const dropdowns = [];
function dropdown(items, x, y, level) {
  while (dropdowns.length > level) dropdowns.pop().remove();
  const el = document.createElement('div');
  el.className = 'menu-dropdown';
  el.innerHTML = items.map((it, i) => it === '-'
    ? '<div class="menu-sep"></div>'
    : `<div class="menu-item${it.disabled ? ' disabled' : ''}${it.sub ? ' sub' : ''}" data-i="${i}">
         ${it.check ? '<span class="check">✓</span>' : ''}<span>${it.label}</span>${it.key ? `<span class="key">${fmtKey(it.key)}</span>` : ''}</div>`).join('');
  document.body.appendChild(el);
  const w = el.offsetWidth, h = el.offsetHeight;
  el.style.left = Math.min(x, innerWidth - w - 4) + 'px';
  el.style.top = Math.min(y, innerHeight - h - 4) + 'px';
  dropdowns.push(el);
  el.addEventListener('pointerover', (e) => {
    const row = e.target.closest('.menu-item');
    if (!row) return;
    const it = items[+row.dataset.i];
    if (it.sub && !it.disabled) {
      const rr = row.getBoundingClientRect();
      dropdown(it.sub, rr.right - 2, rr.top - 4, level + 1);
    } else {
      while (dropdowns.length > level + 1) dropdowns.pop().remove();
    }
  });
  el.addEventListener('pointerdown', (e) => e.stopPropagation());
  el.addEventListener('click', (e) => {
    const row = e.target.closest('.menu-item');
    if (!row) return;
    const it = items[+row.dataset.i];
    if (it.disabled || it.sub) {
      if (it.disabled && !it.sub) toast('Llegará en la siguiente fase');
      return;
    }
    closeMenus();
    if (it.action) it.action();
    else if (it.cmd) run(it.cmd);
  });
  return el;
}

function contextMenu(x, y, items) {
  closeMenus();
  stop();
  dropdown(items, x, y, 0);
}

function closeMenus() {
  while (dropdowns.length) dropdowns.pop().remove();
  if (openMenu) openMenu.classList.remove('open');
  openMenu = null;
}
document.addEventListener('pointerdown', () => closeMenus());

const fmtKey = (k) => (isMac ? k.replace(/Ctrl/g, '⌘').replace(/Alt/g, '⌥') : k);

// ───────────────────────── Keyboard (Flash 8 shortcuts) ─────────────────────────

const SHORTCUTS = {
  'Ctrl+Alt+N': 'newDoc', 'Ctrl+O': 'open', 'Ctrl+S': 'save', 'Ctrl+Alt+Shift+S': 'exportSWF',
  'Ctrl+Z': 'undo', 'Ctrl+Y': 'redo', 'Ctrl+Shift+Z': 'redo',
  'Ctrl+X': 'cut', 'Ctrl+C': 'copy', 'Ctrl+V': 'paste', 'Ctrl+Shift+V': 'pasteInPlace',
  'Ctrl+D': 'duplicate', 'Ctrl+A': 'selectAll', 'Ctrl+Shift+A': 'deselect', 'Delete': 'clear', 'Backspace': 'clear',
  'F5': 'insertFrame', 'Shift+F5': 'removeFrame', 'F6': 'insertKeyframe', 'Shift+F6': 'clearKeyframe', 'F7': 'insertBlankKeyframe',
  'Ctrl+Alt+C': 'copyFrames', 'Ctrl+Alt+X': 'cutFrames', 'Ctrl+Alt+V': 'pasteFrames', 'Alt+Backspace': 'clearFrames',
  'Ctrl+Alt+A': 'selectAllFrames', 'Ctrl+Shift+H': 'addShapeHint', 'Ctrl+Alt+H': 'toggleHints', 'Shift+F2': 'scenePanel',
  'Enter': 'togglePlay', 'Ctrl+Enter': 'testMovie', 'F12': 'testMovie', 'Ctrl+Alt+R': 'firstFrame',
  ',': 'prevFrame', '.': 'nextFrame',
  'Ctrl+=': 'zoomIn', 'Ctrl+-': 'zoomOut', 'Ctrl+1': 'zoom100', 'Ctrl+2': 'zoomFit', 'Ctrl+4': 'zoom400',
  'Ctrl+J': 'docProps', 'Ctrl+Alt+T': 'toggleTimeline', 'Ctrl+Alt+Shift+F': 'fullscreen', 'Ctrl+Alt+F': 'simpleActions', 'F9': 'actionsPanel', 'F2': 'outputPanel',
  // These only arrive here in full screen (Keyboard Lock); otherwise the browser keeps them.
  'Ctrl+N': 'newDoc',
  'Ctrl+G': 'group', 'Ctrl+Shift+G': 'ungroup', 'Ctrl+B': 'breakApart', 'J': 'toggleObjectMode',
  'Ctrl+Alt+Shift+R': 'toggleRulers', 'Ctrl+;': 'toggleGuides', 'Ctrl+Alt+;': 'lockGuides', 'Ctrl+Shift+;': 'snapGuides', 'Ctrl+Alt+Shift+G': 'guidesDialog',
  "Ctrl+'": 'toggleGrid', "Ctrl+Shift+'": 'snapGrid', 'Ctrl+Shift+/': 'snapObjects', 'Ctrl+Alt+G': 'gridDialog',
  'Ctrl+Alt+Shift+C': 'optimizeDialog',
  'Ctrl+R': 'importStage', 'Ctrl+Shift+F12': 'publishSettings', 'Shift+F12': 'publish', 'Ctrl+F12': 'previewHTML', 'Ctrl+F10': 'historyPanel',
  'F8': 'convertSymbol', 'Ctrl+F8': 'newSymbol', 'Ctrl+E': 'editSymbols', 'Ctrl+L': 'libraryPanel', 'Ctrl+Alt+B': 'simpleButtons',
  'Ctrl+Shift+Up': 'front', 'Ctrl+Up': 'forward', 'Ctrl+Down': 'backward', 'Ctrl+Shift+Down': 'back',
  'Ctrl+Shift+9': 'rot90cw', 'Ctrl+Shift+7': 'rot90ccw',
  'Ctrl+Alt+1': 'alignL', 'Ctrl+Alt+2': 'alignHC', 'Ctrl+Alt+3': 'alignR',
  'Ctrl+Alt+4': 'alignT', 'Ctrl+Alt+5': 'alignVC', 'Ctrl+Alt+6': 'alignB',
};

const CODE_NAMES = { Semicolon: ';', Comma: ',', Period: '.', Equal: '=', Minus: '-', Quote: "'", Slash: '/', Backslash: '\\', NumpadAdd: '=', NumpadSubtract: '-', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right', NumpadEnter: 'Enter' };

function comboOf(e) {
  let k = CODE_NAMES[e.code];
  if (!k) {
    if (/^Key[A-Z]$/.test(e.code)) k = e.code.slice(3);
    else if (/^Digit\d$/.test(e.code)) k = e.code.slice(5);
    else k = e.key;
  }
  return (e.ctrlKey || e.metaKey ? 'Ctrl+' : '') + (e.altKey ? 'Alt+' : '') + (e.shiftKey ? 'Shift+' : '') + k;
}

const typing = () => {
  const a = document.activeElement;
  return a && (a.tagName === 'INPUT' || a.tagName === 'SELECT' || a.tagName === 'TEXTAREA' || a.isContentEditable);
};

document.addEventListener('keydown', (e) => {
  if (!$('#modal-back').hidden) {
    if (e.key === 'Escape') closeModal();
    return;
  }
  if (typing()) {
    if (e.key === 'Escape' || e.key === 'Enter') document.activeElement.blur();
    return;
  }
  // Pen: Enter or Esc finish the path (as in Photoshop) instead of playing/deselecting.
  if (S.penActive && (e.key === 'Enter' || e.key === 'Escape')) {
    e.preventDefault();
    ed.pen_finish();
    refresh();
    return;
  }
  // Polygon Lasso: Enter closes the polygon; Esc discards it.
  if (S.lassoActive && (e.key === 'Enter' || e.key === 'Escape')) {
    e.preventDefault();
    if (e.key === 'Enter') ed.lasso_finish();
    else ed.set_lasso_polygon(true);
    refresh();
    return;
  }
  if (e.key === 'Escape') {
    closeMenus();
    hidePopover();
    if (txt.editing) return txt.commit();
    ed.deselect();
    refresh();
    return;
  }
  if (e.code === 'Space') {
    e.preventDefault();
    if (!e.repeat) { ed.set_temp_hand(true); canvas.style.cursor = 'grab'; }
    return;
  }
  const combo = comboOf(e);
  if (txt.shortcut(combo)) {
    e.preventDefault();
    return;
  }
  if (SHORTCUTS[combo]) {
    e.preventDefault();
    closeMenus();
    run(SHORTCUTS[combo]);
    return;
  }
  const arrows = { Left: [-1, 0], Right: [1, 0], Up: [0, -1], Down: [0, 1] };
  const base = combo.replace('Shift+', '');
  if (arrows[base] && S.sel) {
    e.preventDefault();
    const m = e.shiftKey ? 10 : 1;
    cmd(() => ed.nudge(arrows[base][0] * m, arrows[base][1] * m));
    return;
  }
  if (!e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey) {
    const t = TOOL_KEYS[combo];
    if (t) {
      const def = ALL_TOOLS.find((x) => x[0] === t);
      if (def[3]) setTool(t);
      else toast(`${def[1]}: llegará en la siguiente fase`);
    }
  }
});

document.addEventListener('keyup', (e) => {
  if (e.code === 'Space') {
    ed.set_temp_hand(false);
    canvas.style.cursor = '';
  }
});
window.addEventListener('blur', () => ed.set_temp_hand(false));

// ───────────────────────── Dialogs ─────────────────────────

function modal(title, html, onReady) {
  $('#modal-title').textContent = title;
  $('#modal-body').innerHTML = html;
  $('#modal-back').hidden = false;
  closeMenus();
  stop();
  onReady && onReady($('#modal-body'));
}

let modalCleanup = null;
function closeModal() {
  $('#modal-back').hidden = true;
  $('#modal-body').innerHTML = '';
  if (modalCleanup) modalCleanup();
  modalCleanup = null;
}
$('#modal-back').addEventListener('click', (e) => {
  if (e.target.matches('[data-close]') || e.target.id === 'modal-back') closeModal();
});

function docDialog() {
  let bg = S.bg;
  modal('Propiedades del documento', `
    <div class="prow"><span class="lab" style="width:110px">Dimensiones</span><input type="number" id="dd-w" value="${S.width}" min="1" max="2880"> <span class="muted">(ancho)</span> × <input type="number" id="dd-h" value="${S.height}" min="1" max="2880"> <span class="muted">(alto) px</span></div>
    <div class="prow"><span class="lab" style="width:110px">Color de fondo</span><div class="chip" id="dd-bg"><i></i></div></div>
    <div class="prow"><span class="lab" style="width:110px">Velocidad</span><input type="number" id="dd-fps" value="${S.fps}" min="1" max="120"> <span class="muted">fps</span></div>
    <div class="prow"><span class="lab" style="width:110px">Exportar</span><label style="display:flex;gap:6px;align-items:center"><input type="checkbox" id="dd-hidden" ${S.exportHidden ? 'checked' : ''}> Incluir capas ocultas (como Flash 8)</label></div>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="dd-ok">Aceptar</button></div>`, (b) => {
    paintChip($('#dd-bg', b), bg);
    $('#dd-bg', b).addEventListener('click', (e) => openColorPopover(e.currentTarget, { hex: bg, allowNone: false, noAlpha: true }, (h) => { bg = h; paintChip($('#dd-bg'), bg); }));
    $('#dd-ok', b).addEventListener('click', () => {
      const w = clamp(+$('#dd-w').value || 550, 1, 2880), h = clamp(+$('#dd-h').value || 400, 1, 2880);
      const changedSize = w !== S.width || h !== S.height;
      ed.set_document(w, h, bg, clamp(+$('#dd-fps').value || 12, 1, 120));
      if ($('#dd-hidden').checked !== S.exportHidden) ed.set_export_hidden($('#dd-hidden').checked);
      if (changedSize) ed.fit_stage();
      closeModal();
      refresh();
    });
  });
}

function newDocDialog() {
  modal('Nuevo documento', `
    <div class="prow"><span class="lab" style="width:90px">Preajuste</span><select id="nd-pre">
      <option value="550x400">Flash por defecto — 550 × 400</option><option value="640x480">640 × 480</option>
      <option value="800x600">800 × 600</option><option value="1280x720">HD — 1280 × 720</option><option value="1920x1080">Full HD — 1920 × 1080</option>
      <option value="1080x1080">Cuadrado — 1080 × 1080</option></select></div>
    <div class="prow"><span class="lab" style="width:90px">Tamaño</span><input type="number" id="nd-w" value="550"> × <input type="number" id="nd-h" value="400"> px</div>
    <p class="muted" style="max-width:360px">El documento actual se reemplazará. Guarda antes (Ctrl+S) si quieres conservarlo.</p>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="nd-ok">Crear</button></div>`, (b) => {
    $('#nd-pre', b).addEventListener('change', (e) => {
      const [w, h] = e.target.value.split('x');
      $('#nd-w').value = w;
      $('#nd-h').value = h;
    });
    $('#nd-ok', b).addEventListener('click', () => {
      ed.new_document(+$('#nd-w').value || 550, +$('#nd-h').value || 400);
      docName = nextUntitled();
      closeModal();
      refresh();
    });
  });
}

let untitled = 1;
const nextUntitled = () => `Sin título-${++untitled}`;

function promptDialog(title, label, value, onOk) {
  modal(title, `
    <div class="prow"><span class="lab" style="width:150px">${label}</span><input type="number" id="pd-v" value="${value}"></div>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="pd-ok">Aceptar</button></div>`, (b) => {
    const inp = $('#pd-v', b);
    inp.focus();
    inp.select();
    const ok = () => { const v = inp.value; closeModal(); onOk(v); };
    $('#pd-ok', b).addEventListener('click', ok);
    inp.addEventListener('keydown', (e) => e.key === 'Enter' && ok());
  });
}

// PolyStar tool options.
function polystarDialog() {
  const p = S.polystar;
  modal('Configuración de herramienta', `
    <div class="prow"><span class="lab" style="width:150px">Estilo</span><select id="ps-style"><option value="polygon">polígono</option><option value="star">estrella</option></select></div>
    <div class="prow"><span class="lab" style="width:150px">Número de lados</span><input type="number" id="ps-sides" min="3" max="32" value="${p.sides}"></div>
    <div class="prow"><span class="lab" style="width:150px">Tamaño del vértice de la estrella</span><input type="number" id="ps-point" min="0" max="1" step="0.05" value="${p.point}"></div>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="ps-ok">Aceptar</button></div>`, (b) => {
    $('#ps-style', b).value = p.style;
    $('#ps-ok', b).addEventListener('click', () => {
      ed.set_polystar($('#ps-style', b).value, clamp(+$('#ps-sides', b).value || 5, 3, 32), clamp(+$('#ps-point', b).value, 0, 1));
      closeModal();
      refresh();
    });
  });
}

// ───────── Modify > Shape and Grid ─────────

function optimizeDialog() {
  modal('Optimizar curvas', `
    <div class="prow"><span class="lab" style="width:110px">Suavizado</span><input type="range" id="op-r" min="0" max="100" value="50" style="width:160px"><input type="number" id="op-v" min="0" max="100" value="50" style="width:56px"></div>
    <p class="muted" style="max-width:340px">Reduce el número de curvas de las formas seleccionadas. Más suavizado = menos puntos y formas más simples.</p>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="op-ok">Aceptar</button></div>`, (b) => {
    const r = $('#op-r', b), v = $('#op-v', b);
    r.addEventListener('input', () => (v.value = r.value));
    v.addEventListener('input', () => (r.value = v.value));
    $('#op-ok', b).addEventListener('click', () => {
      const msg = ed.optimize_curves(clamp(+v.value, 0, 100));
      closeModal();
      refresh();
      toast(msg);
    });
  });
}

function expandDialog() {
  modal('Expandir relleno', `
    <div class="prow"><span class="lab" style="width:110px">Distancia</span><input type="number" id="ex-d" value="4" min="0.1" step="0.5"> px</div>
    <div class="prow"><span class="lab" style="width:110px">Dirección</span>
      <label style="display:flex;gap:5px;align-items:center"><input type="radio" name="ex-dir" value="1" checked> Expandir</label>
      <label style="display:flex;gap:5px;align-items:center;margin-left:10px"><input type="radio" name="ex-dir" value="0"> Contraer</label></div>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="ex-ok">Aceptar</button></div>`, (b) => {
    $('#ex-ok', b).addEventListener('click', () => {
      const exp = $('input[name=ex-dir]:checked', b).value === '1';
      ed.expand_fill(clamp(+$('#ex-d', b).value || 1, 0.1, 500), exp);
      closeModal();
      refresh();
    });
  });
}

function softenDialog() {
  modal('Suavizar bordes de relleno', `
    <div class="prow"><span class="lab" style="width:130px">Distancia</span><input type="number" id="sf-d" value="4" min="0.5" step="0.5"> px</div>
    <div class="prow"><span class="lab" style="width:130px">Número de pasos</span><input type="number" id="sf-s" value="4" min="1" max="50"></div>
    <div class="prow"><span class="lab" style="width:130px">Dirección</span>
      <label style="display:flex;gap:5px;align-items:center"><input type="radio" name="sf-dir" value="1" checked> Expandir</label>
      <label style="display:flex;gap:5px;align-items:center;margin-left:10px"><input type="radio" name="sf-dir" value="0"> Contraer</label></div>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="sf-ok">Aceptar</button></div>`, (b) => {
    $('#sf-ok', b).addEventListener('click', () => {
      const exp = $('input[name=sf-dir]:checked', b).value === '1';
      ed.soften_edges(clamp(+$('#sf-d', b).value || 4, 0.5, 200), clamp(+$('#sf-s', b).value || 4, 1, 50), exp);
      closeModal();
      refresh();
    });
  });
}

/** Changes guide options while keeping the others. */
function setGuides(patch) {
  const g = { ...S.guides, ...patch };
  const n = parseInt(g.color.slice(1), 16);
  ed.set_guide_opts(JSON.stringify({ show: g.show, lock: g.lock, snap: g.snap, accuracy: g.accuracy, color: { r: n >> 16, g: (n >> 8) & 255, b: n & 255, a: 255 } }));
}

function guidesDialog() {
  let color = S.guides.color;
  const g = S.guides;
  modal('Guías', `
    <div class="prow"><span class="lab" style="width:130px">Color</span><div class="chip" id="gu-c"><i></i></div></div>
    <div class="prow"><span class="lab" style="width:130px"></span><label class="chk"><input type="checkbox" id="gu-show" ${g.show ? 'checked' : ''}> Mostrar guías</label></div>
    <div class="prow"><span class="lab" style="width:130px"></span><label class="chk"><input type="checkbox" id="gu-snap" ${g.snap ? 'checked' : ''}> Ajustar a guías</label></div>
    <div class="prow"><span class="lab" style="width:130px"></span><label class="chk"><input type="checkbox" id="gu-lock" ${g.lock ? 'checked' : ''}> Bloquear guías</label></div>
    <div class="prow"><span class="lab" style="width:130px">Precisión de ajuste</span><select id="gu-acc"><option value="0">Debe estar cerca</option><option value="1">Normal</option><option value="2">Puede estar lejos</option></select></div>
    <div class="modal-actions"><button class="btn" id="gu-clear">Borrar todo</button><div class="grow"></div><button class="btn" data-close>Cancelar</button><button class="btn primary" id="gu-ok">Aceptar</button></div>`, (b) => {
    paintChip($('#gu-c', b), color);
    $('#gu-acc', b).value = String(g.accuracy);
    $('#gu-c', b).addEventListener('click', (e) => openColorPopover(e.currentTarget, { hex: color, allowNone: false, noAlpha: true }, (h) => { color = h; paintChip($('#gu-c'), color); }));
    $('#gu-clear', b).addEventListener('click', () => { ed.clear_guides(); refresh(); });
    $('#gu-ok', b).addEventListener('click', () => {
      setGuides({ color, show: $('#gu-show', b).checked, snap: $('#gu-snap', b).checked, lock: $('#gu-lock', b).checked, accuracy: +$('#gu-acc', b).value });
      closeModal();
      refresh();
    });
  });
}

function gridDialog() {
  let color = S.gridColor;
  modal('Cuadrícula', `
    <div class="prow"><span class="lab" style="width:110px">Color</span><div class="chip" id="gd-c"><i></i></div></div>
    <div class="prow"><span class="lab" style="width:110px"></span><label style="display:flex;gap:6px;align-items:center"><input type="checkbox" id="gd-show" ${S.showGrid ? 'checked' : ''}> Mostrar cuadrícula</label></div>
    <div class="prow"><span class="lab" style="width:110px"></span><label style="display:flex;gap:6px;align-items:center"><input type="checkbox" id="gd-snap" ${S.snapGrid ? 'checked' : ''}> Ajustar a cuadrícula</label></div>
    <div class="prow"><span class="lab" style="width:110px">Espaciado</span><input type="number" id="gd-size" value="${S.gridSize}" min="1" max="500"> px</div>
    <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="gd-ok">Aceptar</button></div>`, (b) => {
    paintChip($('#gd-c', b), color);
    $('#gd-c', b).addEventListener('click', (e) => openColorPopover(e.currentTarget, { hex: color, allowNone: false, noAlpha: true }, (h) => { color = h; paintChip($('#gd-c'), color); }));
    $('#gd-ok', b).addEventListener('click', () => {
      ed.set_grid($('#gd-show', b).checked, clamp(+$('#gd-size', b).value || 18, 1, 500), color);
      ed.set_snap('grid', $('#gd-snap', b).checked);
      closeModal();
      refresh();
    });
  });
}

function shortcutsDialog() {
  const rows = [
    ['V / A / Q', 'Selección / Selección directa / Transformación libre'], ['N / P / Y / B', 'Línea / Pluma / Lápiz / Pincel'], ['O / R', 'Óvalo / Rectángulo'],
    ['J', 'Dibujo de objetos (activar / desactivar)'], ['Ctrl+G / Ctrl+Mayús+G', 'Agrupar / desagrupar'], ['Ctrl+B', 'Separar'],
    ['Doble clic', 'Entrar en un grupo · seleccionar lo conectado'], ["Ctrl+' / Ctrl+Mayús+'", 'Mostrar cuadrícula / ajustar a cuadrícula'],
    ['F9 / F2', 'Paneles Acciones / Salida'], ['Ctrl+Alt+F', 'Acciones de fotograma simples'],
    ['Ctrl+Mayús+/', 'Ajustar a objetos'], ['Ctrl+Alt+Mayús+R', 'Reglas'], ['Ctrl+; / Ctrl+Alt+;', 'Mostrar / bloquear guías'], ['Ctrl+Mayús+;', 'Ajustar a guías'],
    ['Ctrl+Alt+C / X / V', 'Copiar / cortar / pegar fotogramas'], ['Alt+Retroceso', 'Borrar fotogramas'], ['Ctrl+Alt+A', 'Seleccionar todos los fotogramas'],
    ['Ctrl+Mayús+H / Ctrl+Alt+H', 'Añadir / mostrar consejos de forma'], ['Mayús+F2', 'Panel Escena'],
    ['Intro / Esc (pluma)', 'Terminar el trazado'], ['Ctrl+Z (pluma)', 'Quitar el último punto'],
    ['S / K / I / E', 'Bote de tinta / Cubo / Cuentagotas / Borrador'], ['H / Z / Espacio', 'Mano / Zoom / Mano temporal'],
    ['F5 / Mayús+F5', 'Insertar / quitar fotograma'], ['F6 / F7', 'Fotograma clave / clave vacío'], ['Mayús+F6', 'Borrar fotograma clave'],
    ['Intro', 'Reproducir / detener'], [', / .', 'Fotograma anterior / siguiente'], ['Ctrl+Intro', 'Probar película'],
    ['Ctrl+Z / Ctrl+Y', 'Deshacer / rehacer'], ['Ctrl+C / X / V', 'Copiar / cortar / pegar'], ['Ctrl+Mayús+V', 'Pegar in situ'],
    ['T / L / F', 'Texto / Lazo / Transformación de relleno'], ['Ctrl+Mayús+B / I', 'Negrita / cursiva'], ['Ctrl+Mayús+L / C / R / J', 'Alinear texto'],
    ['F8 / Ctrl+F8', 'Convertir en símbolo / nuevo símbolo'], ['Ctrl+E', 'Editar símbolos / editar documento'], ['Ctrl+L', 'Biblioteca'],
    ['Ctrl+Alt+B', 'Habilitar botones simples'],
    ['Ctrl+D', 'Duplicar'], ['Flechas (Mayús)', 'Mover 1 px (10 px)'], ['Ctrl+(Mayús)+↑/↓', 'Organizar'], ['Ctrl+J', 'Propiedades del documento'],
    ['Ctrl+rueda', 'Zoom'], ['Ctrl+1 / Ctrl+2', '100% / Mostrar fotograma'],
  ];
  modal('Métodos abreviados de teclado', `<div class="keys">${rows.map(([k, v]) => `<b>${fmtKey(k)}</b><span>${v}</span>`).join('')}</div>`);
}

function aboutDialog() {
  modal('Acerca de Open McFlash', `<div class="about">
    <p><b>Open McFlash</b> (OMF) es un editor de animación vectorial que recupera el flujo de trabajo de Macromedia Flash 8 (línea de tiempo, F5/F6/F7, interpolaciones de movimiento, papel cebolla, «Probar película»), con el núcleo escrito en Rust y compilado a WebAssembly.</p>
    <p>Es una reimplementación independiente: no contiene código ni recursos de Macromedia ni de Adobe. Exporta a SWF siguiendo la especificación pública del formato.</p>
    <p class="muted">Incluye las fuentes Arimo, Tinos y Cousine (SIL Open Font License 1.1), nspell (MIT) y los diccionarios Hunspell de español (MPL 1.1 / LGPL / GPL) e inglés (MIT y BSD). «Probar película» y la página publicada usan Ruffle (MIT / Apache 2.0), incluido en la aplicación.</p>
    <p class="muted">Flash y Macromedia son marcas de Adobe.</p></div>`);
}

// "Test Movie" (Ctrl+Enter): the exported SWF in Ruffle, with trace() calls and
// ActionScript errors in the Output panel (like the test Flash Player).
async function testMovie() {
  stop();
  acts.commit();
  const swf = ed.export_swf();
  const errors = JSON.parse(ed.compile_errors());
  acts.output.clear();
  if (errors.length) {
    errors.forEach((e) => acts.output.append(e));
    acts.show('output');
  }
  const maxW = innerWidth * 0.8, maxH = innerHeight * 0.72;
  const scale = Math.min(maxW / S.width, maxH / S.height, 2);
  const cw = Math.round(S.width * scale), ch = Math.round(S.height * scale);
  modal(`${docName}.swf`, `
    <div class="player" id="tm-host" style="width:${cw}px;height:${ch}px"><div class="muted" id="tm-loading">Cargando el reproductor…</div></div>
    <div class="player-bar">
      <button class="ib" id="tm-restart" title="Reiniciar"><svg viewBox="0 0 20 20"><path d="M5 5v10M15 5l-7 5 7 5z" class="f"/></svg></button>
      <button class="ib on" id="tm-pause" title="Pausa / continuar"><svg viewBox="0 0 20 20"><rect x="5" y="4" width="3.5" height="12"/><rect x="11.5" y="4" width="3.5" height="12"/></svg></button>
      <div class="grow"></div>
      <span class="muted" id="tm-info">${S.fps} fps · Ruffle</span>
    </div>`);
  let ruffle;
  try {
    ruffle = await loadRuffle();
  } catch (err) {
    closeModal();
    toast('No se pudo cargar Ruffle: se usa el reproductor interno (sin ActionScript)');
    return testMovieInternal();
  }
  const host = $('#tm-host');
  if (!host) return; // closed while loading
  const p = ruffle.createPlayer();
  p.id = 'tm-player';
  p.style.width = cw + 'px';
  p.style.height = ch + 'px';
  host.replaceChildren(p);
  const api = p.ruffle();
  // trace() calls arrive through Ruffle's log (its "log_adapter"), including those from the first frame.
  const log = console.log;
  console.log = (...args) => {
    const first = String(args[0] ?? '');
    const m = first.match(/log_adapter\.rs:\d+%c ?([\s\S]*)$/);
    if (m) {
      acts.output.append(m[1]);
      if (!acts.visible('output')) acts.show('output');
    }
    return log.apply(console, args);
  };
  const renderer = loadPref('omf.ruffle', { renderer: null }).renderer;
  await api.load({ data: swf, ...(renderer ? { preferredRenderer: renderer } : {}), backgroundColor: S.bg });
  let paused = false;
  $('#tm-pause').addEventListener('click', () => {
    paused = !paused;
    paused ? api.suspend() : api.resume();
    $('#tm-pause').classList.toggle('on', !paused);
  });
  $('#tm-restart').addEventListener('click', () => api.load({ data: swf, ...(renderer ? { preferredRenderer: renderer } : {}), backgroundColor: S.bg }));
  modalCleanup = () => {
    try { api.suspend(); } catch {}
    p.remove();
    console.log = log;
  };
}

// Built-in player (no ActionScript), in case Ruffle fails to load.
function testMovieInternal() {
  stop();
  const dpr = window.devicePixelRatio || 1;
  const maxW = innerWidth * 0.8, maxH = innerHeight * 0.72;
  const scale = Math.min(maxW / S.width, maxH / S.height, 2);
  const cw = Math.round(S.width * scale), ch = Math.round(S.height * scale);
  modal(`${docName}.swf`, `
    <div class="player"><canvas id="tm-c" width="${Math.round(cw * dpr)}" height="${Math.round(ch * dpr)}" style="width:${cw}px;height:${ch}px"></canvas></div>
    <div class="player-bar">
      <button class="ib" id="tm-rew" title="Rebobinar"><svg viewBox="0 0 20 20"><path d="M5 5v10M15 5l-7 5 7 5z" class="f"/></svg></button>
      <button class="ib on" id="tm-play" title="Reproducir / pausa"></button>
      <button class="ib" id="tm-step" title="Avanzar"><svg viewBox="0 0 20 20"><path d="M7 5l7 5-7 5z" class="f"/></svg></button>
      <label class="muted" style="display:flex;gap:5px;align-items:center;margin-left:8px"><input type="checkbox" id="tm-loop" checked> Bucle</label>
      <div class="grow"></div>
      <span class="muted" id="tm-info"></span>
    </div>`, (b) => {
    const c = $('#tm-c', b);
    const g = c.getContext('2d');
    const total = ed.movie_total();
    let f = 0, run = true, last = performance.now(), acc = 0, raf = 0;
    const paint = () => {
      ed.render_preview(f, scale * dpr);
      const w = ed.preview_width(), h = ed.preview_height();
      const img = new ImageData(new Uint8ClampedArray(wasm.memory.buffer, ed.preview_ptr(), w * h * 4), w, h);
      if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
      g.putImageData(img, 0, 0);
      $('#tm-info').textContent = `Fotograma ${f + 1} / ${total} · ${S.fps} fps`;
    };
    const setBtn = () => ($('#tm-play').innerHTML = run
      ? '<svg viewBox="0 0 20 20"><rect x="5" y="4" width="3.5" height="12"/><rect x="11.5" y="4" width="3.5" height="12"/></svg>'
      : '<svg viewBox="0 0 20 20"><path d="M6 4l10 6-10 6z" class="f"/></svg>');
    const tick = (now) => {
      acc += now - last;
      last = now;
      const step = 1000 / S.fps;
      if (run && acc >= step) {
        acc %= step;
        if (f + 1 >= total) {
          if ($('#tm-loop').checked) f = 0;
          else run = false, setBtn();
        } else f++;
        paint();
      }
      raf = requestAnimationFrame(tick);
    };
    paint();
    setBtn();
    raf = requestAnimationFrame(tick);
    // Buttons respond to the mouse, as in the player.
    const mouse = (e, down) => {
      const r = c.getBoundingClientRect();
      ed.set_preview_mouse((e.clientX - r.left) * dpr, (e.clientY - r.top) * dpr, down);
      paint();
    };
    let pressed = false;
    c.addEventListener('pointermove', (e) => mouse(e, pressed));
    c.addEventListener('pointerdown', (e) => { pressed = true; mouse(e, true); });
    c.addEventListener('pointerup', (e) => { pressed = false; mouse(e, false); });
    c.addEventListener('pointerleave', () => { ed.set_preview_mouse(-1, -1, false); paint(); });
    $('#tm-play').addEventListener('click', () => { run = !run; setBtn(); });
    $('#tm-rew').addEventListener('click', () => { f = 0; paint(); });
    $('#tm-step').addEventListener('click', () => { run = false; setBtn(); f = (f + 1) % total; paint(); });
    modalCleanup = () => { cancelAnimationFrame(raf); ed.set_preview_mouse(-1, -1, false); };
  });
}

// ───────────────────────── Files and autosave ─────────────────────────

function download(name, blob) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

$('#fileopen').addEventListener('change', async (e) => {
  const file = e.target.files[0];
  e.target.value = '';
  if (file) openDocFile(file);
});

async function openDocFile(file) {
  try {
    ed.load_json(await file.text());
    docName = file.name.replace(/\.(omf|json)$/i, '');
    refresh();
    toast(`Abierto: ${file.name}`);
  } catch (err) {
    toast(`No se pudo abrir el archivo: ${err}`);
  }
}

// ───────────────────────── Installable app (PWA) ─────────────────────────
// Served over http(s) (a host or `node serve.js dist`), Edge and Chrome can
// install it like Photopea: its own window, works offline and opens
// .omf files on double-click. Double-clicking OMF.html (file://) does nothing.
if (/^https?:$/.test(location.protocol)) {
  const link = document.createElement('link');
  link.rel = 'manifest';
  link.href = 'manifest.webmanifest';
  document.head.appendChild(link);
  if ('serviceWorker' in navigator) navigator.serviceWorker.register('sw.js').catch(() => {});
  // .omf files opened from the system with the installed app.
  if ('launchQueue' in window) {
    window.launchQueue.setConsumer(async (params) => {
      const h = params.files && params.files[0];
      if (h) openDocFile(await h.getFile());
    });
  }
}

function autosave() {
  try {
    localStorage.setItem('omf.autosave', JSON.stringify({ name: docName, doc: ed.to_json() }));
  } catch {}
}

function restore() {
  try {
    const raw = localStorage.getItem('omf.autosave');
    if (!raw) return false;
    const { name, doc } = JSON.parse(raw);
    ed.load_json(doc);
    docName = name || docName;
    return true;
  } catch {
    return false;
  }
}

// ───────────────────────── Utilities ─────────────────────────

let toastTimer = 0;
function toast(msg) {
  const t = $('#toast');
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.hidden = true), 2600);
}

function clamp(v, a, b) {
  return Math.min(b, Math.max(a, isNaN(v) ? a : v));
}

function normHex(v) {
  v = v.trim().replace(/^#?/, '#');
  if (/^#[0-9a-f]{3}$/i.test(v)) v = '#' + [...v.slice(1)].map((c) => c + c).join('');
  return /^#[0-9a-f]{6}$/i.test(v) ? v.toLowerCase() : null;
}

function loadPref(key, def) {
  try {
    return { ...def, ...JSON.parse(localStorage.getItem(key) || '{}') };
  } catch {
    return def;
  }
}
function savePref(key, v) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {}
}

// Resizable Timeline splitter
const prefs = loadPref('omf.prefs', { tlHeight: 170, classic: false });
(() => {
  const tl = $('#timeline');
  tl.style.height = prefs.tlHeight + 'px';
  let start = null;
  const sp = $('#tl-splitter');
  sp.addEventListener('pointerdown', (e) => {
    start = { y: e.clientY, h: tl.offsetHeight };
    sp.setPointerCapture(e.pointerId);
  });
  sp.addEventListener('pointermove', (e) => {
    if (!start) return;
    const h = clamp(start.h + e.clientY - start.y, 80, innerHeight * 0.6);
    tl.style.height = h + 'px';
  });
  sp.addEventListener('pointerup', () => {
    if (!start) return;
    start = null;
    prefs.tlHeight = tl.offsetHeight;
    savePref('omf.prefs', prefs);
  });
})();

// ───────────────────────── Startup ─────────────────────────

const app = { ed, requestRender, refresh, cmd, contextMenu, stop, icons: ICONS, loadPref, savePref, layerProps, modal, closeModal, toast, paintChip, openColorPopover, setVal, download, docName: () => docName, state: () => S };
sym = initSymbols(app);
txt = initText(app);
io = initIO(app);
hist = initHistory(app, recorder);
spell = initSpell(app);
rulers = initRulers(app, { viewport, canvas });
acts = initActions(app);
audio = initAudio(app);
app.audio = audio;
rulers.set(!!prefs.rulers);
window.omf = { ...app, run, state: () => S, history: hist };
const timeline = new Timeline({ canvas: $('#tl-grid'), layersEl: $('#tl-layers'), app });
window.omf.timeline = timeline;
timeline.setClassic(!!prefs.classic);

buildToolbar();
buildPanels();
if (restore()) toast('Se ha recuperado tu último trabajo');
refresh();
buildMenubar();
