"""Groups, drawing objects, Break Apart and Modify > Shape."""

import time

from conftest import Keys


def test_agrupar_entrar_salir_y_desagrupar(app):
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.combo(Keys.CONTROL, "a")
    app.combo(Keys.CONTROL, "g")
    shapes = app.frame()
    assert len(shapes) == 1 and shapes[0]["group"] and shapes[0]["children"] == 5
    assert app.state()["sel"]["kind"] == "Grupo"
    app.dblclick((150, 150))
    assert app.state()["scopes"] == ["Grupo"]
    assert "Grupo" in app.el("#crumbs").text
    app.rect((300, 300), (350, 350))
    app.el("#scope-back").click()
    assert app.state()["scopes"] == []
    assert app.frame()[0]["children"] == 10
    app.tool("select")
    app.click((150, 150))
    app.combo(Keys.CONTROL, Keys.SHIFT, "g")
    assert len(app.frame()) == 10


def test_ruta_y_editar_todo(app):
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.combo(Keys.CONTROL, "a")
    app.combo(Keys.CONTROL, "g")
    app.menu("Edición", "Editar selección")
    assert app.state()["scopes"] == ["Grupo"]
    app.el('#crumbs .crumb[data-depth="0"]').click()
    assert app.state()["scopes"] == []
    app.dblclick((150, 150))
    app.menu("Edición", "Editar todo")
    assert app.state()["scopes"] == []


def test_doble_clic_fuera_sale_y_deshacer_vuelve_a_entrar(app):
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.combo(Keys.CONTROL, "a")
    app.combo(Keys.CONTROL, "g")
    app.dblclick((150, 150))
    app.dblclick((400, 350))
    assert app.state()["scopes"] == []
    app.combo(Keys.CONTROL, "z")
    assert app.state()["scopes"] == ["Grupo"]


def test_lo_de_fuera_se_atenua_al_editar(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    app.rect((300, 100), (400, 200))
    app.tool("select")
    app.click((150, 150))
    app.combo(Keys.CONTROL, "g")
    before = app.pixel(350, 150)
    app.dblclick((150, 150))
    assert app.pixel(350, 150) != before


def test_objeto_de_dibujo_doble_clic_y_separar(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.dblclick((150, 150))
    assert app.state()["scopes"] == ["Objeto de dibujo"]
    app.key(Keys.ESCAPE)
    app.el("#scope-back").click()
    s = app.frame()
    assert len(s) == 1 and s[0]["kind"] == "Objeto de dibujo"
    app.click((150, 150))
    app.combo(Keys.CONTROL, "b")
    app.key(Keys.ESCAPE)
    assert len(app.frame()) == 5 and all(x["raw"] for x in app.frame())


def test_suavizar_enderezar_y_optimizar(app):
    app.tool("oval")
    app.drag((100, 100), (220, 200))
    app.tool("select")
    app.dblclick((160, 150))
    app.el('#tb-opts [data-cmd="straighten"]').click()
    assert all(not s["closed"] or s["nodes"] >= 3 for s in app.frame())
    app.el('#tb-opts [data-cmd="smooth"]').click()
    app.menu("Modificar", "Forma", "Optimizar…")
    assert app.modal_open()
    app.set_input("#op-v", 80)
    app.el("#op-ok").click()
    assert "optimizadas" in app.wait(app.toast)


def test_convertir_lineas_en_rellenos(app):
    app.tool("line")
    app.drag((100, 100), (300, 100))
    app.tool("select")
    app.click((200, 100))
    app.menu("Modificar", "Forma", "Convertir líneas en rellenos")
    assert not app.strokes() and len(app.fills()) == 1


def test_expandir_y_suavizar_bordes(app):
    app.js("window.omf.ed.set_stroke('#000000', 255, false)")
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.click((150, 150))
    app.menu("Modificar", "Forma", "Expandir relleno…")
    app.set_input("#ex-d", 10)
    app.el("#ex-ok").click()
    f = app.fills()[0]
    assert abs(f["x0"] - 90) < 1 and abs(f["x1"] - 210) < 1
    app.menu("Modificar", "Forma", "Suavizar bordes de relleno…")
    app.set_input("#sf-s", 3)
    app.el("#sf-ok").click()
    alphas = sorted({s["fillAlpha"] for s in app.fills()})
    assert len(alphas) == 4 and alphas[-1] == 255
