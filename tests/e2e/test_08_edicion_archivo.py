"""Edit (undo, clipboard) and File (save, open, export, autosave, dialogs)."""

import json
import time

from selenium.webdriver.support.ui import Select

from conftest import Keys, swf_plain


def wait_download(app, suffix, timeout=8):
    end = time.time() + timeout
    while time.time() < end:
        files = [p for p in app.d.downloads.iterdir() if p.name.endswith(suffix)]
        if files:
            f = max(files, key=lambda p: p.stat().st_mtime)
            time.sleep(0.2)
            return f
        time.sleep(0.1)
    raise AssertionError(f"no se descargó ningún {suffix}")


def clean_downloads(app):
    for p in app.d.downloads.iterdir():
        p.unlink()


def test_deshacer_y_rehacer(app):
    app.rect((100, 100), (200, 200))
    n = len(app.frame())
    app.combo(Keys.CONTROL, "z")
    assert app.frame() == []
    app.combo(Keys.CONTROL, "y")
    assert len(app.frame()) == n
    app.menu("Edición", "Deshacer")
    assert app.frame() == []


def test_copiar_pegar_en_el_centro_e_in_situ(app):
    app.object_mode(True)
    app.rect((100, 100), (150, 150))
    app.tool("select")
    app.click((125, 125))
    app.combo(Keys.CONTROL, "c")
    app.combo(Keys.CONTROL, "v")
    s = app.state()["sel"]
    center = app.js("const r=document.querySelector('#stage'); return window.omf.ed.doc_coords(r.width/2, r.height/2)")
    assert abs(s["x"] + s["w"] / 2 - center[0]) < 1 and abs(s["y"] + s["h"] / 2 - center[1]) < 1
    app.combo(Keys.CONTROL, Keys.SHIFT, "v")
    xs = sorted(round(x["x0"]) for x in app.frame())
    assert xs.count(100) == 2, xs


def test_duplicar_cortar_y_seleccionar_todo(app):
    app.object_mode(True)
    app.rect((100, 100), (150, 150))
    app.tool("select")
    app.click((125, 125))
    app.combo(Keys.CONTROL, "d")
    assert [round(s["x0"]) for s in app.frame()] == [100, 110]
    app.combo(Keys.CONTROL, "x")
    assert len(app.frame()) == 1
    app.combo(Keys.CONTROL, "a")
    assert app.state()["sel"]["count"] == 1
    app.combo(Keys.CONTROL, Keys.SHIFT, "a")
    assert app.state()["sel"] is None


def test_guardar_y_abrir(app):
    clean_downloads(app)
    app.rect((100, 100), (200, 200))
    app.combo(Keys.CONTROL, "s")
    f = wait_download(app, ".omf")
    data = json.loads(f.read_text(encoding="utf-8"))
    assert data["format"] == "omf" and len(data["document"]["layers"][0]["keys"]["0"]["shapes"]) == 5
    app.combo(Keys.CONTROL, "z")
    assert app.frame() == []
    app.el("#fileopen").send_keys(str(f))
    app.wait(lambda: len(app.frame()) == 5)
    assert "Abierto" in app.wait(app.toast)


def test_exportar_swf_y_png(app):
    clean_downloads(app)
    app.rect((100, 100), (200, 200))
    app.menu("Archivo", "Exportar", "Exportar película (SWF)…")
    raw = wait_download(app, ".swf").read_bytes()
    assert raw[:3] == b"CWS" and raw[3] == 8, "comprimido, como publica Flash 8"
    swf = swf_plain(raw)
    assert int.from_bytes(swf[4:8], "little") == len(swf)
    app.menu("Archivo", "Exportar", "Exportar imagen (PNG)…")
    png = wait_download(app, ".png").read_bytes()
    assert png[:8] == b"\x89PNG\r\n\x1a\n"


def test_autoguardado(app):
    app.rect((100, 100), (200, 200))
    time.sleep(1.2)
    app.reload()
    assert len(app.frame()) == 5
    assert "recuperado" in app.wait(app.toast)


def test_nuevo_documento(app):
    app.rect((100, 100), (200, 200))
    app.menu("Archivo", "Nuevo…")
    Select(app.el("#nd-pre")).select_by_value("1280x720")
    app.el("#nd-ok").click()
    s = app.state()
    assert s["width"] == 1280 and s["height"] == 720 and app.frame() == []
    assert "Sin título-2" in app.el("#tabname").text


def test_propiedades_del_documento(app):
    app.combo(Keys.CONTROL, "j")
    assert app.modal_open()
    app.set_input("#dd-w", 640)
    app.set_input("#dd-fps", 24)
    app.el("#dd-hidden").click()
    app.el("#dd-ok").click()
    s = app.state()
    assert s["width"] == 640 and s["fps"] == 24 and s["exportHidden"] is False
