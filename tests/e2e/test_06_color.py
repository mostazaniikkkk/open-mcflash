"""Colors: Tools panel color boxes, Flash-style picker, Color Mixer, Swatches and applying to the selection."""

from conftest import Keys


def open_fill_picker(app):
    app.el('[data-well="fill"]').click()
    app.wait(lambda: app.el("#popover").is_displayed())


def test_selector_de_muestras_de_flash(app):
    open_fill_picker(app)
    assert len(app.els("#popover [data-sw]")) == 18 * 13
    app.el('#popover [data-sw="#FF0000"]').click()
    assert app.state()["fill"] == "#ff0000" and not app.el("#popover").is_displayed()


def test_hexadecimal_alfa_y_sin_color(app):
    open_fill_picker(app)
    app.set_input("#pc-alpha", 50)
    hx = app.el("#pc-hex")
    hx.click()
    hx.send_keys(Keys.CONTROL, "a")
    hx.send_keys("#00ff00", Keys.ENTER)
    s = app.state()
    assert s["fill"] == "#00ff00" and abs(s["fillAlpha"] - 128) <= 1
    open_fill_picker(app)
    app.el("#pc-none").click()
    assert app.state()["fillOn"] is False


def test_blanco_y_negro_e_intercambiar(app):
    app.el('[data-cmd="defaultColors"]').click()
    s = app.state()
    assert s["fill"] == "#000000" and s["stroke"] == "#000000"
    app.js("window.omf.ed.set_fill('#ff0000', 255, true); window.omf.refresh()")
    app.el('[data-cmd="swapColors"]').click()
    s = app.state()
    assert s["stroke"] == "#ff0000" and s["fill"] == "#000000"


def test_mezclador_y_muestras_aplican_a_la_seleccion(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.click((150, 150))
    app.el('[data-mix="fill"]').click()
    app.set_input("#mx-hex", "#123456")
    assert app.frame()[0]["fill"] == "#123456"
    app.el('#pb-swatches [data-sw="#FFFF00"]').click()
    assert app.frame()[0]["fill"] == "#ffff00"
    app.el('[data-mix="stroke"]').click()
    app.el('#pb-swatches [data-sw="#FF0000"]').click()
    assert app.frame()[0]["stroke"] == "#ff0000"


def test_grosor_de_trazo_en_propiedades(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.click((150, 150))
    app.set_input("#pp-sw", 6)
    assert app.frame()[0]["strokeWidth"] == 6
