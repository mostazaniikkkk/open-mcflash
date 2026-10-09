"""Block 5 — rulers and guides: View > Rulers, guides dragged from the rulers,
moving them and deleting them by dropping them on the ruler, "Snap to Guides",
lock, hide, Edit Guides, per-symbol guides and text with rulers."""

import json
import time

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.support.ui import Select

from conftest import Keys


def rulers_on(app):
    app.menu("Ver", "Reglas")
    assert app.el("#ruler-h").is_displayed() and app.el("#ruler-v").is_displayed()
    app.exact_view()


def guides(app):
    return json.loads(app.js("return window.omf.ed.guides()"))


def ruler_point(app, which):
    r = app.el(f"#ruler-{which}").rect
    return (r["x"] + r["width"] / 2, r["y"] + 9) if which == "h" else (r["x"] + 9, r["y"] + r["height"] / 2)


def new_guide(app, which, doc_xy):
    app.drag_xy(ruler_point(app, which), app.screen(*doc_xy), steps=8)


def test_reglas_desplazan_el_lienzo_y_se_recuerdan(app):
    vp = app.el("#viewport").rect
    rulers_on(app)
    c = app.el("#stage").rect
    assert abs(c["x"] - vp["x"] - 18) < 0.6 and abs(c["y"] - vp["y"] - 18) < 0.6
    # The rulers draw tick marks (they are not a plain background).
    colors = app.js("""
        const c = document.querySelector('#ruler-h'), g = c.getContext('2d');
        const d = g.getImageData(0, 0, c.width, c.height).data, s = new Set();
        for (let i = 0; i < d.length; i += 4) s.add(d[i]);
        return s.size;""")
    assert colors > 3
    app.reload()
    assert app.el("#ruler-h").is_displayed(), "la preferencia se conserva"
    app.menu("Ver", "Reglas")
    assert not app.el("#ruler-h").is_displayed()


def test_guias_desde_las_reglas_y_ajuste(app):
    rulers_on(app)
    new_guide(app, "h", (300, 100))
    new_guide(app, "v", (200, 300))
    g = guides(app)
    assert len(g) == 2
    h = next(x for x in g if not x["vertical"])
    v = next(x for x in g if x["vertical"])
    assert abs(h["pos"] - 100) < 1 and abs(v["pos"] - 200) < 1
    assert app.state()["guides"]["count"] == 2
    # They show in their color.
    assert app.pixel(400, h["pos"])[2] > 200
    # Snap to Guides: a rectangle starting nearby snaps to both.
    app.object_mode(True)
    app.rect((v["pos"] - 4, h["pos"] - 4), (300, 180))
    s = app.frame(0, 0)[0]
    assert abs(s["x0"] - v["pos"]) < 0.01 and abs(s["y0"] - h["pos"]) < 0.01, s
    # Without snapping it doesn't stick.
    app.menu("Ver", "Ajuste", "Ajustar a guías")
    assert not app.state()["guides"]["snap"]
    app.rect((v["pos"] - 4, 300), (300, 340))
    s = [x for x in app.frame(0, 0) if x["y0"] > 250][0]
    assert abs(s["x0"] - (v["pos"] - 4)) < 0.01


def test_mover_y_borrar_guias(app):
    rulers_on(app)
    new_guide(app, "v", (200, 300))
    app.tool("select")
    assert app.cursor_at((200, 250)) == "ew-resize"
    app.drag((200, 250), (260, 250))
    assert abs(guides(app)[0]["pos"] - 260) < 1
    # Dropping it on the ruler deletes it.
    app.drag_xy(app.screen(260, 250), ruler_point(app, "v"), steps=8)
    assert guides(app) == []
    app.combo(Keys.CONTROL, "z")
    assert len(guides(app)) == 1, "se puede deshacer"
    # A new guide dragged back to the ruler is not created.
    app.drag_xy(ruler_point(app, "h"), ruler_point(app, "h"), steps=4)
    assert len(guides(app)) == 1


def test_bloquear_ocultar_y_editar_guias(app):
    rulers_on(app)
    new_guide(app, "v", (200, 300))
    app.menu("Ver", "Guías", "Bloquear guías")
    assert app.state()["guides"]["lock"]
    app.tool("select")
    app.drag((200, 250), (260, 250))
    assert abs(guides(app)[0]["pos"] - 200) < 1, "bloqueada no se mueve"
    app.menu("Ver", "Guías", "Mostrar guías")
    assert not app.state()["guides"]["show"]
    assert app.pixel(200, 250) == [255, 255, 255], "oculta"
    app.combo(Keys.CONTROL, ";")
    assert app.state()["guides"]["show"]
    # Edit Guides: accuracy, unlock and clear all.
    app.menu("Ver", "Guías", "Editar guías…")
    Select(app.el("#gu-acc")).select_by_value("2")
    app.el("#gu-lock").click()
    app.el("#gu-ok").click()
    g = app.state()["guides"]
    assert g["accuracy"] == 2 and not g["lock"]
    app.menu("Ver", "Guías", "Borrar guías")
    assert guides(app) == []


def test_guias_propias_de_cada_simbolo(app):
    rulers_on(app)
    new_guide(app, "h", (300, 100))
    app.combo(Keys.CONTROL, Keys.F8)
    app.el("#ns-ok").click()
    assert app.state()["symPath"]
    assert guides(app) == [], "el símbolo empieza sin guías"
    new_guide(app, "h", (0, 40))
    assert len(guides(app)) == 1
    app.combo(Keys.CONTROL, "e")
    g = guides(app)
    assert len(g) == 1 and abs(g[0]["pos"] - 100) < 1


def test_texto_alineado_con_las_reglas(app):
    rulers_on(app)
    app.tool("text")
    app.click((100, 100))
    box = app.wait(lambda: app.els(".text-edit"))[0]
    ActionChains(app.d).send_keys("Hola").perform()
    sx, sy = app.screen(100, 100)
    r = box.rect
    assert abs(r["x"] - sx) < 2.5 and abs(r["y"] - sy) < 2.5, (r, sx, sy)
    app.key(Keys.ESCAPE)
