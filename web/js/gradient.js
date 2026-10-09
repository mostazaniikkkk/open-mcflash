// Gradients in the UI: the Color Mixer's gradient bar editor (like Flash
// 8's), gradient swatches and a CSS preview for swatches.

const hex2 = (n) => n.toString(16).padStart(2, '0');
export const rgbaHex = (c) => '#' + hex2(c.r) + hex2(c.g) + hex2(c.b);
export const hexRgba = (hex, a = 255) => {
  const n = parseInt(hex.slice(1), 16);
  return { r: n >> 16, g: (n >> 8) & 255, b: n & 255, a };
};
const css = (c) => `rgba(${c.r},${c.g},${c.b},${(c.a / 255).toFixed(3)})`;

/** CSS background that mimics a gradient (for swatches and color wells). */
export function gradCss(g) {
  const stops = g.stops.map((s) => `${css(s.c)} ${(s.t * 100).toFixed(1)}%`).join(', ');
  return g.kind === 'radial' ? `radial-gradient(circle, ${stops})` : `linear-gradient(90deg, ${stops})`;
}

const st = (t, hex) => ({ t, c: hexRgba(hex) });
/** Gradients from Flash 8's default palette. */
export const DEFAULT_GRADIENTS = [
  { kind: 'linear', stops: [st(0, '#ffffff'), st(1, '#000000')] },
  { kind: 'radial', stops: [st(0, '#ffffff'), st(1, '#000000')] },
  { kind: 'radial', stops: [st(0, '#ff0000'), st(1, '#000000')] },
  { kind: 'radial', stops: [st(0, '#00ff00'), st(1, '#000000')] },
  { kind: 'radial', stops: [st(0, '#0000ff'), st(1, '#000000')] },
  { kind: 'linear', stops: [st(0, '#0066fd'), st(0.33, '#ffffff'), st(0.66, '#ffffff'), st(1, '#ffcc00')] },
  { kind: 'linear', stops: [st(0, '#ff0000'), st(0.17, '#ffff00'), st(0.33, '#00ff00'), st(0.5, '#00ffff'), st(0.67, '#0000ff'), st(0.83, '#ff00ff'), st(1, '#ff0000')] },
];

const MAX_STOPS = 15;

/**
 * Gradient bar: clicking the bar adds a color, pointers can be dragged,
 * dragging them down off the bar removes them, and the selected one is edited
 * with the color picker.
 */
export class GradientBar {
  constructor(host, { onChange, onPick }) {
    this.host = host;
    this.onChange = onChange;
    this.onPick = onPick;
    this.g = null;
    this.sel = 0;
    host.classList.add('gbar');
    host.innerHTML = '<div class="gbar-strip checker"><i></i></div><div class="gbar-marks"></div>';
    this.strip = host.querySelector('.gbar-strip');
    this.marks = host.querySelector('.gbar-marks');
    this.strip.addEventListener('pointerdown', (e) => this.addAt(e));
    this.marks.addEventListener('pointerdown', (e) => this.grab(e));
  }

  set(g) {
    this.g = JSON.parse(JSON.stringify(g));
    this.sel = Math.min(this.sel, this.g.stops.length - 1);
    this.draw();
  }

  get selected() {
    return this.g && this.g.stops[this.sel];
  }

  draw() {
    if (!this.g) return;
    this.strip.querySelector('i').style.background = gradCss({ ...this.g, kind: 'linear' });
    this.marks.innerHTML = this.g.stops.map((s, i) =>
      `<b class="gmark${i === this.sel ? ' sel' : ''}" data-i="${i}" style="left:${(s.t * 100).toFixed(2)}%"><i style="background:${css(s.c)}"></i></b>`).join('');
  }

  ratio(e) {
    const r = this.strip.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }

  addAt(e) {
    if (!this.g || this.g.stops.length >= MAX_STOPS) return;
    const t = this.ratio(e);
    const c = colorAt(this.g, t);
    this.g.stops.push({ t, c });
    this.g.stops.sort((a, b) => a.t - b.t);
    this.sel = this.g.stops.findIndex((s) => s.t === t);
    this.draw();
    this.onChange(this.g);
  }

  grab(e) {
    const m = e.target.closest('.gmark');
    if (!m) return;
    e.preventDefault();
    this.sel = +m.dataset.i;
    this.draw();
    this.onPick && this.onPick(this.selected);
    const stop = this.g.stops[this.sel];
    const y0 = e.clientY;
    let removed = false;
    const move = (ev) => {
      stop.t = this.ratio(ev);
      // Down, off the bar: removed (if at least two remain).
      removed = ev.clientY - y0 > 30 && this.g.stops.length > 2;
      this.marks.querySelector(`[data-i="${this.sel}"]`)?.classList.toggle('gone', removed);
      const el = this.marks.querySelector(`[data-i="${this.sel}"]`);
      if (el) el.style.left = (stop.t * 100).toFixed(2) + '%';
      this.strip.querySelector('i').style.background = gradCss({ ...this.g, kind: 'linear', stops: [...this.g.stops].sort((a, b) => a.t - b.t) });
    };
    const up = () => {
      document.removeEventListener('pointermove', move);
      document.removeEventListener('pointerup', up);
      if (removed) {
        this.g.stops.splice(this.sel, 1);
        this.sel = 0;
      }
      this.g.stops.sort((a, b) => a.t - b.t);
      this.sel = removed ? 0 : this.g.stops.indexOf(stop);
      this.draw();
      this.onChange(this.g);
    };
    document.addEventListener('pointermove', move);
    document.addEventListener('pointerup', up);
  }

  /** Changes the color of the selected pointer. */
  setColor(c) {
    if (!this.selected) return;
    this.selected.c = c;
    this.draw();
    this.onChange(this.g);
  }
}

export function colorAt(g, t) {
  const s = g.stops;
  if (t <= s[0].t) return { ...s[0].c };
  for (let i = 1; i < s.length; i++) {
    if (t <= s[i].t) {
      const k = (t - s[i - 1].t) / Math.max(1e-6, s[i].t - s[i - 1].t);
      const l = (a, b) => Math.round(a + (b - a) * k);
      const a = s[i - 1].c, b = s[i].c;
      return { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) };
    }
  }
  return { ...s[s.length - 1].c };
}
