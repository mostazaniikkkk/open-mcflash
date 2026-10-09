// Rulers (View > Rulers): above and to the left of the Stage, in document
// pixels, with the pointer position. Dragging from a ruler creates a guide
// (horizontal from the top one, vertical from the left one).

export const RULER = 18;
const STEPS = [1, 2, 5, 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000, 5000, 10000];

export function initRulers(app, { viewport, canvas }) {
  const { ed } = app;
  const mk = (id, cls) => {
    const el = document.createElement(cls === 'ruler-corner' ? 'div' : 'canvas');
    el.id = id;
    el.className = cls;
    viewport.appendChild(el);
    return el;
  };
  const h = mk('ruler-h', 'ruler'), v = mk('ruler-v', 'ruler'), corner = mk('ruler-corner', 'ruler-corner');
  let on = false;
  let mouse = null; // pointer position in canvas CSS pixels

  function set(state) {
    on = state;
    viewport.classList.toggle('rulers', on);
    draw();
  }

  /** Doc → canvas CSS pixels on one axis. */
  function axis(horizontal) {
    const a = ed.to_screen(0, 0), b = ed.to_screen(1, 1);
    return horizontal ? { o: a[0], k: b[0] - a[0] } : { o: a[1], k: b[1] - a[1] };
  }

  function drawOne(c, horizontal) {
    const dpr = window.devicePixelRatio || 1;
    const r = canvas.getBoundingClientRect();
    const len = horizontal ? r.width : r.height;
    const W = horizontal ? len : RULER, H = horizontal ? RULER : len;
    if (c.width !== Math.round(W * dpr) || c.height !== Math.round(H * dpr)) {
      c.width = Math.round(W * dpr);
      c.height = Math.round(H * dpr);
    }
    const g = c.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const S = app.state();
    g.fillStyle = '#2b2b2b';
    if (!S) return g.fillRect(0, 0, W, H);
    g.fillRect(0, 0, W, H);
    const { o, k } = axis(horizontal);
    if (!(k > 0)) return;
    // The Stage, slightly lighter.
    const ext = horizontal ? S.width : S.height;
    g.fillStyle = '#353535';
    if (horizontal) g.fillRect(o, 0, ext * k, H);
    else g.fillRect(0, o, W, ext * k);
    const major = STEPS.find((s) => s * k >= 55) || STEPS[STEPS.length - 1];
    const sub = major * k / 10 >= 5 ? 10 : major * k / 5 >= 5 ? 5 : 2;
    const minor = major / sub;
    const first = Math.floor((0 - o) / k / minor) * minor;
    const last = (len - o) / k;
    g.fillStyle = '#6a6a6a';
    g.font = '9px Segoe UI, system-ui, sans-serif';
    g.textBaseline = 'top';
    for (let u = first; u <= last; u += minor) {
      const p = Math.round(o + u * k) + 0.5;
      const isMajor = Math.abs(u / major - Math.round(u / major)) < 1e-6;
      const t = isMajor ? RULER - 1 : sub === 10 && Math.abs(u / (major / 2) - Math.round(u / (major / 2))) < 1e-6 ? 7 : 4;
      if (horizontal) g.fillRect(p - 0.5, RULER - t, 1, t);
      else g.fillRect(RULER - t, p - 0.5, t, 1);
      if (isMajor) {
        const label = String(Math.round(u));
        g.fillStyle = '#a0a0a0';
        if (horizontal) g.fillText(label, p + 2, 1);
        else {
          g.save();
          g.translate(1, p + 2);
          g.rotate(Math.PI / 2);
          g.fillText(label, 0, -10);
          g.restore();
        }
        g.fillStyle = '#6a6a6a';
      }
    }
    g.fillStyle = '#121212';
    if (horizontal) g.fillRect(0, H - 1, W, 1);
    else g.fillRect(W - 1, 0, 1, H);
    // Pointer position.
    if (mouse) {
      g.fillStyle = '#2f8cff';
      if (horizontal) g.fillRect(Math.round(mouse[0]), 0, 1, H);
      else g.fillRect(0, Math.round(mouse[1]), W, 1);
    }
  }

  function draw() {
    if (!on) return;
    drawOne(h, true);
    drawOne(v, false);
  }

  // Dragging from a ruler: new guide (dropping it on the ruler discards it).
  const stageXY = (e) => {
    const r = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    return [(e.clientX - r.left) * dpr, (e.clientY - r.top) * dpr];
  };
  for (const [el, vertical] of [[h, false], [v, true]]) {
    el.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      ed.guide_begin(vertical);
      if (!ed.is_dragging()) return app.refresh();
      el.setPointerCapture(e.pointerId);
      const move = (ev) => {
        const [x, y] = stageXY(ev);
        ed.pointer_move(x, y, false, false);
        mouse = [x / (window.devicePixelRatio || 1), y / (window.devicePixelRatio || 1)];
        app.requestRender();
        draw();
      };
      const up = (ev) => {
        el.removeEventListener('pointermove', move);
        el.removeEventListener('pointerup', up);
        const [x, y] = stageXY(ev);
        ed.pointer_up(x, y, false, false);
        app.refresh();
      };
      el.addEventListener('pointermove', move);
      el.addEventListener('pointerup', up);
    });
  }
  corner.title = 'Reglas (Ctrl+Alt+Mayús+R)';

  return {
    set, draw,
    get on() { return on; },
    mark(x, y) { mouse = x == null ? null : [x, y]; draw(); },
  };
}
