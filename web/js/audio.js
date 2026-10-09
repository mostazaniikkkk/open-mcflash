// Sound in the editor: import (the browser decodes WAV, MP3, OGG…), preview
// in the Library, and hear frame sounds when playing the Timeline (Event,
// Start, Stop and Stream, as in Flash), with effects (per-channel envelope
// and in/out points).

/** Sample rate sounds are stored at (Flash's 22 kHz). */
export const RATE = 22050;

const FULL = 32768;
/** Envelope of a preset effect (same as in the core). */
export function envPreset(effect, len) {
  const p = (pos, l, r) => ({ pos, l, r });
  switch (effect) {
    case 'left': return [p(0, FULL, 0)];
    case 'right': return [p(0, 0, FULL)];
    case 'fadeLR': return [p(0, FULL, 0), p(len, 0, FULL)];
    case 'fadeRL': return [p(0, 0, FULL), p(len, FULL, 0)];
    case 'fadeIn': return [p(0, 0, 0), p(Math.floor(len / 4), FULL, FULL)];
    case 'fadeOut': return [p(len - Math.floor(len / 4), FULL, FULL), p(len, 0, 0)];
    default: return [];
  }
}

/** Volume of each channel (0…1) at sample `pos` (at 44 kHz). */
export function envGain(env, pos) {
  if (!env || !env.length) return [1, 1];
  const k = (v) => v / FULL;
  if (pos <= env[0].pos) return [k(env[0].l), k(env[0].r)];
  for (let i = 1; i < env.length; i++) {
    const a = env[i - 1], b = env[i];
    if (pos <= b.pos) {
      const t = b.pos > a.pos ? (pos - a.pos) / (b.pos - a.pos) : 1;
      return [k(a.l) + (k(b.l) - k(a.l)) * t, k(a.r) + (k(b.r) - k(a.r)) * t];
    }
  }
  const z = env[env.length - 1];
  return [k(z.l), k(z.r)];
}

export const EFFECTS = [
  ['none', 'Ninguno'], ['left', 'Canal izquierdo'], ['right', 'Canal derecho'], ['fadeLR', 'Desvanecimiento de izquierda a derecha'],
  ['fadeRL', 'Desvanecimiento de derecha a izquierda'], ['fadeIn', 'Aumento progresivo'], ['fadeOut', 'Desvanecimiento'], ['custom', 'Personalizado'],
];

export function initAudio(app) {
  const { ed } = app;
  let ac = null;
  const buffers = new Map();
  const playing = new Map(); // id → [{src, stream}]

  const ctx = () => (ac ||= new (window.AudioContext || window.webkitAudioContext)());

  /** Audio file → 16-bit PCM at 22,050 Hz (mono or stereo). */
  async function decode(file) {
    const data = await file.arrayBuffer();
    const buf = await ctx().decodeAudioData(data);
    const ch = Math.min(2, buf.numberOfChannels);
    const len = Math.max(1, Math.ceil(buf.duration * RATE));
    const off = new OfflineAudioContext(ch, len, RATE);
    const src = off.createBufferSource();
    src.buffer = buf;
    src.connect(off.destination);
    src.start();
    const out = await off.startRendering();
    const pcm = new Int16Array(out.length * ch);
    const chans = [...Array(ch)].map((_, c) => out.getChannelData(c));
    for (let i = 0; i < out.length; i++) {
      for (let c = 0; c < ch; c++) pcm[i * ch + c] = Math.max(-32768, Math.min(32767, Math.round(chans[c][i] * 32767)));
    }
    return { pcm: new Uint8Array(pcm.buffer), stereo: ch === 2 };
  }

  async function importFile(file) {
    const { pcm, stereo } = await decode(file);
    return ed.import_sound(file.name, pcm, RATE, stereo);
  }

  function buffer(id, bits = null) {
    const key = bits == null ? id : `${id}:${bits}`;
    if (!buffers.has(key)) {
      const info = JSON.parse(ed.sound_info(id));
      if (!info) return null;
      // As it will sound in the movie (with ADPCM compression if it has it).
      const bytes = bits == null ? ed.sound_preview_pcm(id) : ed.sound_test_pcm(id, bits);
      const ch = info.stereo ? 2 : 1;
      const pcm = new Int16Array(bytes.buffer, bytes.byteOffset, bytes.byteLength >> 1);
      const b = ctx().createBuffer(ch, info.samples, info.rate);
      for (let c = 0; c < ch; c++) {
        const d = b.getChannelData(c);
        for (let i = 0; i < info.samples; i++) d[i] = pcm[i * ch + c] / 32768;
      }
      buffers.set(key, b);
    }
    return buffers.get(key);
  }

  /**
   * Plays a sound (from `offset` seconds, `loops` times), with its envelope
   * and its in/out points (in samples at 44 kHz).
   */
  function play(id, { offset = 0, loops = 1, stream = false, env = null, inp = 0, out = 0, bits = null } = {}) {
    const b = buffer(id, bits);
    if (!b) return;
    const c = ctx();
    if (c.state === 'suspended') c.resume();
    const src = c.createBufferSource();
    src.buffer = b;
    const a = Math.min(b.duration, inp / 44100);
    const z = out > 0 ? Math.max(a + 0.001, Math.min(b.duration, out / 44100)) : b.duration;
    const len = z - a;
    if (loops > 1 || a > 0 || z < b.duration) {
      src.loop = loops > 1;
      src.loopStart = a;
      src.loopEnd = z;
      setTimeout(() => { try { src.stop(); } catch {} }, Math.max(0, (len * loops - offset) * 1000));
    }
    if (env && env.length) {
      // One volume per channel: both sides go through their gain.
      const gl = c.createGain(), gr = c.createGain(), merge = c.createChannelMerger(2);
      if (b.numberOfChannels > 1) {
        const split = c.createChannelSplitter(2);
        src.connect(split);
        split.connect(gl, 0);
        split.connect(gr, 1);
      } else {
        src.connect(gl);
        src.connect(gr);
      }
      gl.connect(merge, 0, 0);
      gr.connect(merge, 0, 1);
      merge.connect(c.destination);
      const t0 = c.currentTime;
      const at = (s) => t0 + s;
      for (let k = 0; k < Math.min(loops, 50); k++) {
        // Time of each point in this loop (from the in point).
        const base = k * len - (offset % len);
        const g0 = envGain(env, Math.round((a + Math.max(0, -base)) * 44100));
        if (k === 0) { gl.gain.setValueAtTime(g0[0], t0); gr.gain.setValueAtTime(g0[1], t0); }
        else { gl.gain.setValueAtTime(envGain(env, Math.round(a * 44100))[0], at(base)); gr.gain.setValueAtTime(envGain(env, Math.round(a * 44100))[1], at(base)); }
        for (const p of env) {
          const s = p.pos / 44100;
          if (s < a || s > z) continue;
          const t = base + (s - a);
          if (t <= 0) continue;
          gl.gain.linearRampToValueAtTime(p.l / FULL, at(t));
          gr.gain.linearRampToValueAtTime(p.r / FULL, at(t));
        }
        const ge = envGain(env, Math.round(z * 44100));
        if (base + len > 0) { gl.gain.linearRampToValueAtTime(ge[0], at(base + len - 0.001)); gr.gain.linearRampToValueAtTime(ge[1], at(base + len - 0.001)); }
      }
    } else {
      src.connect(c.destination);
    }
    src.start(0, a + (offset % len));
    const list = playing.get(id) || [];
    list.push({ src, stream });
    playing.set(id, list);
    src.onended = () => {
      const l = playing.get(id) || [];
      playing.set(id, l.filter((x) => x.src !== src));
    };
  }

  function stop(id) {
    for (const p of playing.get(id) || []) try { p.src.stop(); } catch {}
    playing.delete(id);
  }

  function stopAll() {
    for (const id of [...playing.keys()]) stop(id);
  }

  const isPlaying = (id, stream) => (playing.get(id) || []).some((p) => !stream || p.stream);

  /**
   * When editor playback reaches a frame: event and start sounds, stop, and
   * stream sounds start at their position if not already playing.
   */
  function onFrame(frame, fps) {
    let list;
    try { list = JSON.parse(ed.frame_sounds(frame)); } catch { return; }
    for (const s of list) {
      if (s.sync === 'stop') stop(s.id);
      else if (s.sync === 'stream') {
        if (!isPlaying(s.id, true)) play(s.id, { offset: (frame - s.key) / fps, loops: s.loops, stream: true, env: s.env, inp: s.in || 0, out: s.out || 0 });
      } else if (s.key === frame) {
        if (s.sync === 'start' && isPlaying(s.id)) continue;
        play(s.id, { loops: s.loops, env: s.env, inp: s.in || 0, out: s.out || 0 });
      }
    }
  }

  /** When playback stops, streams are cut (events keep playing, as in Flash). */
  function onStop() {
    for (const [id, l] of playing) if (l.some((p) => p.stream)) stop(id);
  }

  function forget(id) {
    for (const k of [...buffers.keys()]) if (k === id || String(k).startsWith(id + ':')) buffers.delete(k);
  }

  return { decode, importFile, play, stop, stopAll, onFrame, onStop, forget, isPlaying };
}
