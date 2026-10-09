"""Selection, move, nudge, Free Transform, transformation point, properties, Align and Arrange."""

import math

from conftest import Keys


def obj_rect(app, a=(100, 100), b=(200, 200)):
    app.object_mode(True)
    app.rect(a, b)
    app.tool("select")


def test_mover_arrastrando_y_con_flechas(app):
    obj_rect(app)
    app.click((150, 150))
    app.drag((150, 150), (200, 170))
    s = app.frame()[0]
    assert abs(s["x0"] - 150) < 0.6 and abs(s["y0"] - 120) < 0.6
    app.key(Keys.ARROW_RIGHT)
    app.combo(Keys.SHIFT, Keys.ARROW_DOWN)
    s = app.frame()[0]
    assert abs(s["x0"] - 151) < 0.6 and abs(s["y0"] - 130) < 0.6


def test_mayus_suma_y_seleccion_por_area_exige_contener(app):
    obj_rect(app)
    app.rect((300, 100), (400, 200))
    app.tool("select")
    app.drag((50, 50), (150, 250))
    assert app.state()["sel"] is None, "a medias no se selecciona un objeto"
    app.drag((50, 50), (250, 250))
    assert app.state()["sel"]["count"] == 1
    app.click((350, 150), mods=(Keys.SHIFT,))
    assert app.state()["sel"]["count"] == 2


def test_borrar_con_supr(app):
    obj_rect(app)
    app.click((150, 150))
    app.key(Keys.DELETE)
    assert app.frame() == []


def test_transformacion_libre_rotar(app):
    obj_rect(app)
    app.tool("transform")
    app.click((150, 150))
    app.drag((210, 90), (210, 210), steps=12)
    s = app.frame()[0]
    assert abs(abs(s["rot"]) - math.pi / 2) < 0.06, s["rot"]


def test_transformacion_libre_escalar_y_caja_girada(app):
    obj_rect(app)
    app.tool("transform")
    app.click((150, 150))
    app.drag((200, 150), (250, 150))
    s = app.frame()[0]
    assert abs((s["x1"] - s["x0"]) - 150) < 1
    # After rotating, the handle on the local right side still scales along the object.
    app.menu("Modificar", "Transformar", "Girar 90° en sentido horario")
    s = app.frame()[0]
    assert abs(abs(s["rot"]) - math.pi / 2) < 0.01


def test_mover_el_punto_de_transformacion(app):
    obj_rect(app)
    app.tool("transform")
    app.click((150, 150))
    assert app.cursor_at((150, 150)).startswith("url(")
    app.drag((150, 150), (100, 100))
    app.menu("Modificar", "Transformar", "Girar 90° en sentido horario")
    s = app.frame()[0]
    assert abs(s["x0"] - 0) < 0.6 and abs(s["x1"] - 100) < 0.6


def test_voltear(app):
    app.object_mode(True)
    app.tool("line")
    app.drag((100, 100), (200, 150))
    app.tool("select")
    app.click((150, 125))
    app.menu("Modificar", "Transformar", "Voltear horizontalmente")
    assert app.frame()[0]["sx"] * app.frame()[0]["sy"] < 0 or abs(abs(app.frame()[0]["rot"]) - math.pi) < 0.01


def test_panel_propiedades_cambia_posicion_y_tamano(app):
    obj_rect(app)
    app.click((150, 150))
    app.set_input("#pp-x", 30)
    app.set_input("#pp-w", 120)
    s = app.frame()[0]
    assert abs(s["x0"] - 30) < 0.6 and abs((s["x1"] - s["x0"]) - 120) < 0.6


def test_panel_alinear(app):
    obj_rect(app)
    app.rect((300, 140), (400, 240))
    app.tool("select")
    app.combo(Keys.CONTROL, "a")
    app.el('[data-align="left"]').click()
    xs = sorted(s["x0"] for s in app.frame())
    assert abs(xs[0] - xs[1]) < 0.6
    app.el("#al-stage").click()
    app.el('[data-align="right"]').click()
    assert all(abs(s["x1"] - 550) < 0.6 for s in app.frame())


def test_organizar(app):
    obj_rect(app)
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.rect((150, 150), (250, 250))
    app.tool("select")
    bottom = app.frame()[0]["id"]
    app.click((110, 110))
    app.combo(Keys.CONTROL, Keys.SHIFT, Keys.ARROW_UP)
    assert app.frame()[-1]["id"] == bottom
    app.combo(Keys.CONTROL, Keys.SHIFT, Keys.ARROW_DOWN)
    assert app.frame()[0]["id"] == bottom
