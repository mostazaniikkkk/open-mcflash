"""Timeline: F5/F6/F7, tweening, easing, layers, playback and "Test Movie"."""

import time

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.support.ui import Select

from conftest import Keys


def keys_of(app, layer=0):
    return app.state()["layers"][layer]["keys"]


def test_fotograma_y_claves_con_f5_f6_f7(app):
    app.rect((100, 100), (200, 200))
    app.click_xy(*app.cell(0, 4))
    assert app.state()["frame"] == 4
    app.key(Keys.F5)
    assert app.state()["total"] == 5
    app.key(Keys.F6)
    k = keys_of(app)
    assert [x["f"] for x in k] == [0, 4] and not k[1]["empty"]
    app.click_xy(*app.cell(0, 8))
    app.key(Keys.F7)
    k = keys_of(app)
    assert k[-1]["f"] == 8 and k[-1]["empty"]
    app.key(Keys.SHIFT + Keys.F6)
    app.combo(Keys.SHIFT, Keys.F6)
    assert [x["f"] for x in keys_of(app)] == [0, 4]
    app.combo(Keys.SHIFT, Keys.F5)
    assert app.state()["total"] == 8


def test_interpolacion_de_movimiento_con_menu_contextual(app):
    app.object_mode(True)
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 10))
    app.key(Keys.F6)
    app.tool("select")
    app.click((125, 125))
    app.drag((125, 125), (325, 125))
    app.click_xy(*app.cell(0, 2), button="right")
    app._walk(["Crear interpolación de movimiento"])
    assert keys_of(app)[0]["tween"]
    mid = app.frame(0, 5)[0]
    assert abs(mid["x0"] - 200) < 1, mid["x0"]
    # Easing from the Property inspector.
    app.click_xy(*app.cell(0, 3))
    app.set_input("#pp-ease", 100)
    assert app.frame(0, 5)[0]["x0"] > 230


def test_editar_dentro_de_la_interpolacion_crea_clave(app):
    app.object_mode(True)
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 10))
    app.key(Keys.F6)
    app.click_xy(*app.cell(0, 0), button="right")
    app._walk(["Crear interpolación de movimiento"])
    app.click_xy(*app.cell(0, 5))
    app.tool("select")
    app.click((125, 125))
    assert [x["f"] for x in keys_of(app)] == [0, 10], "un clic no crea clave"
    app.drag((125, 125), (125, 300))
    assert [x["f"] for x in keys_of(app)] == [0, 5, 10]


def test_capas(app):
    app.el('.tl-foot [data-cmd="addLayer"]').click()
    layers = app.state()["layers"]
    assert len(layers) == 2 and app.state()["layer"] == 0
    # Rename by double-clicking.
    name = app.els("#tl-layers .layer .lname")[0]
    ActionChains(app.d).double_click(name).perform()
    inp = app.wait(lambda: app.el("#tl-layers input"))
    inp.send_keys(Keys.CONTROL, "a")
    inp.send_keys("Fondo", Keys.ENTER)
    assert app.state()["layers"][0]["name"] == "Fondo"
    # Eye, lock and outline.
    app.el('[data-eye="0"]').click()
    app.el('[data-lock="1"]').click()
    app.el('[data-outline="1"]').click()
    l = app.state()["layers"]
    assert not l[0]["visible"] and l[1]["locked"] and l[1]["outline"]
    # Drawing on a hidden layer shows a warning.
    app.rect((100, 100), (200, 200))
    assert "oculta" in app.wait(app.toast)
    # Reorder by dragging.
    rows = app.els("#tl-layers .layer")
    ActionChains(app.d).click_and_hold(rows[0]).move_by_offset(0, 12).move_by_offset(0, 14).release().perform()
    assert app.state()["layers"][1]["name"] == "Fondo"
    # Delete.
    app.el('.tl-foot [data-cmd="deleteLayer"]').click()
    assert len(app.state()["layers"]) == 1


def test_papel_cebolla(app):
    app.el("#onionbtn").click()
    assert app.state()["onion"]


def test_reproducir_y_paso_a_paso(app):
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 20))
    app.key(Keys.F5)
    app.click_xy(*app.cell(0, 0))
    app.key(Keys.ENTER)
    time.sleep(0.6)
    app.key(Keys.ENTER)
    f = app.state()["frame"]
    assert f > 2, f
    app.key(".")
    assert app.state()["frame"] == f + 1
    app.key(",")
    app.key(",")
    assert app.state()["frame"] == f - 1
    assert app.el("#ro-frame").text == str(f)


def test_probar_pelicula(app):
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 11))
    app.key(Keys.F5)
    # The last frame reports with a trace: the movie advances in Ruffle.
    app.click_xy(*app.cell(0, 11))
    app.key(Keys.F6)
    app.js("window.omf.ed.set_actions('trace(\"final\")'); window.omf.refresh()")
    app.test_movie()
    assert app.modal_open() and app.el("#tm-player").is_displayed()
    app.wait(lambda: "final" in app.output(), 10, "llega al último fotograma")
    app.key(Keys.ESCAPE)
    assert not app.modal_open()


def test_un_solo_clic_detiene_la_reproduccion(app):
    """The play button must not be rebuilt on every frame: a slow click
    (held down for several frames) has to pause on the first try."""
    from selenium.webdriver.common.actions.action_builder import ActionBuilder

    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 59))
    app.key(Keys.F5)
    app.js("window.omf.ed.set_document(550, 400, '#ffffff', 60); window.omf.refresh()")
    btn = app.el("#playbtn")
    btn.click()
    time.sleep(0.3)
    assert app.js("return document.querySelector('#playbtn').classList.contains('on')")
    r = btn.rect
    ab = ActionBuilder(app.d)
    ab.pointer_action.move_to_location(int(r["x"] + r["width"] / 2), int(r["y"] + r["height"] / 2))
    ab.pointer_action.pointer_down()
    ab.pointer_action.pause(0.25)
    ab.pointer_action.pointer_up()
    ab.perform()
    time.sleep(0.2)
    f = app.state()["frame"]
    time.sleep(0.3)
    assert app.state()["frame"] == f, "un clic basta para pausar"
    assert not app.js("return document.querySelector('#playbtn').classList.contains('on')")
