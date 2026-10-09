"""Completing the remaining menus: File > Import (bitmaps and bitmap fills),
Publish Settings / Publish / HTML preview, History panel and Commands menu,
and Text > Check Spelling."""

import struct
import time
import zlib

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.support.ui import Select

from conftest import Keys, swf_codes, swf_plain


def make_png(path, w=60, h=40):
    """Test PNG: left half blue, right half red."""
    rows = b"".join(b"\x00" + b"".join((b"\x00\x00\xff" if x < w // 2 else b"\xff\x00\x00") for x in range(w)) for _ in range(h))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
    path.write_bytes(data)
    return str(path)


def import_file(app, labels, path):
    app.menu("Archivo", "Importar", labels)
    inp = app.wait(lambda: app.els("#import-input"), msg="selector de archivos")[0]
    inp.send_keys(path)
    app.wait(lambda: any(x["kind"] == "bitmap" for x in app.state()["library"]), 6, "la imagen llega a la biblioteca")
    time.sleep(0.1)


def shapes(app):
    return app.frame(0, 0)


def bitmaps(app):
    return [x for x in app.state()["library"] if x["kind"] == "bitmap"]


# ───────── Import ─────────

def test_importar_al_escenario(app, tmp_path):
    import_file(app, "Importar al escenario…", make_png(tmp_path / "foto.png"))
    b = bitmaps(app)
    assert len(b) == 1 and b[0]["name"] == "foto.png" and b[0]["size"] == [60, 40]
    s = shapes(app)
    assert len(s) == 1 and s[0]["instance"] == b[0]["id"]
    assert abs(s[0]["x0"]) < 0.5 and abs(s[0]["x1"] - 60) < 0.5, "en la esquina del escenario, a su tamaño"
    assert app.pixel(10, 20) == [0, 0, 255] and app.pixel(50, 20) == [255, 0, 0]
    assert app.state()["inst"]["kind"] == "bitmap"
    assert app.el("#pb-props .ptitle").text.startswith("Mapa de bits")
    # Ctrl+B: a shape with a bitmap fill.
    app.combo(Keys.CONTROL, "b")
    s = shapes(app)
    assert not any(x["instance"] for x in s) and any(x["bmp"] for x in s)
    assert app.pixel(10, 20) == [0, 0, 255]
    # SWF with the image (DefineBitsLossless2).
    assert 36 in swf_codes(app.js("return Array.from(window.omf.ed.export_swf())"))


def test_importar_a_la_biblioteca_y_arrastrar(app, tmp_path):
    import_file(app, "Importar a biblioteca…", make_png(tmp_path / "logo.png"))
    assert shapes(app) == [], "solo a la biblioteca"
    row = next(r for r in app.els("#lib-list [data-sym]") if "logo.png" in r.text)
    r = row.rect
    app.drag_xy((r["x"] + r["width"] / 2, r["y"] + r["height"] / 2), app.screen(300, 200), steps=10)
    s = shapes(app)
    assert len(s) == 1 and s[0]["instance"]
    # Bitmap Properties: double-click the icon.
    row = next(r for r in app.els("#lib-list [data-sym]") if "logo.png" in r.text)
    ActionChains(app.d).double_click(row.find_element("css selector", ".licon")).perform()
    assert app.modal_open() and "60 × 40" in app.el("#modal-body").text
    app.el("#bp-smooth").click()
    app.el("#bp-ok").click()
    assert bitmaps(app)[0]["smooth"] is False


def test_relleno_de_mapa_de_bits(app, tmp_path):
    import_file(app, "Importar a biblioteca…", make_png(tmp_path / "tela.png", 20, 20))
    app.object_mode(True)
    Select(app.el("#mx-type")).select_by_value("bitmap")
    assert app.state()["fillBmp"]
    assert app.els("#mx-bmps img")
    app.js("window.omf.ed.set_stroke('#000000', 255, false); window.omf.refresh()")
    app.rect((100, 100), (180, 160))
    s = shapes(app)[0]
    assert s["bmp"] and abs(s["bmp"]["m"]["tx"] - 100) < 0.5
    # Tiled: the pattern repeats every 20 px.
    assert app.pixel(105, 110) == [0, 0, 255] and app.pixel(115, 110) == [255, 0, 0] and app.pixel(125, 110) == [0, 0, 255]
    # Fill Transform: move the bitmap.
    app.tool("filltransform")
    app.click((140, 130))
    app.drag((110, 110), (120, 110))
    assert abs(shapes(app)[0]["bmp"]["m"]["tx"] - 110) < 0.5
    # A solid color replaces it.
    Select(app.el("#mx-type")).select_by_value("solid")
    assert app.state()["fillBmp"] is None


# ───────── Publish ─────────

def test_configuracion_de_publicacion_y_publicar(app):
    app.rect((100, 100), (200, 200))
    app.menu("Archivo", "Configuración de publicación…")
    assert app.modal_open()
    app.set_input("#pb-name", "mi-peli")
    app.el('[data-ptab="flash"]').click()
    app.el("#pb-compress").click()  # uncompressed
    app.el('[data-ptab="html"]').click()
    app.el("#pb-paused").click()
    for p in app.d.downloads.iterdir():
        p.unlink()
    app.el("#pb-publish").click()
    end = time.time() + 8
    while time.time() < end and len([p for p in app.d.downloads.iterdir() if p.suffix in (".swf", ".html")]) < 2:
        time.sleep(0.1)
    time.sleep(0.3)
    swf = (app.d.downloads / "mi-peli.swf").read_bytes()
    assert swf[:3] == b"FWS", "sin comprimir"
    html = (app.d.downloads / "mi-peli.html").read_text(encoding="utf-8")
    assert 'data="mi-peli.swf"' in html and "ruffle" in html and "<param name=\"play\" value=\"false\">" in html
    pub = app.js("return JSON.parse(window.omf.ed.publish_settings())")
    assert pub["name"] == "mi-peli" and pub["compress"] is False and pub["paused"] is True
    # It is saved with the document.
    data = app.js("return window.omf.ed.to_json()")
    assert '"publish"' in data and "mi-peli" in data


def test_swf_comprimido_por_defecto_y_vista_previa_html(app):
    app.rect((100, 100), (200, 200))
    raw = bytes(app.js("return Array.from(window.omf.ed.export_swf())"))
    assert raw[:3] == b"CWS" and swf_plain(raw)[:3] == b"FWS"
    handles = set(app.d.window_handles)
    app.menu("Archivo", "Vista previa de publicación", "HTML")
    new = app.wait(lambda: set(app.d.window_handles) - handles, msg="ventana de vista previa")
    main = app.d.current_window_handle
    app.d.switch_to.window(new.pop())
    html = app.d.execute_script("return document.documentElement.outerHTML")
    assert "omf-movie" in html and "RufflePlayer" in html
    app.d.close()
    app.d.switch_to.window(main)


# ───────── History and commands ─────────

def open_history(app):
    app.menu("Ventana", "Otros paneles", "Historial")
    assert app.el('[data-panel="history"]').is_displayed()


def test_historial_repetir_y_guardar_comando(app):
    open_history(app)
    app.object_mode(True)
    app.rect((50, 50), (100, 100))
    rows = app.els("#hist-list [data-step]")
    assert rows and rows[-1].text == "Rectángulo"
    app.tool("select")
    app.click((75, 75))
    app.combo(Keys.CONTROL, "d")  # duplicate
    assert app.els("#hist-list [data-step]")[-1].text == "Duplicar"
    n = len(shapes(app))
    # Replay the rectangle step.
    app.els("#hist-list [data-step]")[0].click()
    app.el("#hist-replay").click()
    assert len(shapes(app)) == n + 1
    # Save the first two steps as a command and run it from the menu.
    app.els("#hist-list [data-step]")[0].click()
    app.click_xy(*(lambda r: (r["x"] + 20, r["y"] + 8))(app.els("#hist-list [data-step]")[1].rect), mods=(Keys.SHIFT,))
    app.el("#hist-save").click()
    app.set_input("#cmd-name", "Rect y copia")
    app.el("#cmd-ok").click()
    n = len(shapes(app))
    app.menu("Comandos", "Rect y copia")
    assert len(shapes(app)) == n + 2
    # Manage: delete.
    app.menu("Comandos", "Administrar comandos guardados…")
    app.el('[data-cmdname="Rect y copia"]').click()
    app.el("#cm-delete").click()
    assert not app.els('[data-cmdname="Rect y copia"]')
    app.el("[data-close]").click()


def test_ejecutar_comando_desde_archivo(app, tmp_path):
    f = tmp_path / "capa.jsfl"
    f.write_text("ed.add_layer(); ed.add_layer(); omf.toast('hecho');", encoding="utf-8")
    n = len(app.state()["layers"])
    app.menu("Comandos", "Ejecutar comando…")
    app.wait(lambda: app.els("#run-command-input"))[0].send_keys(str(f))
    app.wait(lambda: len(app.state()["layers"]) == n + 2, msg="el script añade dos capas")


# ───────── Spelling ─────────

def write_text(app, at, text):
    app.tool("text")
    app.click(at)
    app.wait(lambda: app.els(".text-edit"))
    ActionChains(app.d).send_keys(text).perform()
    app.key(Keys.ESCAPE)


def test_revisar_ortografia(app):
    write_text(app, (100, 100), "Hola munod bonito")
    app.menu("Texto", "Revisar ortografía…")
    app.wait(lambda: app.els("#sc-word"), 15, "diálogo de ortografía")
    assert app.el("#sc-word").text == "munod"
    sug = [e.text for e in app.els("#sc-sug [data-sug]")]
    assert "mundo" in sug
    next(e for e in app.els("#sc-sug [data-sug]") if e.text == "mundo").click()
    app.el("#sc-change").click()
    app.wait(lambda: not app.modal_open(), msg="no quedan errores")
    txt = [s for s in app.doc()["layers"][0]["keys"]["0"]["shapes"]][0]["geom"]["text"]
    assert txt == "Hola mundo bonito"
    app.combo(Keys.CONTROL, "z")
    assert app.doc()["layers"][0]["keys"]["0"]["shapes"][0]["geom"]["text"] == "Hola munod bonito"


def test_diccionario_personal_y_configuracion(app):
    write_text(app, (100, 100), "Flashero total")
    app.menu("Texto", "Configuración de revisión ortográfica…")
    app.el("#sp-personal").send_keys("Flashero")
    app.el("#sp-ok").click()
    app.menu("Texto", "Revisar ortografía…")
    app.wait(lambda: "completa" in app.toast(), 15, "sin errores gracias al diccionario personal")
