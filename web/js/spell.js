// Text > Check Spelling and Spelling Setup.
// Spanish and English dictionaries (Hunspell, see web/dict/*.LICENSE)
// bundled with the app and checked with nspell (MIT): works offline.

import nspell from '../vendor/nspell.mjs';

const $ = (q, el = document) => el.querySelector(q);
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const LANGS = { es: 'Español', en: 'Inglés' };
const WORD = /[\p{L}\p{M}]+(?:['’][\p{L}\p{M}]+)*/gu;

export function initSpell(app) {
  const { ed } = app;
  const loaded = {};

  const prefs = () => app.loadPref('omf.spell', { texts: true, layers: false, labels: true, library: false, langs: ['es', 'en'], upper: true, digits: true, personal: [] });

  async function gunzipText(bytes) {
    const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
    return new Response(stream).text();
  }

  async function dictFile(lang, part) {
    const b64 = globalThis.OMF_DICTS?.[lang]?.[part];
    if (b64) return gunzipText(Uint8Array.from(atob(b64), (c) => c.charCodeAt(0)));
    const r = await fetch(`dict/${lang}.${part}.gz`);
    return gunzipText(new Uint8Array(await r.arrayBuffer()));
  }

  async function dict(lang) {
    if (!loaded[lang]) loaded[lang] = (async () => nspell({ aff: await dictFile(lang, 'aff'), dic: await dictFile(lang, 'dic') }))();
    return loaded[lang];
  }

  function settingsDialog() {
    const p = prefs();
    const chk = (id, label, on) => `<label class="chk"><input type="checkbox" id="${id}" ${on ? 'checked' : ''}> ${label}</label>`;
    app.modal('Configuración de revisión ortográfica', `
      <div class="spell-cols">
        <div><div class="ptitle">Opciones del documento</div><div style="display:grid;gap:6px">
          ${chk('sp-texts', 'Revisar el contenido de los campos de texto', p.texts)}${chk('sp-labels', 'Revisar las etiquetas de fotograma', p.labels)}
          ${chk('sp-layers', 'Revisar los nombres de capa', p.layers)}${chk('sp-library', 'Revisar los nombres de la biblioteca', p.library)}</div></div>
        <div><div class="ptitle">Diccionarios</div><div style="display:grid;gap:6px">
          ${Object.entries(LANGS).map(([k, l]) => chk(`sp-lang-${k}`, l, p.langs.includes(k))).join('')}</div>
          <div class="ptitle" style="margin-top:10px">Opciones de revisión</div><div style="display:grid;gap:6px">
          ${chk('sp-upper', 'Omitir palabras en MAYÚSCULAS', p.upper)}${chk('sp-digits', 'Omitir palabras con números', p.digits)}</div></div>
      </div>
      <div class="ptitle" style="margin-top:10px">Diccionario personal</div>
      <textarea id="sp-personal" rows="4" style="width:100%;background:var(--bg0);color:var(--text);border:1px solid #0f0f0f">${esc(p.personal.join('\n'))}</textarea>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="sp-ok">Aceptar</button></div>`, (b) => {
      $('#sp-ok', b).addEventListener('click', () => {
        app.savePref('omf.spell', {
          texts: $('#sp-texts', b).checked, labels: $('#sp-labels', b).checked, layers: $('#sp-layers', b).checked, library: $('#sp-library', b).checked,
          langs: Object.keys(LANGS).filter((k) => $(`#sp-lang-${k}`, b).checked),
          upper: $('#sp-upper', b).checked, digits: $('#sp-digits', b).checked,
          personal: $('#sp-personal', b).value.split(/\s+/).map((w) => w.trim()).filter(Boolean),
        });
        app.closeModal();
      });
    });
  }

  async function check() {
    const p = prefs();
    if (!p.langs.length) return app.toast('Elige al menos un diccionario en la configuración de revisión ortográfica');
    app.toast('Cargando diccionarios…');
    let dicts;
    try {
      dicts = await Promise.all(p.langs.map(dict));
    } catch {
      return app.toast('No se pudieron cargar los diccionarios');
    }
    const items = JSON.parse(ed.spell_items(p.texts, p.layers, p.labels, p.library));
    app.refresh();
    const personal = new Set(p.personal.map((w) => w.toLowerCase()));
    const ignored = new Set();
    const ok = (w) => {
      if (personal.has(w.toLowerCase()) || ignored.has(w)) return true;
      if (p.upper && w.length > 1 && w === w.toUpperCase() && w !== w.toLowerCase()) return true;
      return dicts.some((d) => d.correct(w));
    };
    let it = 0, pos = 0;
    const next = () => {
      for (; it < items.length; it++, pos = 0) {
        const text = items[it].text;
        WORD.lastIndex = pos;
        let m;
        while ((m = WORD.exec(text))) {
          const near = (text[m.index - 1] || '') + (text[m.index + m[0].length] || '');
          if (p.digits && /\d/.test(near)) continue;
          if (!ok(m[0])) return { item: items[it], word: m[0], at: m.index };
        }
      }
      return null;
    };
    const suggest = (w) => [...new Set(dicts.flatMap((d) => d.suggest(w)))].slice(0, 8);

    const show = () => {
      const hit = next();
      if (!hit) {
        app.closeModal();
        app.refresh();
        return app.toast('Revisión ortográfica completa');
      }
      const { item, word, at } = hit;
      const sug = suggest(word);
      const ctx = `${esc(item.text.slice(Math.max(0, at - 40), at))}<mark>${esc(word)}</mark>${esc(item.text.slice(at + word.length, at + word.length + 40))}`;
      app.modal('Revisar ortografía', `
        <div class="muted">${esc(item.where)}</div>
        <div class="spell-ctx">${ctx}</div>
        <div class="prow"><span class="lab" style="width:90px">No está</span><b id="sc-word">${esc(word)}</b></div>
        <div class="prow"><span class="lab" style="width:90px">Cambiar a</span><input type="text" id="sc-to" value="${esc(sug[0] || word)}" style="width:200px"></div>
        <div class="prow" style="align-items:flex-start"><span class="lab" style="width:90px">Sugerencias</span>
          <div class="swap-list" id="sc-sug" style="width:200px;max-height:130px">${sug.map((s, i) => `<div class="lib-row${i ? '' : ' sel'}" data-sug="${esc(s)}"><span class="lname">${esc(s)}</span></div>`).join('') || '<div class="muted lib-empty">Sin sugerencias</div>'}</div>
          <div class="spell-btns"><button class="btn small" id="sc-skip">Omitir</button><button class="btn small" id="sc-skipall">Omitir todo</button>
            <button class="btn small primary" id="sc-change">Cambiar</button><button class="btn small" id="sc-changeall">Cambiar todo</button>
            <button class="btn small" id="sc-add">Añadir a personal</button></div></div>
        <div class="modal-actions"><button class="btn" data-close>Cerrar</button></div>`, (b) => {
        $('#sc-sug', b).addEventListener('click', (e) => {
          const r = e.target.closest('[data-sug]');
          if (!r) return;
          $('#sc-to', b).value = r.dataset.sug;
          b.querySelectorAll('[data-sug]').forEach((x) => x.classList.toggle('sel', x === r));
        });
        $('#sc-sug', b).addEventListener('dblclick', () => $('#sc-change', b).click());
        $('#sc-skip', b).addEventListener('click', () => { pos = at + word.length; show(); });
        $('#sc-skipall', b).addEventListener('click', () => { ignored.add(word); pos = at; show(); });
        $('#sc-add', b).addEventListener('click', () => {
          const pr = prefs();
          pr.personal.push(word);
          app.savePref('omf.spell', pr);
          personal.add(word.toLowerCase());
          pos = at;
          show();
        });
        const replace = (target, from, to) => {
          if (ed.spell_replace(JSON.stringify(target.spot), from)) target.text = from;
          return to;
        };
        $('#sc-change', b).addEventListener('click', () => {
          const to = $('#sc-to', b).value;
          const text = item.text.slice(0, at) + to + item.text.slice(at + word.length);
          replace(item, text);
          pos = at + to.length;
          show();
        });
        $('#sc-changeall', b).addEventListener('click', () => {
          const to = $('#sc-to', b).value;
          const re = new RegExp(`(?<![\\p{L}\\p{M}])${word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\p{L}\\p{M}])`, 'gu');
          for (const x of items.slice(it)) {
            const t = x.text.replace(re, to);
            if (t !== x.text) replace(x, t);
          }
          pos = at + to.length;
          show();
        });
      });
    };
    show();
  }

  return { check, settingsDialog, dict };
}
