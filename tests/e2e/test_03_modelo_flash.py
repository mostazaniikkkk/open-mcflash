"""Flash's Merge Drawing model: merging, cutting, floating shapes, double-click, bending and corners."""

from conftest import Keys

BLUE = [51, 102, 204]
RED = [255, 0, 0]


def no_stroke(app):
    app.js("window.omf.ed.set_stroke('#000000', 255, false); window.omf.refresh()")


def test_mismo_color_se_une(app):
    no_stroke(app)
    app.rect((100, 100), (200, 200))
    app.rect((150, 150), (250, 250))
    f = app.fills()
    assert len(f) == 1 and abs(f[0]["area"] - 17500) < 40


def test_otro_color_recorta(app):
    no_stroke(app)
    app.rect((100, 100), (200, 200))
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.rect((150, 150), (250, 250))
    blue = [s for s in app.fills() if s["fill"] == "#3366cc"][0]
    assert abs(blue["area"] - 7500) < 40
    assert app.pixel(175, 175) == RED and app.pixel(120, 120) == BLUE


def test_relleno_nuevo_borra_las_lineas_que_tapa(app):
    app.tool("line")
    app.drag((50, 150), (350, 150))
    no_stroke(app)
    app.rect((100, 100), (200, 200))
    lines = app.strokes()
    assert len(lines) == 2, "la línea queda partida a ambos lados del relleno"
    assert app.pixel(150, 150) == BLUE


def test_linea_divide_relleno_y_se_parte_en_cruces(app):
    app.rect((100, 100), (200, 200))
    app.tool("line")
    app.drag((150, 50), (150, 250))
    assert len(app.fills()) == 2
    # 4 sides (top and bottom split → 6) + the line split in 3.
    assert len(app.strokes()) == 9


def test_clic_selecciona_pieza_y_doble_clic_lo_conectado(app):
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.click((150, 150))
    s = app.state()["sel"]
    assert s["count"] == 1 and s["kind"] == "Forma"
    app.dblclick((150, 150))
    assert app.state()["sel"]["count"] == 5
    app.key(Keys.ESCAPE)
    app.dblclick((150, 100))
    assert app.state()["sel"]["count"] == 4


def test_trama_de_seleccion(app):
    no_stroke(app)
    app.rect((100, 100), (200, 200))
    before = [app.pixel(100 + i, 150) for i in range(8)]
    app.tool("select")
    app.click((150, 150))
    after = [app.pixel(100 + i, 150) for i in range(8)]
    assert before != after and any(p != BLUE for p in after), "la trama de puntos marca la selección"


def test_flotante_se_fusiona_al_deseleccionar(app):
    no_stroke(app)
    app.rect((100, 100), (200, 200))
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.rect((300, 100), (400, 200))
    app.tool("select")
    app.click((350, 150))
    app.drag((350, 150), (200, 150))
    assert app.state()["floating"] == 1
    assert abs([s for s in app.fills() if s["fill"] == "#3366cc"][0]["area"] - 10000) < 40
    app.key(Keys.ESCAPE)
    assert app.state()["floating"] == 0
    assert abs([s for s in app.fills() if s["fill"] == "#3366cc"][0]["area"] - 5000) < 40


def test_rectangulo_de_seleccion_corta(app):
    no_stroke(app)
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.drag((50, 50), (150, 250))
    assert len(app.fills()) == 2
    assert app.state()["sel"]["count"] == 1 and abs(app.state()["sel"]["w"] - 50) < 1
    app.drag((125, 150), (125, 300))
    app.key(Keys.ESCAPE)  # without the selection hatch
    assert app.pixel(125, 150) != BLUE and app.pixel(125, 300) == BLUE


def test_doblar_borde(app):
    app.rect((100, 100), (200, 200))
    app.tool("select")
    assert app.cursor_at((150, 100)).startswith("url(")  # curve cursor
    app.drag((150, 100), (150, 60))
    f = app.fills()[0]
    top_line = min(s["y0"] for s in app.strokes())
    assert f["y0"] < 80 and abs(f["y0"] - top_line) < 1


def test_mover_esquina(app):
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.drag((200, 200), (240, 230))
    f = app.fills()[0]
    assert abs(f["x1"] - 240) < 0.6 and abs(f["y1"] - 230) < 0.6
    assert abs(max(s["x1"] for s in app.strokes()) - 240) < 0.6


def test_objetos_no_se_mezclan(app):
    app.object_mode(True)
    no_stroke(app)
    app.rect((100, 100), (200, 200))
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.rect((150, 150), (250, 250))
    shapes = app.frame()
    assert len(shapes) == 2 and abs(shapes[0]["area"] - 10000) < 40
