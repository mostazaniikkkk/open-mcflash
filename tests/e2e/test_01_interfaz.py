"""Loading, Flash 8-style layout, menus, panels and dialogs."""

import time

from conftest import By, Keys

TOOLS = ["select", "subselect", "transform", "filltransform", "line", "lasso", "pen", "text", "oval", "rect",
         "pencil", "brush", "ink", "bucket", "eyedropper", "eraser", "hand", "zoom"]
# Since block 4 all of Flash 8's Tools panel tools are present.
ENABLED = set(TOOLS)


def test_carga_sin_errores(app):
    s = app.state()
    assert s["width"] == 550 and s["height"] == 400 and s["fps"] == 12
    assert "Open McFlash" in app.d.title
    assert len(s["layers"]) == 1 and s["layers"][0]["name"] == "Capa 1"


def test_caja_de_herramientas_en_orden_de_flash(app):
    tools = [e.get_attribute("data-tool") for e in app.els("#toolbar .tool")]
    assert tools == TOOLS
    for e in app.els("#toolbar .tool"):
        disabled = "disabled" in e.get_attribute("class")
        assert disabled == (e.get_attribute("data-tool") not in ENABLED)
    labels = [e.text for e in app.els("#toolbar .tb-label")]
    assert [l.lower() for l in labels] == ["herram.", "ver", "colores", "opciones"]


def test_atajos_de_herramientas(app):
    keys = {"v": "select", "a": "subselect", "q": "transform", "n": "line", "p": "pen", "o": "oval", "r": "rect",
            "y": "pencil", "b": "brush", "s": "ink", "k": "bucket", "i": "eyedropper", "e": "eraser", "h": "hand",
            "z": "zoom", "t": "text", "l": "lasso", "f": "filltransform"}
    for k, tool in keys.items():
        app.key(k)
        assert app.state()["tool"] == tool, k


def test_todas_las_herramientas_se_activan(app):
    for t in TOOLS:
        app.el(f'#toolbar .tool[data-tool="{t}"]').click()
        assert app.state()["tool"] == t, t


def test_menus_de_flash(app):
    names = [b.text for b in app.els("#menus .menu-btn")]
    assert names == ["Archivo", "Edición", "Ver", "Insertar", "Modificar", "Texto", "Comandos", "Control", "Ventana", "Ayuda"]
    app.menu("Modificar", "Forma")
    items = app.menu_items()
    assert "Optimizar…" in items and "Expandir relleno…" in items
    app.key(Keys.ESCAPE)
    assert not [e for e in app.els(".menu-dropdown") if e.is_displayed()]


def test_dialogos_de_ayuda(app):
    app.menu("Ayuda", "Métodos abreviados de teclado")
    assert app.modal_open() and "F6" in app.el("#modal-body").text
    app.key(Keys.ESCAPE)
    assert not app.modal_open()
    app.menu("Ayuda", "Acerca de Open McFlash")
    assert "Open McFlash" in app.el("#modal-body").text
    app.el("#modal [data-close]").click()
    assert not app.modal_open()


def test_paneles_plegables_y_ventana(app):
    head = app.el('[data-panel="mixer"] .panel-head')
    head.click()
    assert "collapsed" in app.el('[data-panel="mixer"]').get_attribute("class")
    head.click()
    assert "collapsed" not in app.el('[data-panel="mixer"]').get_attribute("class")
    app.menu("Ventana", "Alinear")
    assert not app.el('[data-panel="align"]').is_displayed()
    app.menu("Ventana", "Alinear")
    assert app.el('[data-panel="align"]').is_displayed()


def test_linea_de_tiempo_ocultar_y_redimensionar(app):
    app.el("#tl-toggle").click()
    assert not app.el("#timeline").is_displayed()
    app.combo(Keys.CONTROL, Keys.ALT, "t")
    assert app.el("#timeline").is_displayed()
    h0 = app.el("#timeline").size["height"]
    sp = app.el("#tl-splitter")
    from selenium.webdriver.common.action_chains import ActionChains

    ActionChains(app.d).click_and_hold(sp).move_by_offset(0, 60).release().perform()
    assert app.el("#timeline").size["height"] > h0 + 40


def test_linea_de_tiempo_clasica(app):
    app.menu("Ver", "Línea de tiempo clásica (colores de Flash 8)")
    assert app.js("return JSON.parse(localStorage.getItem('omf.prefs')).classic") is True


def test_zoom_desde_la_barra(app):
    sel = app.el("#zoomsel")
    from selenium.webdriver.support.ui import Select

    Select(sel).select_by_value("200")
    assert abs(app.state()["zoom"] - 200) < 1
    Select(sel).select_by_value("fit")
    assert app.state()["zoom"] < 200


def test_barra_de_estado_muestra_coordenadas(app):
    app.hover((100, 50))
    txt = app.el("#st-coords").text
    assert "100.0" in txt and "50.0" in txt
