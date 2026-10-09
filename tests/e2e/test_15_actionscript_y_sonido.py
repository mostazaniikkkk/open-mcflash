"""Block 6 — ActionScript and sound: Actions panel (frame, button and clip),
"Test Movie" with Ruffle and the Output panel, syntax checking and errors,
simple frame actions in the editor, and importing sounds and placing them on
frames (with their waveform in the Timeline and in the SWF)."""

import json
import math
import struct
import time

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.support.ui import Select

from conftest import Keys, swf_codes

BLUE = [51, 102, 204]


def write_actions(app, code):
    """Types into the Actions panel (opens it with F9 if needed)."""
    if not app.el("#act-src").is_displayed():
        app.key(Keys.F9)
    src = app.el("#act-src")
    src.click()
    src.send_keys(Keys.CONTROL, "a")
    src.send_keys(Keys.DELETE)
    # No auto-indent: each line as is.
    app.js("const t = document.querySelector('#act-src'); t.value = arguments[0]; t.dispatchEvent(new Event('input'));", code)
    app.blur()
    time.sleep(0.1)


def frame_key(app, layer=0, frame=0):
    return next(k for k in app.state()["layers"][layer]["keys"] if k["f"] == frame)


def make_wav(path, seconds=0.5, rate=22050):
    n = int(seconds * rate)
    data = b"".join(struct.pack("<h", int(12000 * math.sin(i * 0.12))) for i in range(n))
    hdr = b"RIFF" + struct.pack("<I", 36 + len(data)) + b"WAVEfmt " + struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16) + b"data" + struct.pack("<I", len(data))
    path.write_bytes(hdr + data)
    return str(path)


# ───────── Actions panel and Test Movie ─────────

def test_acciones_de_fotograma_y_trace_en_salida(app):
    app.key(Keys.F9)
    assert app.el("#act-title").text == "Acciones - Fotograma"
    assert "Capa 1 : 1" in app.el("#act-label").text
    write_actions(app, 'var nombre = "OMF";\ntrace("hola " + nombre);')
    assert frame_key(app)["actions"], "el fotograma lleva la «a»"
    app.test_movie()
    app.wait(lambda: "hola OMF" in app.output(), 10, "el trace llega al panel Salida")
    assert app.el("#output").is_displayed()
    app.key(Keys.ESCAPE)


def test_stop_detiene_la_pelicula(app):
    app.click_xy(*app.cell(0, 2))
    app.key(Keys.F7)
    app.click_xy(*app.cell(0, 0))
    write_actions(app, "trace('uno'); stop();")
    app.click_xy(*app.cell(0, 2))
    write_actions(app, "trace('tres');")
    app.test_movie()
    app.wait(lambda: "uno" in app.output(), 10)
    time.sleep(0.8)
    assert "tres" not in app.output(), "stop() en el primer fotograma"
    app.key(Keys.ESCAPE)


def test_boton_on_release_y_clip_on_clip_event(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 160))
    app.tool("select")
    app.click((150, 130))
    app.key(Keys.F8)
    app.el('input[name="cs-kind"][value="button"]').click()
    app.el("#cs-ok").click()
    app.key(Keys.F9)
    assert app.el("#act-title").text == "Acciones - Botón"
    write_actions(app, 'on (release) {\n\ttrace("clic");\n}')
    # A movie clip with onClipEvent(load).
    app.rect((300, 100), (350, 150))
    app.tool("select")
    app.click((325, 125))
    app.key(Keys.F8)
    app.el('input[name="cs-kind"][value="movieclip"]').click()
    app.el("#cs-ok").click()
    assert app.el("#act-title").text == "Acciones - Clip de película"
    write_actions(app, 'onClipEvent (load) {\n\ttrace("clip cargado " + this._name);\n}')
    app.set_input("#pi-name", "caja")
    app.test_movie()
    app.wait(lambda: "clip cargado caja" in app.output(), 10)
    el, ox, oy = app.ruffle_point(150, 130)
    ActionChains(app.d).move_to_element_with_offset(el, ox, oy).click().perform()
    app.wait(lambda: "clic" in app.output().replace("clip", ""), 6, "on (release) del botón")
    app.key(Keys.ESCAPE)


def test_revisar_sintaxis_y_errores_al_probar(app):
    write_actions(app, "stop();\ngotoAndPlay(;")
    app.el("#act-check").click()
    out = app.output()
    assert "Línea 2" in out and app.el("#output").is_displayed()
    assert app.els("#act-gutter .bad"), "la línea con error se marca"
    app.test_movie()
    out = app.output()
    assert "**Error** Escena=Escena 1, capa=Capa 1, fotograma=1" in out and "Línea 2" in out
    app.key(Keys.ESCAPE)
    # On a button, actions must go inside on (…).
    app.object_mode(True)
    app.rect((100, 100), (150, 150))
    app.tool("select")
    app.click((125, 125))
    app.key(Keys.F8)
    app.el('input[name="cs-kind"][value="button"]').click()
    app.el("#cs-ok").click()
    write_actions(app, "play();")
    app.el("#act-check").click()
    assert "controlador on" in app.output()


def test_menu_anadir_y_graficos_sin_acciones(app):
    app.key(Keys.F9)
    app.el("#act-add").click()
    app._walk(["Control de línea de tiempo", "stop"])
    assert "stop();" in app.el("#act-src").get_attribute("value")
    app.blur()
    time.sleep(0.7)
    assert frame_key(app)["actions"]
    app.object_mode(True)
    app.rect((100, 100), (150, 150))
    app.tool("select")
    app.click((125, 125))
    app.key(Keys.F8)
    app.el('input[name="cs-kind"][value="graphic"]').click()
    app.el("#cs-ok").click()
    assert app.el("#act-src").get_attribute("readOnly") is not None
    assert "gráficos" in app.el("#act-label").text


def test_acciones_de_fotograma_simples(app):
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 9))
    app.key(Keys.F5)
    app.click_xy(*app.cell(0, 3))
    app.key(Keys.F6)
    write_actions(app, "stop();")
    app.menu("Control", "Habilitar acciones de fotograma simples")
    app.js("window.omf.ed.set_frame(0); window.omf.ed.set_document(550, 400, '#ffffff', 60); window.omf.refresh()")
    app.key(Keys.ENTER)
    time.sleep(0.6)
    assert app.state()["frame"] == 3, "se detiene en el stop()"
    assert not app.js("return document.querySelector('#playbtn').classList.contains('on')")


# ───────── Sound ─────────

def test_importar_sonido_y_ponerlo_en_un_fotograma(app, tmp_path):
    app.menu("Archivo", "Importar", "Importar a biblioteca…")
    app.wait(lambda: app.els("#import-input"))[0].send_keys(make_wav(tmp_path / "pitido.wav"))
    app.wait(lambda: any(x["kind"] == "sound" for x in app.state()["library"]), 8, "el sonido llega a la biblioteca")
    snd = next(x for x in app.state()["library"] if x["kind"] == "sound")
    info = json.loads(app.js("return window.omf.ed.sound_info(arguments[0])", snd["id"]))
    assert abs(info["seconds"] - 0.5) < 0.02 and info["rate"] == 22050
    # On frame 1, with Event sync and 2 loops.
    app.click_xy(*app.cell(0, 11))
    app.key(Keys.F5)
    app.click_xy(*app.cell(0, 0))
    Select(app.el("#pp-sound")).select_by_value(str(snd["id"]))
    app.set_input("#pp-loops", 2)
    k = frame_key(app)
    assert k["sound"] == {"id": snd["id"], "sync": "event", "loops": 2}
    # The waveform is drawn in the Timeline.
    blue = app.js("""
        const c = document.querySelector('#tl-grid'), g = c.getContext('2d');
        const t = window.omf.timeline, r = t.rows[0], dpr = devicePixelRatio;
        const d = g.getImageData(t.cellW * 2 * dpr, (20 + r.y) * dpr, t.cellW * 3 * dpr, r.h * dpr).data;
        let n = 0; for (let i = 0; i < d.length; i += 4) if (d[i] < 80 && d[i + 2] > 180) n++;
        return n;""")
    assert blue > 10
    codes = swf_codes(app.js("return Array.from(window.omf.ed.export_swf())"))
    assert 14 in codes and 15 in codes, "DefineSound y StartSound"
    # Stream.
    Select(app.el("#pp-sync")).select_by_value("stream")
    assert frame_key(app)["sound"]["sync"] == "stream"
    codes = swf_codes(app.js("return Array.from(window.omf.ed.export_swf())"))
    assert 45 in codes and 19 in codes
    # Remove it.
    Select(app.el("#pp-sound")).select_by_value("")
    assert frame_key(app)["sound"] is None


def test_biblioteca_de_sonidos(app, tmp_path):
    app.menu("Archivo", "Importar", "Importar a biblioteca…")
    app.wait(lambda: app.els("#import-input"))[0].send_keys(make_wav(tmp_path / "golpe.wav", 0.3))
    app.wait(lambda: any(x["kind"] == "sound" for x in app.state()["library"]), 8)
    row = next(r for r in app.els("#lib-list [data-sym]") if "golpe.wav" in r.text)
    row.click()
    assert app.el("#lib-play").is_displayed(), "botón para escucharlo"
    app.el("#lib-play").click()
    ActionChains(app.d).double_click(row.find_element("css selector", ".licon")).perform()
    assert app.modal_open() and "22.1 kHz" in app.el("#modal-body").text
    app.el("[data-close]").click()
    # Deleting it removes it from the frames.
    sid = next(x["id"] for x in app.state()["library"] if x["kind"] == "sound")
    app.js("window.omf.ed.set_frame_sound(JSON.stringify({id: arguments[0], sync: 'event', loops: 1})); window.omf.refresh()", sid)
    assert frame_key(app)["sound"]
    row = next(r for r in app.els("#lib-list [data-sym]") if "golpe.wav" in r.text)
    row.click()
    app.el("#lib-delete").click()
    if app.modal_open():
        app.el("#ld-ok").click()
    assert frame_key(app)["sound"] is None


def test_guardar_y_abrir_con_acciones_y_sonido(app, tmp_path):
    write_actions(app, "stop();")
    app.menu("Archivo", "Importar", "Importar a biblioteca…")
    app.wait(lambda: app.els("#import-input"))[0].send_keys(make_wav(tmp_path / "s.wav", 0.2))
    app.wait(lambda: any(x["kind"] == "sound" for x in app.state()["library"]), 8)
    data = app.js("return window.omf.ed.to_json()")
    app.js("window.omf.ed.new_document(550, 400); window.omf.refresh()")
    app.js("window.omf.ed.load_json(arguments[0]); window.omf.refresh()", data)
    assert frame_key(app)["actions"]
    assert any(x["kind"] == "sound" for x in app.state()["library"])
