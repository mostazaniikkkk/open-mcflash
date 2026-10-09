"""View (zoom, scrolling, Hand, Zoom tool) and snapping (snap to objects, grid, pixels)."""

from selenium.webdriver.common.action_chains import ActionChains

from conftest import Keys


def view_origin(app):
    return app.js("return window.omf.ed.doc_coords(0, 0)")


def test_zoom_con_ctrl_rueda_y_lupa(app):
    z0 = app.state()["zoom"]
    app.wheel((275, 200), -300, mods=(Keys.CONTROL,))
    assert app.state()["zoom"] > z0
    app.tool("zoom")
    z1 = app.state()["zoom"]
    app.click((275, 200))
    assert abs(app.state()["zoom"] - z1 * 2) < 1
    app.click((275, 200), mods=(Keys.ALT,))
    assert abs(app.state()["zoom"] - z1) < 1
    app.combo(Keys.CONTROL, "1")
    assert abs(app.state()["zoom"] - 100) < 0.5


def test_desplazar_con_rueda_mano_y_espacio(app):
    o0 = view_origin(app)
    app.wheel((275, 200), 100)
    o1 = view_origin(app)
    assert o1[1] > o0[1]
    app.tool("hand")
    app.drag((275, 200), (225, 200))
    o2 = view_origin(app)
    assert o2[0] > o1[0]
    app.tool("rect")
    app.drag((275, 200), (225, 200), mods=(Keys.SPACE,))
    assert app.frame() == [], "con Espacio se desplaza, no se dibuja"
    assert view_origin(app)[0] > o2[0]


def test_cuadricula_y_ajustar_a_cuadricula(app):
    app.combo(Keys.CONTROL, "'")
    assert app.state()["showGrid"]
    app.menu("Ver", "Cuadrícula", "Editar cuadrícula…")
    app.set_input("#gd-size", 20)
    app.el("#gd-snap").click()
    app.el("#gd-ok").click()
    s = app.state()
    assert s["gridSize"] == 20 and s["snapGrid"]
    app.rect((103, 97), (196, 205))
    f = app.fills()[0]
    assert (f["x0"], f["y0"], f["x1"], f["y1"]) == (100, 100, 200, 200)


def test_iman_ajusta_a_esquinas(app):
    app.rect((100, 100), (200, 200))
    app.el('#tb-opts [data-cmd="snapObjects"]').click()
    assert app.state()["snapObjects"]
    app.rect((205, 196), (300, 300))
    f = [s for s in app.fills() if s["x1"] > 250][0]
    assert f["x0"] == 200 and f["y0"] == 200


def test_ajustar_a_pixeles(app):
    app.menu("Ver", "Ajuste", "Ajustar a píxeles")
    assert app.state()["snapPixels"]
    app.js("window.omf.ed.set_zoom(400); window.omf.refresh()")
    p = app.js("const r=document.querySelector('#stage'); return window.omf.ed.doc_coords(r.width/2, r.height/2)")
    app.rect((p[0] - 10.3, p[1] - 10.3), (p[0] + 10.4, p[1] + 10.2))
    f = app.fills()[0]
    assert all(abs(v - round(v)) < 1e-3 for v in (f["x0"], f["y0"], f["x1"], f["y1"]))
