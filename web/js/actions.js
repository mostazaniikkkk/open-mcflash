// Actions (F9) and Output (F2) panels, docked below the Stage as in
// Flash 8. The Actions panel edits the script of the keyframe or of the
// selected button or movie clip instance; Output shows the trace() calls from
// "Test Movie" and compile errors. The editor highlights syntax and also
// opens the document's ActionScript (.as) files: AS 2.0 classes and code
// for #include.

const $ = (q, el = document) => el.querySelector(q);
const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

const TITLES = { frame: 'Acciones - Fotograma', button: 'Acciones - Botón', movieclip: 'Acciones - Clip de película', none: 'Acciones', script: 'Script' };

// Syntax highlighting (colors of Flash's Actions panel: keywords in blue,
// strings in green, comments in gray).
const BUILTINS = new Set(['stop', 'play', 'gotoAndPlay', 'gotoAndStop', 'nextFrame', 'prevFrame', 'trace', 'getURL', 'startDrag', 'stopDrag',
  'duplicateMovieClip', 'removeMovieClip', 'getProperty', 'setProperty', 'getTimer', 'random', 'int', 'chr', 'ord', 'eval', 'targetPath',
  'stopAllSounds', 'attachMovie', 'createEmptyMovieClip', 'loadMovie', 'setInterval', 'clearInterval', 'Math', 'String', 'Number', 'Array',
  'Object', 'MovieClip', 'Sound', 'Key', 'Mouse', 'Date', 'Error', 'TextField', 'Stage', 'Color', 'Boolean', 'Function', 'XML', 'LoadVars',
  '_root', '_parent', '_global', '_level0', '#include']);
let KEYWORDS = new Set();
export function highlight(code) {
  const re = /(\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$))|("(?:[^"\\\n]|\\.)*"?|'(?:[^'\\\n]|\\.)*'?)|(#include\b)|(\b\d+(?:\.\d+)?(?:e[+-]?\d+)?\b|\b0x[0-9a-f]+\b)|([A-Za-z_$][\w$]*)/gi;
  let out = '', last = 0, m;
  while ((m = re.exec(code))) {
    out += esc(code.slice(last, m.index));
    const t = esc(m[0]);
    if (m[1]) out += `<span class="hl-c">${t}</span>`;
    else if (m[2]) out += `<span class="hl-s">${t}</span>`;
    else if (m[3]) out += `<span class="hl-k">${t}</span>`;
    else if (m[4]) out += `<span class="hl-n">${t}</span>`;
    else if (KEYWORDS.has(m[5])) out += `<span class="hl-k">${t}</span>`;
    else if (BUILTINS.has(m[5]) || /^_[a-z]+$/.test(m[5])) out += `<span class="hl-b">${t}</span>`;
    else out += t;
    last = re.lastIndex;
  }
  return out + esc(code.slice(last)) + '\n';
}

const CLASS_TEMPLATE = (name) => {
  const short = name.replace(/\.as$/i, '').split('/').pop();
  const pkg = name.replace(/\.as$/i, '').split('/').slice(0, -1).join('.');
  const full = pkg ? `${pkg}.${short}` : short;
  return `class ${full} extends MovieClip {\n\tprivate var velocidad:Number = 5;\n\n\tfunction ${short}() {\n\t\tsuper();\n\t}\n\n\tfunction onEnterFrame():Void {\n\t\t_x += velocidad;\n\t}\n}\n`;
};

// "Add" menu (the "+" in Flash's panel): the most-used actions.
const SNIPPETS = {
  frame: [
    ['Control de línea de tiempo', [['stop', 'stop();'], ['play', 'play();'], ['gotoAndPlay', 'gotoAndPlay(1);'], ['gotoAndStop', 'gotoAndStop(1);'], ['nextFrame', 'nextFrame();'], ['prevFrame', 'prevFrame();']]],
    ['Navegador/red', [['getURL', 'getURL("http://", "_blank");']]],
    ['Varios', [['trace', 'trace("");'], ['var', 'var nombre = 0;'], ['function', 'function nombre() {\n\t\n}'], ['if', 'if (condición) {\n\t\n}'], ['for', 'for (var i = 0; i < 10; i++) {\n\t\n}']]],
    ['Controladores', [['onEnterFrame', 'this.onEnterFrame = function () {\n\t\n};'], ['onPress (clip)', 'mc.onPress = function () {\n\t\n};']]],
  ],
  button: [
    ['Controladores de botón', [['on (release)', 'on (release) {\n\t\n}'], ['on (press)', 'on (press) {\n\t\n}'], ['on (rollOver)', 'on (rollOver) {\n\t\n}'], ['on (rollOut)', 'on (rollOut) {\n\t\n}'], ['on (keyPress)', 'on (keyPress "<Enter>") {\n\t\n}']]],
    ['Control de línea de tiempo', [['gotoAndPlay', 'gotoAndPlay(1);'], ['gotoAndStop', 'gotoAndStop(1);'], ['getURL', 'getURL("http://", "_blank");']]],
  ],
  movieclip: [
    ['Controladores de clip', [['onClipEvent (load)', 'onClipEvent (load) {\n\t\n}'], ['onClipEvent (enterFrame)', 'onClipEvent (enterFrame) {\n\t\n}'], ['onClipEvent (mouseDown)', 'onClipEvent (mouseDown) {\n\t\n}'], ['on (press)', 'on (press) {\n\tstartDrag(this);\n}'], ['on (release)', 'on (release) {\n\tstopDrag();\n}']]],
  ],
};

export function initActions(app) {
  const { ed } = app;
  const dock = document.createElement('div');
  dock.id = 'dock';
  dock.hidden = true;
  dock.innerHTML = `
    <div class="dock-tabs">
      <button class="dock-tab" data-dock="actions">Acciones</button>
      <button class="dock-tab" data-dock="output">Salida</button>
      <div class="grow"></div>
      <button class="ib" id="dock-close" title="Cerrar">×</button>
    </div>
    <div class="dock-page" data-page="actions">
      <div class="act-bar">
        <select id="act-file" title="Qué se edita: las acciones del fotograma o instancia, o un archivo ActionScript (.as)"></select>
        <b id="act-title">Acciones</b><span class="muted" id="act-label"></span><div class="grow"></div>
        <button class="btn small" id="act-new" title="Nuevo archivo ActionScript (.as)">Nuevo .as…</button>
        <button class="btn small" id="act-del" title="Eliminar este archivo" hidden>Eliminar</button>
        <button class="btn small" id="act-add" title="Añadir una acción">+ Añadir</button>
        <button class="btn small" id="act-check" title="Revisar sintaxis">Revisar sintaxis</button>
      </div>
      <div class="act-editor"><div class="act-gutter" id="act-gutter"></div><div class="act-code"><pre class="act-hl" id="act-hl" aria-hidden="true"></pre><textarea id="act-src" spellcheck="false" wrap="off" placeholder="// Escribe aquí las acciones (ActionScript 2)"></textarea></div></div>
    </div>
    <div class="dock-page" data-page="output" hidden>
      <div class="act-bar"><b>Salida</b><div class="grow"></div><button class="btn small" id="out-clear">Borrar</button></div>
      <pre id="output" class="output"></pre>
    </div>`;
  $('#statusbar').before(dock);
  const src = $('#act-src', dock), gutter = $('#act-gutter', dock), out = $('#output', dock), hl = $('#act-hl', dock), fileSel = $('#act-file', dock);
  let target = null, sig = '', dirty = false, timer = 0, badLines = new Set();
  // Open .as file (null: the frame or instance actions).
  let file = null;
  try { KEYWORDS = new Set(JSON.parse(ed.as_keywords())); } catch {}

  function paint() {
    hl.innerHTML = highlight(src.value);
    hl.scrollTop = src.scrollTop;
    hl.scrollLeft = src.scrollLeft;
  }

  let filesSig = '';
  function files() {
    const list = JSON.parse(ed.scripts_list());
    const s = JSON.stringify(list.map((f) => f.name)) + file;
    if (s !== filesSig) {
      filesSig = s;
      fileSel.innerHTML = '<option value="">Fotograma o instancia actual</option>' + list.map((f) => `<option value="${esc(f.name)}">${esc(f.name)}</option>`).join('');
    }
    if (file && !list.some((f) => f.name === file)) file = null;
    fileSel.value = file || '';
    $('#act-del', dock).hidden = !file;
    return list;
  }

  function show(page) {
    dock.hidden = false;
    for (const t of dock.querySelectorAll('[data-dock]')) t.classList.toggle('on', t.dataset.dock === page);
    for (const p of dock.querySelectorAll('[data-page]')) p.hidden = p.dataset.page !== page;
    if (page === 'actions') sync(true);
  }
  const visible = (page) => !dock.hidden && !dock.querySelector(`[data-page="${page}"]`).hidden;
  function toggle(page) {
    if (visible(page)) dock.hidden = true;
    else show(page);
  }
  dock.addEventListener('click', (e) => {
    const t = e.target.closest('[data-dock]');
    if (t) show(t.dataset.dock);
  });
  $('#dock-close', dock).addEventListener('click', () => (dock.hidden = true));

  // ───────── Actions editor ─────────

  function lines() {
    const n = src.value.split('\n').length;
    gutter.innerHTML = Array.from({ length: n }, (_, i) => `<div class="${badLines.has(i + 1) ? 'bad' : ''}">${i + 1}</div>`).join('');
    gutter.scrollTop = src.scrollTop;
    paint();
  }

  function commit() {
    clearTimeout(timer);
    if (!dirty || !target || target.kind === 'none') return;
    dirty = false;
    if (target.kind === 'script') ed.set_script(target.name, src.value);
    else ed.set_actions(src.value);
    app.refresh();
  }

  function scriptTarget() {
    return { kind: 'script', name: file, label: file, src: ed.script_code(file) };
  }

  function sync(force = false) {
    if (dock.hidden && !force) return;
    files();
    const t = file ? scriptTarget() : JSON.parse(ed.actions_target());
    const s = `${t.kind}|${t.label}|${t.layer}|${t.key}|${app.state()?.sel?.count || 0}`;
    if (s === sig && !force) return;
    if (dirty && document.activeElement === src && s.split('|')[0] === sig.split('|')[0] && t.label === target?.label) return;
    commit();
    sig = s;
    target = t;
    $('#act-title', dock).textContent = TITLES[t.kind];
    $('#act-label', dock).textContent = t.kind === 'none' ? t.label : t.label;
    if (document.activeElement !== src || force) src.value = t.src;
    src.readOnly = t.kind === 'none';
    badLines = new Set();
    lines();
  }

  src.addEventListener('input', () => {
    dirty = true;
    clearTimeout(timer);
    timer = setTimeout(commit, 600);
    lines();
  });
  src.addEventListener('blur', commit);
  src.addEventListener('scroll', () => {
    gutter.scrollTop = src.scrollTop;
    hl.scrollTop = src.scrollTop;
    hl.scrollLeft = src.scrollLeft;
  });

  // .as files: choose, create and delete.
  function openFile(name) {
    commit();
    file = name || null;
    sig = '';
    sync(true);
  }
  fileSel.addEventListener('change', () => openFile(fileSel.value));
  $('#act-new', dock).addEventListener('click', () => {
    app.modal('Nuevo archivo ActionScript', `
      <div class="prow"><span class="lab" style="width:90px">Archivo</span><input type="text" id="as-name" value="MiClase.as" style="width:220px"></div>
      <p class="muted" style="max-width:340px">Una clase va en un archivo con su nombre, y los paquetes en carpetas: <code>juego/Pelota.as</code> para <code>class juego.Pelota</code>.</p>
      <div class="prow"><span class="lab" style="width:90px"></span><label class="chk"><input type="checkbox" id="as-class" checked> Empezar con una clase</label></div>
      <div class="modal-actions"><button class="btn" data-close>Cancelar</button><button class="btn primary" id="as-ok">Aceptar</button></div>`, (b) => {
      const inp = $('#as-name', b);
      inp.focus();
      inp.setSelectionRange(0, inp.value.length - 3);
      const ok = () => {
        let name = inp.value.trim().replace(/\\/g, '/');
        if (!name) return;
        if (!/\.as$/i.test(name)) name += '.as';
        const code = $('#as-class', b).checked ? CLASS_TEMPLATE(name) : '';
        app.closeModal();
        ed.set_script(name, code);
        app.refresh();
        show('actions');
        openFile(name);
      };
      $('#as-ok', b).addEventListener('click', ok);
      inp.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') ok(); });
    });
  });
  $('#act-del', dock).addEventListener('click', () => {
    if (!file) return;
    const name = file;
    file = null;
    dirty = false;
    ed.delete_script(name);
    app.refresh();
    openFile(null);
  });
  src.addEventListener('keydown', (e) => {
    e.stopPropagation();
    if (e.key === 'Tab') {
      e.preventDefault();
      const { selectionStart: a, selectionEnd: b } = src;
      src.setRangeText('\t', a, b, 'end');
      src.dispatchEvent(new Event('input'));
    } else if (e.key === 'Enter') {
      // Keep the previous line's indentation.
      e.preventDefault();
      const a = src.selectionStart;
      const line = src.value.slice(0, a).split('\n').pop();
      const indent = line.match(/^\s*/)[0] + (/\{\s*$/.test(line) ? '\t' : '');
      src.setRangeText('\n' + indent, a, src.selectionEnd, 'end');
      src.dispatchEvent(new Event('input'));
    }
  });

  function insert(code) {
    if (!target || target.kind === 'none') return app.toast(target ? target.label : 'Elige un fotograma clave o una instancia');
    const a = src.selectionStart ?? src.value.length;
    const pre = a > 0 && src.value[a - 1] !== '\n' ? '\n' : '';
    src.setRangeText(pre + code + '\n', a, src.selectionEnd ?? a, 'end');
    dirty = true;
    commit();
    lines();
    src.focus();
  }

  $('#act-add', dock).addEventListener('click', (e) => {
    const groups = target?.kind === 'script'
      ? [['Clases', [['class', CLASS_TEMPLATE(target.name)], ['function', 'function nombre():Void {\n\t\n}'], ['var', 'private var nombre:Number = 0;'], ['get/set', 'function get valor():Number {\n\treturn 0;\n}\nfunction set valor(v:Number):Void {\n}']]],
        ['Errores', [['try/catch', 'try {\n\t\n} catch (e:Error) {\n\ttrace(e.message);\n}'], ['throw', 'throw new Error("mensaje");']]]]
      : (SNIPPETS[target?.kind] || SNIPPETS.frame).concat(target?.kind === 'frame' ? [['Errores e inclusión', [['try/catch', 'try {\n\t\n} catch (e) {\n\ttrace(e);\n}'], ['throw', 'throw new Error("mensaje");'], ['#include', '#include "archivo.as"'], ['import', 'import paquete.Clase;'], ['attachMovie', 'this.attachMovie("identificador", "nombre", this.getNextHighestDepth());']]]] : []);
    const r = e.currentTarget.getBoundingClientRect();
    app.contextMenu(r.left, r.bottom + 2, groups.map(([g, items]) => ({ label: g, sub: items.map(([l, code]) => ({ label: l, action: () => insert(code) })) })));
  });

  function check() {
    commit();
    const errs = JSON.parse(target && target.kind === 'script' ? ed.check_script(target.name, src.value) : ed.check_actions(src.value));
    badLines = new Set(errs.map((e) => e.line));
    lines();
    output.clear();
    if (!errs.length) output.append('No hay errores en el script.');
    else {
      output.append(`**Error** ${target?.label || ''}: ${errs.length} error${errs.length === 1 ? '' : 'es'}`);
      errs.forEach((e) => output.append(`     Línea ${e.line}: ${e.msg}`));
      show('output');
    }
    return errs;
  }
  $('#act-check', dock).addEventListener('click', check);

  // ───────── Output ─────────

  const output = {
    append(msg) {
      out.textContent += (out.textContent ? '\n' : '') + msg;
      out.scrollTop = out.scrollHeight;
    },
    clear() {
      out.textContent = '';
    },
    get text() {
      return out.textContent;
    },
  };
  $('#out-clear', dock).addEventListener('click', () => output.clear());

  return { show, toggle, sync, check, output, commit, visible, insert, openFile, get file() { return file; } };
}
