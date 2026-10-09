//! Sound: files imported into the Library and frame sounds with
//! Flash sync (Event, Start, Stop, Stream) and loops.
//!
//! The UI decodes audio with the browser (WAV, MP3, OGG…) and
//! delivers it as 16-bit PCM; it is exported to SWF uncompressed ("Raw")
//! or with 2- to 5-bit ADPCM compression, like Flash 8. Each frame sound
//! can carry an effect (left channel, fades…), which is the
//! SOUNDINFO envelope, with in and out points.

use super::*;
use crate::symbols::{Symbol, SymbolKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundAsset {
    /// Same as that of the Library item.
    pub id: u32,
    pub rate: u32,
    pub stereo: bool,
    /// Samples per channel.
    pub samples: u32,
    /// 16-bit PCM, little endian, interleaved channels (base64).
    pub pcm: String,
    /// Compression for SWF: 0 = raw; 2…5 = ADPCM with that many bits.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub adpcm: u8,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

impl SoundAsset {
    pub fn seconds(&self) -> f32 {
        self.samples as f32 / self.rate.max(1) as f32
    }
    /// SWF rate code (5.5 / 11 / 22 / 44 kHz).
    pub fn rate_code(&self) -> u8 {
        match self.rate {
            r if r <= 5512 => 0,
            r if r <= 11025 => 1,
            r if r <= 22050 => 2,
            _ => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sync {
    #[default]
    Event,
    Start,
    Stop,
    Stream,
}

fn one() -> u32 {
    1
}

/// Envelope point: position in samples at 44 kHz and volume of each
/// channel (0…32768), as in SOUNDINFO.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvPoint {
    pub pos: u32,
    pub l: u16,
    pub r: u16,
}

/// Sound on a keyframe.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameSound {
    pub id: u32,
    #[serde(default)]
    pub sync: Sync,
    #[serde(default = "one")]
    pub loops: u32,
    /// Effect (Properties): none, left, right, fadeLR, fadeRL, fadeIn, fadeOut, custom.
    #[serde(default = "no_effect", skip_serializing_if = "is_none_effect")]
    pub effect: String,
    /// Envelope (at most 8 points, same as in Flash).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvPoint>,
    /// In and out points (samples at 44 kHz; 0 = no trimming).
    #[serde(default, skip_serializing_if = "is_zero32", rename = "in")]
    pub inp: u32,
    #[serde(default, skip_serializing_if = "is_zero32")]
    pub out: u32,
}

fn no_effect() -> String {
    "none".into()
}
fn is_none_effect(s: &String) -> bool {
    s == "none" || s.is_empty()
}
fn is_zero32(v: &u32) -> bool {
    *v == 0
}

pub const ENV_MAX: usize = 8;
const FULL: u16 = 32768;

impl FrameSound {
    pub fn plain(id: u32) -> FrameSound {
        FrameSound { id, sync: Sync::Event, loops: 1, effect: no_effect(), env: Vec::new(), inp: 0, out: 0 }
    }

    /// Envelope of a preset effect for a sound of `len44` samples (at 44 kHz).
    pub fn preset(effect: &str, len44: u32) -> Vec<EnvPoint> {
        let p = |pos: u32, l: u16, r: u16| EnvPoint { pos, l, r };
        match effect {
            "left" => vec![p(0, FULL, 0)],
            "right" => vec![p(0, 0, FULL)],
            "fadeLR" => vec![p(0, FULL, 0), p(len44, 0, FULL)],
            "fadeRL" => vec![p(0, 0, FULL), p(len44, FULL, 0)],
            "fadeIn" => vec![p(0, 0, 0), p(len44 / 4, FULL, FULL)],
            "fadeOut" => vec![p(len44 - len44 / 4, FULL, FULL), p(len44, 0, 0)],
            _ => Vec::new(),
        }
    }

    /// Volume (left, right) from 0 to 1 at sample `pos44` (at 44 kHz).
    pub fn gain_at(&self, pos44: u32) -> (f32, f32) {
        let e = &self.env;
        if e.is_empty() {
            return (1.0, 1.0);
        }
        let k = |v: u16| v as f32 / FULL as f32;
        if pos44 <= e[0].pos {
            return (k(e[0].l), k(e[0].r));
        }
        for w in e.windows(2) {
            if pos44 <= w[1].pos {
                let t = if w[1].pos > w[0].pos { (pos44 - w[0].pos) as f32 / (w[1].pos - w[0].pos) as f32 } else { 1.0 };
                return (k(w[0].l) + (k(w[1].l) - k(w[0].l)) * t, k(w[0].r) + (k(w[1].r) - k(w[0].r)) * t);
            }
        }
        let l = e[e.len() - 1];
        (k(l.l), k(l.r))
    }

    pub fn has_envelope(&self) -> bool {
        !self.env.is_empty()
    }
}

// ───────────────────────── ADPCM ─────────────────────────

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658,
    724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350,
    22385, 24623, 27086, 29794, 32767,
];

fn index_table(bits: u8) -> &'static [i32] {
    match bits {
        2 => &[-1, 2],
        3 => &[-1, -1, 2, 4],
        4 => &[-1, -1, -1, -1, 2, 4, 6, 8],
        _ => &[-1, -1, -1, -1, -1, -1, -1, -1, 1, 2, 4, 6, 8, 10, 13, 16],
    }
}

struct BitW {
    buf: Vec<u8>,
    acc: u32,
    n: u32,
}

impl BitW {
    fn put(&mut self, bits: u32, v: u32) {
        for i in (0..bits).rev() {
            self.acc = self.acc << 1 | (v >> i & 1);
            self.n += 1;
            if self.n == 8 {
                self.buf.push(self.acc as u8);
                self.acc = 0;
                self.n = 0;
            }
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.buf.push((self.acc << (8 - self.n)) as u8);
        }
        self.buf
    }
}

/// One step of the Flash decoder (the encoder mimics it so as not to drift).
fn adpcm_step(sample: &mut i32, index: &mut i32, code: u32, bits: u8) {
    let sign = 1u32 << (bits - 1);
    let mag = (code & (sign - 1)) as i32;
    let step = STEPS[*index as usize];
    let delta = ((2 * mag + 1) * step) >> (bits - 1);
    *sample = if code & sign != 0 { *sample - delta } else { *sample + delta }.clamp(-32768, 32767);
    *index = (*index + index_table(bits)[mag as usize]).clamp(0, 88);
}

/// Encodes 16-bit PCM (interleaved channels) into SWF ADPCM.
pub fn adpcm_encode(pcm: &[i16], stereo: bool, bits: u8) -> Vec<u8> {
    let bits = bits.clamp(2, 5);
    let ch = if stereo { 2 } else { 1 };
    let frames = pcm.len() / ch;
    let mut w = BitW { buf: Vec::new(), acc: 0, n: 0 };
    w.put(2, (bits - 2) as u32);
    let mut state = [(0i32, 0i32); 2];
    let sign = 1u32 << (bits - 1);
    for f in 0..frames {
        if f % 4096 == 0 {
            for c in 0..ch {
                let s = pcm[f * ch + c] as i32;
                // Initial index: the one that best follows the first difference.
                let next = pcm.get((f + 1) * ch + c).map_or(s, |v| *v as i32);
                let d = (next - s).abs();
                let idx = STEPS.iter().position(|st| *st * 2 >= d).unwrap_or(88) as i32;
                state[c] = (s, idx.clamp(0, 88));
                w.put(16, s as u16 as u32);
                w.put(6, state[c].1 as u32);
            }
            continue;
        }
        for c in 0..ch {
            let target = pcm[f * ch + c] as i32;
            let (s, idx) = state[c];
            let step = STEPS[idx as usize];
            let d = target - s;
            let mut best = (i32::MAX, 0u32);
            for mag in 0..sign {
                let delta = ((2 * mag as i32 + 1) * step) >> (bits - 1);
                let code = if d < 0 { mag | sign } else { mag };
                let v = if d < 0 { s - delta } else { s + delta }.clamp(-32768, 32767);
                let err = (v - target).abs();
                if err < best.0 {
                    best = (err, code);
                }
            }
            let (mut s2, mut i2) = (s, idx);
            adpcm_step(&mut s2, &mut i2, best.1, bits);
            state[c] = (s2, i2);
            w.put(bits as u32, best.1);
        }
    }
    w.finish()
}

/// Decodes SWF ADPCM (for tests and for playback in the editor).
pub fn adpcm_decode(data: &[u8], stereo: bool, frames: usize) -> Vec<i16> {
    let ch = if stereo { 2 } else { 1 };
    let mut pos = 0usize;
    let mut get = |n: u32| -> u32 {
        let mut v = 0;
        for _ in 0..n {
            let b = data.get(pos / 8).map_or(0, |b| b >> (7 - pos % 8) & 1);
            v = v << 1 | b as u32;
            pos += 1;
        }
        v
    };
    let bits = get(2) as u8 + 2;
    let mut out = Vec::with_capacity(frames * ch);
    let mut state = [(0i32, 0i32); 2];
    for f in 0..frames {
        for c in 0..ch {
            if f % 4096 == 0 {
                state[c] = (get(16) as u16 as i16 as i32, get(6) as i32);
            } else {
                let code = get(bits as u32);
                let (mut s, mut i) = state[c];
                adpcm_step(&mut s, &mut i, code, bits);
                state[c] = (s, i);
            }
        }
        // In the first sample of each packet the channels are read together.
        for c in 0..ch {
            out.push(state[c].0 as i16);
        }
    }
    out
}

pub fn pcm_samples(b64: &str) -> Vec<i16> {
    crate::bitmap::b64_decode(b64).chunks(2).map(|c| i16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect()
}

impl Editor {
    pub(crate) fn sound_asset(&self, id: u32) -> Option<&SoundAsset> {
        self.doc.sounds.iter().find(|s| s.id == id)
    }
}

#[wasm_bindgen]
impl Editor {
    /// File > Import a sound: already-decoded 16-bit PCM.
    pub fn import_sound(&mut self, name: &str, pcm: Vec<u8>, rate: u32, stereo: bool) -> u32 {
        let ch = if stereo { 2 } else { 1 };
        let samples = (pcm.len() / 2 / ch) as u32;
        if samples == 0 {
            self.message = format!("«{name}» no tiene audio");
            return 0;
        }
        let base = self.doc.clone();
        let id = self.doc.alloc_id();
        self.doc.sounds.push(SoundAsset { id, rate, stereo, samples, pcm: crate::bitmap::b64_encode(&pcm), adpcm: 0 });
        let mut n = name.trim().to_string();
        if n.is_empty() || self.doc.library.iter().any(|s| s.name == n) {
            let mut k = 2;
            while self.doc.library.iter().any(|s| s.name == format!("{n} {k}")) {
                k += 1;
            }
            n = format!("{n} {k}");
        }
        self.doc.library.push(Symbol { id, name: n, kind: SymbolKind::Sound, layers: Vec::new(), folder: None, scale9: None, linkage: None });
        self.commit_from(base);
        self.sync_lib();
        id
    }

    /// PCM of a sound (for playback in the editor).
    pub fn sound_pcm(&self, id: u32) -> Vec<u8> {
        self.sound_asset(id).map(|s| crate::bitmap::b64_decode(&s.pcm)).unwrap_or_default()
    }

    /// {rate, stereo, samples, seconds, adpcm, bytes (size in the SWF), raw}
    pub fn sound_info(&self, id: u32) -> String {
        match self.sound_asset(id) {
            Some(s) => {
                let raw = s.samples as usize * 2 * if s.stereo { 2 } else { 1 };
                let bytes = if s.adpcm == 0 { raw } else { raw / 16 * s.adpcm as usize + 1 };
                json!({ "rate": s.rate, "stereo": s.stereo, "samples": s.samples, "seconds": s.seconds(), "adpcm": s.adpcm, "bytes": bytes, "raw": raw }).to_string()
            }
            None => "null".into(),
        }
    }

    /// Sound Properties > Compression: 0 = raw; 2…5 = ADPCM with that many bits.
    pub fn set_sound_compression(&mut self, id: u32, adpcm: u8) {
        let v = if adpcm == 0 { 0 } else { adpcm.clamp(2, 5) };
        let Some(i) = self.doc.sounds.iter().position(|s| s.id == id) else { return };
        if self.doc.sounds[i].adpcm == v {
            return;
        }
        let base = self.doc.clone();
        self.doc.sounds[i].adpcm = v;
        self.commit_from(base);
    }

    /// PCM as it will sound in the movie (with compression applied), for "Test".
    pub fn sound_preview_pcm(&self, id: u32) -> Vec<u8> {
        let bits = self.sound_asset(id).map_or(0, |s| s.adpcm);
        self.sound_test_pcm(id, bits)
    }

    /// PCM with a trial compression (Sound Properties > Test).
    pub fn sound_test_pcm(&self, id: u32, adpcm: u8) -> Vec<u8> {
        let Some(s) = self.sound_asset(id) else { return Vec::new() };
        let s = SoundAsset { adpcm: if adpcm == 0 { 0 } else { adpcm.clamp(2, 5) }, ..s.clone() };
        if s.adpcm == 0 {
            return crate::bitmap::b64_decode(&s.pcm);
        }
        let enc = adpcm_encode(&pcm_samples(&s.pcm), s.stereo, s.adpcm);
        adpcm_decode(&enc, s.stereo, s.samples as usize).iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// Waveform peaks (0…1) in `n` segments, for drawing it on the timeline.
    pub fn sound_peaks(&self, id: u32, n: u32) -> Vec<f32> {
        let Some(s) = self.sound_asset(id) else { return Vec::new() };
        let pcm = crate::bitmap::b64_decode(&s.pcm);
        let ch = if s.stereo { 2 } else { 1 };
        let frames = pcm.len() / 2 / ch;
        let n = n.max(1) as usize;
        (0..n)
            .map(|k| {
                let (a, b) = (k * frames / n, ((k + 1) * frames / n).max(k * frames / n + 1).min(frames));
                let step = ((b - a) / 64).max(1);
                let mut m = 0i32;
                for f in (a..b).step_by(step) {
                    let i = f * ch * 2;
                    if i + 1 < pcm.len() {
                        m = m.max((i16::from_le_bytes([pcm[i], pcm[i + 1]]) as i32).abs());
                    }
                }
                m as f32 / 32768.0
            })
            .collect()
    }

    /// Sound of the current keyframe: JSON {"id", "sync", "loops"} or "null" to remove it.
    pub fn set_frame_sound(&mut self, json: &str) {
        let snd = if json.trim() == "null" {
            None
        } else {
            match serde_json::from_str::<FrameSound>(json) {
                Ok(s) if self.sound_asset(s.id).is_some() => {
                    let a = self.sound_asset(s.id).unwrap();
                    let len44 = (a.samples as u64 * 44100 / a.rate.max(1) as u64) as u32;
                    let mut s = FrameSound { loops: s.loops.clamp(1, 9999), ..s };
                    // A preset effect brings its own envelope; a custom one is sorted and capped.
                    if s.effect != "custom" {
                        s.env = FrameSound::preset(&s.effect, len44);
                    }
                    if s.env.is_empty() && s.effect == "custom" {
                        s.effect = "none".into();
                    }
                    s.env.sort_by_key(|p| p.pos);
                    s.env.truncate(ENV_MAX);
                    for p in &mut s.env {
                        p.l = p.l.min(FULL);
                        p.r = p.r.min(FULL);
                    }
                    s.out = s.out.min(len44);
                    if s.out > 0 && s.inp >= s.out {
                        s.inp = 0;
                    }
                    Some(s)
                }
                Ok(_) => {
                    self.message = "Ese sonido no está en la biblioteca".into();
                    return;
                }
                Err(e) => {
                    self.message = format!("Sonido no válido: {e}");
                    return;
                }
            }
        };
        let l = self.layer;
        let Some(k) = self.doc.layers[l].key_at_or_before(self.frame).filter(|_| self.frame < self.doc.layers[l].length) else {
            self.message = "Elige un fotograma clave para ponerle sonido".into();
            return;
        };
        let base = self.doc.clone();
        self.doc.layers[l].keys.get_mut(&k).unwrap().sound = snd;
        self.commit_from(base);
    }

    /// Sounds starting at `frame` (current timeline), to
    /// play them in the editor: [{id, sync, loops, key, span}].
    pub fn frame_sounds(&self, frame: u32) -> String {
        let mut v = Vec::new();
        for l in &self.doc.layers {
            if matches!(l.kind, LayerKind::Folder | LayerKind::Guide | LayerKind::MotionGuide) || frame >= l.length {
                continue;
            }
            let Some(key) = l.key_at_or_before(frame) else { continue };
            let Some(s) = &l.keys[&key].sound else { continue };
            // Event, start and stop play on reaching the keyframe; stream, over its whole span.
            if key == frame || s.sync == Sync::Stream {
                v.push(json!({ "id": s.id, "sync": s.sync, "loops": s.loops, "key": key, "span": l.span_end(key) - key + 1, "env": s.env, "in": s.inp, "out": s.out }));
            }
        }
        Value::Array(v).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize) -> Vec<u8> {
        (0..n).flat_map(|i| (((i as f32 * 0.1).sin() * 12000.0) as i16).to_le_bytes()).collect()
    }

    #[test]
    fn adpcm_roundtrip_is_close() {
        let pcm: Vec<i16> = (0..10000).map(|i| ((i as f32 * 0.05).sin() * 12000.0) as i16).collect();
        for bits in 2..=5u8 {
            let enc = adpcm_encode(&pcm, false, bits);
            assert!(enc.len() < pcm.len() * 2 * bits as usize / 16 + 64);
            let dec = adpcm_decode(&enc, false, pcm.len());
            let err: f64 = pcm.iter().zip(&dec).map(|(a, b)| ((*a as f64) - (*b as f64)).powi(2)).sum::<f64>() / pcm.len() as f64;
            let rms = err.sqrt();
            assert!(rms < [0.0, 0.0, 4000.0, 1500.0, 600.0, 300.0][bits as usize], "{bits} bits: error {rms}");
        }
        // Stereo: channels are not mixed.
        let st: Vec<i16> = (0..9000).flat_map(|i| [((i as f32 * 0.05).sin() * 9000.0) as i16, 0]).collect();
        let dec = adpcm_decode(&adpcm_encode(&st, true, 4), true, 9000);
        assert!(dec.iter().skip(1).step_by(2).all(|v| v.abs() < 200));
    }

    #[test]
    fn envelope_presets() {
        let f = FrameSound { env: FrameSound::preset("fadeIn", 44100), ..FrameSound::plain(1) };
        assert_eq!(f.gain_at(0), (0.0, 0.0));
        assert!((f.gain_at(5512).0 - 0.5).abs() < 0.01);
        assert_eq!(f.gain_at(40000), (1.0, 1.0));
        let f = FrameSound { env: FrameSound::preset("fadeLR", 44100), ..FrameSound::plain(1) };
        assert_eq!(f.gain_at(0), (1.0, 0.0));
        assert_eq!(f.gain_at(44100), (0.0, 1.0));
    }

    #[test]
    fn import_place_and_export_sounds() {
        let mut e = Editor::new();
        let id = e.import_sound("pitido.wav", tone(22050), 22050, false);
        assert!(id > 0);
        let info: Value = serde_json::from_str(&e.sound_info(id)).unwrap();
        assert!((info["seconds"].as_f64().unwrap() - 1.0).abs() < 1e-3);
        let peaks = e.sound_peaks(id, 10);
        assert_eq!(peaks.len(), 10);
        assert!(peaks.iter().all(|p| *p > 0.3));
        e.set_frame_sound(&format!(r#"{{"id":{id},"sync":"event","loops":2}}"#));
        assert_eq!(e.doc.layers[0].keys[&0].sound.as_ref().unwrap().loops, 2);
        let v: Vec<Value> = serde_json::from_str(&e.frame_sounds(0)).unwrap();
        assert_eq!(v[0]["id"], id);
        let swf = crate::swf::uncompressed(&e.export_swf());
        let codes = crate::swf::tests_support::tag_codes(&swf);
        assert!(codes.contains(&14) && codes.contains(&15), "DefineSound y StartSound");
        // Stream: header and one block per frame.
        e.select_cell(0, 23);
        e.insert_frame();
        e.select_cell(0, 0);
        e.set_frame_sound(&format!(r#"{{"id":{id},"sync":"stream"}}"#));
        let swf = crate::swf::uncompressed(&e.export_swf());
        let codes = crate::swf::tests_support::tag_codes(&swf);
        assert!(codes.contains(&45), "SoundStreamHead2");
        assert_eq!(codes.iter().filter(|c| **c == 19).count(), 12, "1 s de audio a 12 fps: 12 bloques");
        // ADPCM: smaller and close to the original.
        let raw_len = swf.len();
        e.set_sound_compression(id, 4);
        let swf = crate::swf::uncompressed(&e.export_swf());
        assert!(swf.len() < raw_len - 20000, "{} vs {}", swf.len(), raw_len);
        // Effect: left channel (envelope in SOUNDINFO) for an event sound.
        e.set_frame_sound(&format!(r#"{{"id":{id},"sync":"event","effect":"left","in":4410}}"#));
        let fs = e.doc.layers[0].keys[&0].sound.clone().unwrap();
        assert_eq!(fs.env.len(), 1);
        assert_eq!(fs.gain_at(100), (1.0, 0.0));
        let swf = crate::swf::uncompressed(&e.export_swf());
        let codes = crate::swf::tests_support::tag_codes(&swf);
        assert!(codes.contains(&15));
        // Deleting the sound from the library removes it from the frames.
        e.lib_delete(id);
        assert!(e.doc.sounds.is_empty() && e.doc.layers[0].keys[&0].sound.is_none());
    }
}
