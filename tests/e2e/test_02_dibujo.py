"""Drawing tools: shapes, Pencil, Brush, Eraser, Paint Bucket, Ink Bottle, Eyedropper, Pen and Subselection."""

import math

from conftest import Keys

BLUE = [51, 102, 204]


def close(a, b, tol=1.0):
    return abs(a - b) <= tol


def test_rectangulo_crudo_relleno_y_cuatro_lados(app):
    app.rect((100, 100), (200, 180))
    assert len(app.fills()) == 1 and len(app.strokes()) == 4
    assert all(s["raw"] for s in app.frame())
    f = app.fills()[0]
    assert close(f["x0"], 100) and close(f["x1"], 200) and close(f["y1"], 180)
    assert app.pixel(150, 140) == BLUE


def test_rectangulo_cuadrado_con_mayus_y_desde_centro_con_alt(app):
    app.tool("rect")
    app.drag((100, 100), (200, 150), mods=(Keys.SHIFT,))
    f = app.fills()[0]
    assert close(f["x1"] - f["x0"], f["y1"] - f["y0"])
    app.combo(Keys.CONTROL, "z")
    app.tool("rect")
    app.drag((300, 200), (350, 230), mods=(Keys.ALT,))
    f = app.fills()[0]
    assert close(f["x0"], 250) and close(f["x1"], 350) and close(f["y0"], 170)


def test_radio_de_esquina(app):
    app.tool("rect")
    app.object_mode(True)
    app.set_input("#ob-radius", 20)
    app.drag((100, 100), (200, 200))
    assert app.frame()[0]["kind"] == "Rectángulo"
    # The rounded corner leaves the rectangle's corner tip empty.
    assert app.pixel(101, 101) != BLUE and app.pixel(150, 150) == BLUE


def test_ovalo_y_linea_a_45_grados(app):
    app.tool("oval")
    app.drag((100, 100), (220, 180))
    assert len(app.fills()) == 1 and len(app.strokes()) == 1
    assert app.strokes()[0]["closed"]
    app.tool("line")
    app.drag((300, 300), (400, 380), mods=(Keys.SHIFT,))  # 38.7° → 45°
    line = [s for s in app.strokes() if s["x0"] >= 299][0]
    assert close(line["x1"] - line["x0"], line["y1"] - line["y0"], 1.5), "Mayús restringe a 45°"


def test_dibujo_de_objetos_con_j(app):
    app.tool("rect")
    app.key("j")
    assert app.state()["objectMode"]
    app.drag((100, 100), (200, 200))
    shapes = app.frame()
    assert len(shapes) == 1 and not shapes[0]["raw"] and shapes[0]["kind"] == "Rectángulo"
    # The Options button toggles it too.
    app.el('#tb-opts [data-cmd="toggleObjectMode"]').click()
    assert not app.state()["objectMode"]


def test_lapiz_enderezar_reconoce_rectangulo(app):
    app.tool("pencil")
    assert app.state()["pencilMode"] == "straighten"
    pts = [(100 + i * 10, 100) for i in range(11)] + [(200, 100 + i * 10) for i in range(7)]
    pts += [(200 - i * 10, 160) for i in range(11)] + [(100, 160 - i * 10) for i in range(7)] + [(101, 101)]
    app.stroke(pts)
    strokes = app.strokes()
    assert len(strokes) == 4 and not app.fills(), "rectángulo sin relleno: 4 lados"


def test_lapiz_suavizar_y_tinta_por_menu_de_opciones(app):
    app.tool("pencil")
    app.el('[data-modemenu="pencil"]').click()
    app._walk(["Suavizar"])
    assert app.state()["pencilMode"] == "smooth"
    pts = [(100 + i * 6, 200 + 30 * math.sin(i / 4)) for i in range(40)]
    app.stroke(pts)
    assert len(app.strokes()) >= 1
    app.select_mode("pencil", "ink")
    assert app.state()["pencilMode"] == "ink"


def test_pincel_crea_relleno_y_puntas(app):
    app.tool("brush")
    app.select_mode("brush", "normal")
    app.stroke([(100, 100), (150, 120), (200, 100)])
    f = app.fills()
    assert len(f) == 1 and f[0]["raw"] and not app.strokes()
    from selenium.webdriver.support.ui import Select

    Select(app.el('[data-tipsel="brush"]')).select_by_value("square")
    app.set_input("#ob-brush", 30)
    assert app.state()["brushTip"] == "square" and app.state()["brushSize"] == 30
    app.stroke([(300, 300), (301, 300)])
    sq = [s for s in app.fills() if s["x0"] > 250][0]
    assert close(sq["x1"] - sq["x0"], 31, 2)


def test_pincel_pintar_detras_y_dentro(app):
    app.object_mode(False)
    app.rect((100, 100), (200, 200))
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.tool("brush")
    app.select_mode("brush", "behind")
    app.stroke([(50, 150), (250, 150)])
    assert app.pixel(150, 150) == BLUE, "pintar detrás no tapa el rectángulo"
    assert app.pixel(70, 150) == [255, 0, 0]
    app.select_mode("brush", "inside")
    app.js("window.omf.ed.set_fill('#00ff00', 255, true)")
    app.stroke([(150, 120), (150, 300)])
    assert app.pixel(150, 130) == [0, 255, 0]
    assert app.pixel(150, 260) != [0, 255, 0], "pintar dentro no sale del relleno"


def test_borrador_corta_el_relleno(app):
    app.rect((100, 100), (300, 160))
    app.tool("eraser")
    app.set_input("#ob-eraser", 20)
    app.drag((200, 60), (200, 200))
    assert len(app.fills()) == 2
    assert app.pixel(200, 130) != BLUE


def test_borrador_modos_lineas_y_grifo(app):
    app.rect((100, 100), (300, 160))
    app.tool("eraser")
    app.select_mode("eraser", "lines")
    app.drag((200, 60), (200, 200))
    assert len(app.fills()) == 1, "borrar líneas no toca el relleno"
    assert len(app.strokes()) == 6, "izquierda, derecha y los lados de arriba y abajo partidos en dos"
    app.el("#ob-faucet").click()
    assert app.state()["faucet"]
    app.click((150, 130))
    assert not app.fills(), "el grifo borra el relleno entero"


def test_cubo_area_cerrada_por_lineas_y_huecos(app):
    app.tool("line")
    app.drag((100, 100), (300, 100))
    app.drag((300, 100), (200, 260))
    app.drag((200, 260), (106, 112))  # leaves a ~13 px gap
    app.tool("bucket")
    app.select_mode("bucket", 0)
    app.click((200, 150))
    assert "cerrada" in app.wait(app.toast)
    assert not app.fills()
    app.select_mode("bucket", 3)
    app.click((200, 150))
    assert len(app.fills()) == 1 and app.pixel(200, 150) == BLUE


def test_cubo_cambia_color_y_tinta_anade_contorno(app):
    app.js("window.omf.ed.set_stroke('#000000', 255, false)")
    app.rect((100, 100), (200, 200))
    assert not app.strokes()
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.tool("bucket")
    app.click((150, 150))
    assert app.fills()[0]["fill"] == "#ff0000"
    app.js("window.omf.ed.set_stroke('#000000', 255, true); window.omf.refresh()")
    app.tool("ink")
    app.click((150, 150))
    assert len(app.strokes()) == 4


def test_cuentagotas(app):
    app.js("window.omf.ed.set_fill('#ff8800', 255, true)")
    app.rect((100, 100), (200, 200))
    app.js("window.omf.ed.set_fill('#000000', 255, true)")
    app.tool("eyedropper")
    app.click((150, 150))
    assert app.state()["fill"] == "#ff8800"


def test_pluma_curva_cerrada(app):
    app.tool("pen")
    app.object_mode(True)
    app.click((100, 100))
    app.drag((200, 100), (240, 140))
    app.click((150, 200))
    assert app.state()["penActive"]
    app.click((100, 100))  # close on the first point
    assert not app.state()["penActive"]
    s = app.frame()
    assert len(s) == 1 and s[0]["closed"] and s[0]["nodes"] == 3 and s[0]["fill"]


def test_pluma_intro_esc_y_deshacer_punto(app):
    app.tool("pen")
    app.object_mode(True)
    for p in [(100, 300), (200, 300), (300, 320)]:
        app.click(p)
    app.combo(Keys.CONTROL, "z")
    app.key(Keys.ENTER)
    s = app.frame()
    assert len(s) == 1 and s[0]["nodes"] == 2 and not s[0]["closed"]
    assert app.state()["frame"] == 0, "Intro no reprodujo la película"
    app.click((100, 350))
    app.click((200, 350))
    app.key(Keys.ESCAPE)
    assert len(app.frame()) == 2


def test_pluma_anade_y_quita_puntos(app):
    app.tool("pen")
    app.object_mode(True)
    app.click((100, 100))
    app.click((300, 100))
    app.key(Keys.ENTER)
    assert app.cursor_at((200, 100)) and len(app.state()["layers"]) == 1
    app.click((200, 100))
    assert app.frame()[0]["nodes"] == 3
    app.click((200, 100))
    assert app.frame()[0]["nodes"] == 2


def test_seleccion_directa(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    app.tool("subselect")
    app.click((150, 150))
    app.drag((200, 200), (260, 240))
    s = app.frame()[0]
    assert close(s["x1"], 260) and close(s["y1"], 240) and s["kind"] == "Trazado"
    # Alt+click turns the corner smooth: the curve bulges out.
    app.click((100, 100), mods=(Keys.ALT,))
    s = app.frame()[0]
    assert s["x0"] < 99 or s["y0"] < 99
    # Delete removes the selected anchor.
    app.click((260, 240))
    app.key(Keys.DELETE)
    assert app.frame()[0]["nodes"] == 3
