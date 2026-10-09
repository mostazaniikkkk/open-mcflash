"""Block 7 — The last Flash 8 features: Synchronize Symbols, JPEG bitmaps,
linear RGB gradients, text with multiple formats (kerning, document and
embedded fonts), ADPCM compression and sound envelopes, the History slider
and ActionScript (highlighting, try/catch, .as classes, #include and linkage
with attachMovie)."""

import base64
import json
import time

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.support.ui import Select

from conftest import Keys, swf_codes, swf_plain
from test_13_importar_publicar_comandos import import_file, make_png
from test_15_actionscript_y_sonido import frame_key, make_wav, write_actions


def doc_shapes(app, layer=0, frame=0):
    lay = app.doc()["layers"][layer]
    return lay["keys"][str(frame)]["shapes"]


def swf(app):
    return app.js("return Array.from(window.omf.ed.export_swf())")


def convert(app, kind):
    app.key(Keys.F8)
    app.el(f'input[name="cs-kind"][value="{kind}"]').click()
    app.el("#cs-ok").click()
    time.sleep(0.1)


# ───────── Timeline and drawing ─────────

def test_sincronizar_grafico_en_interpolacion(app):
    app.object_mode(True)
    app.rect((100, 100), (140, 140))
    app.tool("select")
    app.click((120, 120))
    convert(app, "graphic")
    app.click_xy(*app.cell(0, 19))
    app.key(Keys.F6)
    app.click_xy(*app.cell(0, 0))
    Select(app.el("#pp-tween")).select_by_value("motion")
    sync = app.el("#pp-syncg")
    assert not sync.is_selected()
    sync.click()
    assert app.state()["tween"]["sync"] is True
    assert app.doc()["layers"][0]["keys"]["0"]["sync"] is True
    sync.click()
    assert not app.state()["tween"]["sync"]


def test_rgb_lineal_en_el_mezclador(app):
    Select(app.el("#mx-type")).select_by_value("linear")
    lin = app.el("#mx-linear")
    app.js("arguments[0].scrollIntoView({block: 'center'})", lin)
    lin.click()
    assert app.state()["fillGrad"]["linear_rgb"] is True
    app.js("window.omf.ed.set_stroke('#000000', 255, false); window.omf.refresh()")
    app.rect((100, 100), (300, 160))
    assert doc_shapes(app)[0]["grad"]["linear_rgb"] is True
    codes = swf_codes(swf(app))
    assert 32 in codes or 83 in codes


# ───────── Bitmaps ─────────

def test_compresion_jpeg_del_mapa_de_bits(app, tmp_path):
    import_file(app, "Importar a biblioteca…", make_png(tmp_path / "foto.png", 80, 60))
    sym = next(x for x in app.state()["library"] if x["kind"] == "bitmap")
    row = next(r for r in app.els("#lib-list [data-sym]") if "foto.png" in r.text)
    row.click()
    ActionChains(app.d).double_click(row.find_element("css selector", ".licon")).perform()
    assert app.modal_open()
    Select(app.el("#bp-comp")).select_by_value("jpeg")
    app.wait(lambda: "comprimido" in app.el("#bp-size").text, 6, "tamaño del JPEG")
    app.set_input("#bp-q", 50)
    app.el("#bp-ok").click()
    lib = app.wait(lambda: next((x for x in app.state()["library"] if x["id"] == sym["id"] and x["quality"] == 50), None), 6, "calidad guardada")
    assert lib["jpegBytes"] > 0
    # In the movie: DefineBitsJPEG2 instead of Lossless.
    sx, sy = app.screen(300, 200)
    app.js("const c = document.querySelector('#stage').getBoundingClientRect(); window.omf.ed.place_symbol(arguments[0], arguments[1] - c.left, arguments[2] - c.top); window.omf.refresh()", sym["id"], sx, sy)
    codes = swf_codes(swf(app))
    assert 21 in codes and 36 not in codes
    app.test_movie()
    px = app.wait(lambda: [p for p in [app.ruffle_pixel(x, 200) for x in range(250, 360, 4)] if p[2] > p[0] + 40], 6, "se ve la imagen")
    assert px
    app.key(Keys.ESCAPE)


# ───────── Text ─────────

def edit_text(app, at, text):
    app.tool("text")
    app.click(at)
    app.wait(lambda: app.els(".text-edit"), msg="cuadro de edición")
    ActionChains(app.d).send_keys(text).perform()
    time.sleep(0.15)


def test_formato_de_una_parte_del_texto(app):
    edit_text(app, (100, 100), "Hola mundo")
    # Select "mundo" with the keyboard and make it bold, bigger and red.
    ac = ActionChains(app.d).key_down(Keys.SHIFT)
    for _ in range(5):
        ac.send_keys(Keys.ARROW_LEFT)
    ac.key_up(Keys.SHIFT).perform()
    time.sleep(0.2)
    app.el("#pt-bold").click()
    app.set_input("#pt-size", 40)
    app.el("#pt-color").click()
    hx = app.el("#pc-hex")
    hx.send_keys(Keys.CONTROL, "a")
    hx.send_keys("#FF0000", Keys.ENTER)
    time.sleep(0.1)
    if app.els(".text-edit"):
        app.key(Keys.ESCAPE)
    tf = next(s["geom"] for s in doc_shapes(app) if s["geom"]["type"] == "Text")
    runs = tf["runs"]
    assert [r["len"] for r in runs] == [5, 5], runs
    assert runs[1]["bold"] and runs[1]["size"] == 40 and not runs[0]["bold"]
    assert runs[1]["color"] == {"r": 255, "g": 0, "b": 0, "a": 255} and "color" not in runs[0]
    # One shape per format in the SWF, and the text shows in Ruffle.
    codes = swf_codes(swf(app))
    assert sum(1 for c in codes if c in (2, 32, 83)) >= 2
    # Edit again: the box shows the runs.
    app.tool("select")
    t = next(s for s in app.frame(0, 0))
    app.dblclick(((t["x0"] + t["x1"]) / 2, (t["y0"] + t["y1"]) / 2))
    app.wait(lambda: len(app.els(".text-edit span")) == 2, 4, "un tramo por formato")
    assert "700" in app.els(".text-edit span")[1].get_attribute("style") or "bold" in app.els(".text-edit span")[1].get_attribute("style")
    app.key(Keys.ESCAPE)


def test_kerning_e_incorporar_fuente_en_texto_dinamico(app):
    edit_text(app, (60, 100), "AVAVAVAV WAVE")
    app.key(Keys.ESCAPE)
    w_kern = app.frame(0, 0)[0]
    kern = app.el("#pt-kern")
    assert kern.is_selected(), "kerning activado por defecto"
    kern.click()
    w_none = app.frame(0, 0)[0]
    assert (w_none["x1"] - w_none["x0"]) > (w_kern["x1"] - w_kern["x0"]) + 2, "sin kerning es más ancho"
    kern.click()
    # Dynamic with the embedded font: DefineFont3 with the outlines.
    Select(app.el("#pt-kind")).select_by_value("dynamic")
    app.el("#pt-embed").click()
    app.el("#em-on").click()
    app.el("#em-ok").click()
    tf = doc_shapes(app)[0]["geom"]
    assert tf["embed"] is True and tf["kind"] == "dynamic"
    codes = swf_codes(swf(app))
    assert 75 in codes and 37 in codes
    app.test_movie()
    dark = app.wait(lambda: sum(1 for x in range(60, 300, 3) for y in (110, 118, 124) if sum(app.ruffle_pixel(x, y)) < 300), 8, "el texto incorporado se dibuja")
    assert dark > 5
    app.key(Keys.ESCAPE)


def test_fuente_de_archivo_viaja_con_el_documento(app, tmp_path):
    data = app.js("return Array.from(window.omf.ed.font_data('Tinos', false, false))")
    f = tmp_path / "MiLetra.ttf"
    f.write_bytes(bytes(data))
    edit_text(app, (100, 100), "Fuente propia")
    app.key(Keys.ESCAPE)
    app.el("#pt-morefonts").click()
    app.el("#mf-file").send_keys(str(f))
    app.wait(lambda: doc_shapes(app)[0]["geom"]["font"] == "MiLetra", 6, "el texto usa la fuente nueva")
    saved = app.js("return window.omf.ed.to_json()")
    doc = json.loads(saved)["document"]
    assert [x["family"] for x in doc["fonts"]] == ["MiLetra"]
    assert base64.b64decode(doc["fonts"][0]["data"])[:4] == bytes(data[:4])
    # Opening the document in another session: the font is registered again.
    app.js("localStorage.clear()")
    app.reload()
    app.js("window.omf.ed.load_json(arguments[0]); window.omf.refresh()", saved)
    assert "MiLetra" in app.state()["fonts"]


# ───────── Sound ─────────

def import_sound(app, tmp_path, name="s.wav", seconds=0.5):
    app.menu("Archivo", "Importar", "Importar a biblioteca…")
    app.wait(lambda: app.els("#import-input"))[0].send_keys(make_wav(tmp_path / name, seconds))
    app.wait(lambda: any(x["kind"] == "sound" for x in app.state()["library"]), 8, "el sonido llega a la biblioteca")
    return next(x for x in app.state()["library"] if x["kind"] == "sound")


def test_compresion_adpcm(app, tmp_path):
    snd = import_sound(app, tmp_path, "pitido.wav", 1.0)
    app.js("window.omf.ed.set_frame_sound(JSON.stringify({id: arguments[0], sync: 'event', loops: 1})); window.omf.refresh()", snd["id"])
    raw_len = len(swf_plain(swf(app)))
    row = next(r for r in app.els("#lib-list [data-sym]") if "pitido.wav" in r.text)
    row.click()
    ActionChains(app.d).double_click(row.find_element("css selector", ".licon")).perform()
    Select(app.el("#sd-comp")).select_by_value("adpcm")
    Select(app.el("#sd-bits")).select_by_value("3")
    assert "en la película" in app.el("#sd-size").text
    app.el("#sd-play").click()
    app.el("#sd-stop").click()
    app.el("#sd-ok").click()
    info = json.loads(app.js("return window.omf.ed.sound_info(arguments[0])", snd["id"]))
    assert info["adpcm"] == 3
    data = swf_plain(swf(app))
    assert len(data) < raw_len - 25000, (len(data), raw_len)
    app.test_movie()
    app.key(Keys.ESCAPE)
    # Undo goes back to "Sin formato" (no format).
    app.combo(Keys.CONTROL, "z")
    assert json.loads(app.js("return window.omf.ed.sound_info(arguments[0])", snd["id"]))["adpcm"] == 0


def test_efectos_y_envolvente_de_sonido(app, tmp_path):
    snd = import_sound(app, tmp_path)
    app.click_xy(*app.cell(0, 0))
    Select(app.el("#pp-sound")).select_by_value(str(snd["id"]))
    Select(app.el("#pp-sfx")).select_by_value("fadeIn")
    s = frame_key(app)["sound"]
    assert s["effect"] == "fadeIn" and len(s["env"]) == 2 and s["env"][0]["l"] == 0
    # Edit Envelope: one more handle on the left channel and the in point.
    app.el('[data-cmd="soundEnvDialog"]').click()
    cv = app.el("#ev-c")
    r = cv.rect
    app.click_xy(r["x"] + r["width"] * 0.6, r["y"] + 18 + 3 + 70)
    app.drag_xy((r["x"] + 1, r["y"] + 6), (r["x"] + r["width"] * 0.1, r["y"] + 6))
    assert Select(app.el("#ev-fx")).first_selected_option.get_attribute("value") == "custom"
    app.el("#ev-play").click()
    app.el("#ev-ok").click()
    s = frame_key(app)["sound"]
    assert s["effect"] == "custom" and len(s["env"]) == 3 and s["in"] > 1000
    # SOUNDINFO with envelope in the StartSound.
    data = swf_plain(swf(app))
    codes = swf_codes(data)
    assert 15 in codes
    app.test_movie()
    app.key(Keys.ESCAPE)


# ───────── History ─────────

def test_deslizador_del_historial(app):
    app.menu("Ventana", "Otros paneles", "Historial")
    app.object_mode(True)
    for i in range(3):
        app.rect((50 + i * 80, 50), (100 + i * 80, 100))
    rows = app.els("#hist-list [data-step]")
    assert [r.text for r in rows] == ["Rectángulo"] * 3
    assert app.els("#hist-list [data-step]")[2].find_elements("css selector", ".hist-knob")
    # Drag the slider to the first step: the other two are undone.
    gut = lambda i: app.els("#hist-list [data-step] .hist-gut")[i].rect
    g2, g0 = gut(2), gut(0)
    app.drag_xy((g2["x"] + 8, g2["y"] + g2["height"] / 2), (g0["x"] + 8, g0["y"] + g0["height"] / 2))
    assert len(app.frame(0, 0)) == 1
    rows = app.els("#hist-list [data-step]")
    assert [("undone" in r.get_attribute("class")) for r in rows] == [False, True, True]
    # Moving it back down redoes.
    g2 = gut(2)
    app.click_xy(g2["x"] + 8, g2["y"] + g2["height"] / 2)
    assert len(app.frame(0, 0)) == 3
    # "Documento": before the first step.
    st = app.el("#hist-list [data-hstart] .hist-gut").rect
    app.click_xy(st["x"] + 8, st["y"] + st["height"] / 2)
    assert app.frame(0, 0) == []
    # A new step discards the undone ones.
    app.rect((300, 300), (340, 340))
    rows = app.els("#hist-list [data-step]")
    assert [r.text for r in rows] == ["Rectángulo"]
    # Ctrl+Z also moves the slider.
    app.combo(Keys.CONTROL, "z")
    assert "undone" in app.els("#hist-list [data-step]")[0].get_attribute("class")


# ───────── ActionScript ─────────

def test_resaltado_y_try_catch_throw(app):
    write_actions(app, '// prueba\nvar n = 3;\ntry {\n\tthrow new Error("mal");\n} catch (e) {\n\ttrace("atrapado " + e.message);\n} finally {\n\ttrace("fin " + n);\n}')
    assert app.els("#act-hl .hl-k") and app.els("#act-hl .hl-s") and app.els("#act-hl .hl-c")
    kws = [e.text for e in app.els("#act-hl .hl-k")]
    assert "try" in kws and "catch" in kws and "throw" in kws
    app.el("#act-check").click()
    assert "No hay errores" in app.output()
    app.test_movie()
    app.wait(lambda: "atrapado mal" in app.output() and "fin 3" in app.output(), 10, "try/catch/finally en Ruffle")
    app.key(Keys.ESCAPE)


def new_script(app, name, code):
    if not app.el("#act-src").is_displayed():
        app.key(Keys.F9)
    app.el("#act-new").click()
    app.set_input("#as-name", name)
    app.el("#as-class").click()
    app.el("#as-ok").click()
    assert Select(app.el("#act-file")).first_selected_option.get_attribute("value") == name
    app.js("const t = document.querySelector('#act-src'); t.value = arguments[0]; t.dispatchEvent(new Event('input'));", code)
    app.blur()
    time.sleep(0.8)


def test_clases_include_y_vinculacion(app):
    new_script(app, "juego/Pelota.as", 'class juego.Pelota extends MovieClip {\n\tvar nombre:String = "pelota";\n\tstatic var cuantas:Number = 0;\n\tfunction Pelota() {\n\t\tcuantas++;\n\t\ttrace("ctor " + cuantas);\n\t}\n\tfunction saluda():String {\n\t\treturn "hola " + nombre;\n\t}\n\tfunction get doble():Number { return cuantas * 2; }\n}\n')
    app.el("#act-check").click()
    assert "No hay errores" in app.output()
    assert [x["name"] for x in json.loads(app.js("return window.omf.ed.scripts_list()"))] == ["juego/Pelota.as"]
    new_script(app, "util.as", "function triple(x) { return x * 3; }\n")
    # A clip exported for ActionScript with the class.
    app.object_mode(True)
    app.rect((100, 100), (140, 140))
    app.tool("select")
    app.click((120, 120))
    convert(app, "movieclip")
    sym = app.state()["inst"]["symbol"]
    app.key(Keys.DELETE)
    row = next(r for r in app.els("#lib-list [data-sym]") if r.get_attribute("data-sym") == str(sym))
    app.click_xy(row.rect["x"] + 30, row.rect["y"] + row.rect["height"] / 2, button="right")
    app._walk(["Vinculación…"])
    app.el("#lk-export").click()
    app.set_input("#lk-id", "bola")
    app.set_input("#lk-class", "juego.Pelota")
    app.el("#lk-ok").click()
    assert next(x for x in app.state()["library"] if x["id"] == sym)["linkage"] == {"id": "bola", "class": "juego.Pelota"}
    # Frame: back to the frame actions, #include, import and attachMovie.
    Select(app.el("#act-file")).select_by_value("")
    write_actions(app, '#include "util.as"\nimport juego.Pelota;\nvar p = attachMovie("bola", "b1", 1);\np._x = 200;\ntrace("es " + (p instanceof Pelota) + " " + p.saluda() + " " + triple(2) + " " + p.doble);')
    app.el("#act-check").click()
    assert "No hay errores" in app.output()
    codes = swf_codes(swf(app))
    assert 56 in codes and 59 in codes, "ExportAssets y DoInitAction"
    app.test_movie()
    app.wait(lambda: "es true hola pelota 6 2" in app.output(), 10, "la clase, la vinculación y el #include funcionan")
    assert "ctor 1" in app.output()
    app.key(Keys.ESCAPE)
    # Saving and opening keeps the scripts and the linkage.
    data = app.js("return window.omf.ed.to_json()")
    doc = json.loads(data)["document"]
    assert doc["scripts"][0]["name"] == "juego/Pelota.as"
    app.js("window.omf.ed.new_document(550, 400); window.omf.ed.load_json(arguments[0]); window.omf.refresh()", data)
    assert len(json.loads(app.js("return window.omf.ed.scripts_list()"))) == 2


def test_errores_en_archivo_de_clase(app):
    new_script(app, "Mal.as", "class Mal {\n\tfunction f() {\n\t\treturn (;\n\t}\n}\n")
    app.el("#act-check").click()
    assert "Línea 3" in app.output()
    assert app.els("#act-gutter .bad")
    app.test_movie()
    assert "**Error** Mal.as: Línea 3" in app.output()
    app.key(Keys.ESCAPE)
    # Delete the file.
    app.el('[data-dock="actions"]').click()
    app.el("#act-del").click()
    assert json.loads(app.js("return window.omf.ed.scripts_list()")) == []
