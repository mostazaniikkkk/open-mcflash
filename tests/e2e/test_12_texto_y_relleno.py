"""Block 4 — text and fill: gradients (Color Mixer, Swatches, Lock
Fill), Fill Transform, Text (static, dynamic, input, formatting, editing,
Break Apart), Lasso, PolyStar and the Free Transform modes (skew,
distort, envelope)."""

import json
import time

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.support.ui import Select

from conftest import Keys, swf_plain

WHITE = [255, 255, 255]


def ink(app, x0, y0, x1, y1):
    """Non-white canvas pixels inside a document rectangle."""
    time.sleep(0.1)
    return app.js(
        """
        const o = window.omf, cv = document.querySelector('#stage');
        const a = o.ed.doc_coords(0, 0), b = o.ed.doc_coords(1000, 1000);
        const kx = (b[0] - a[0]) / 1000, ky = (b[1] - a[1]) / 1000;
        const X = (x) => Math.round((x - a[0]) / kx), Y = (y) => Math.round((y - a[1]) / ky);
        const [x0, y0, x1, y1] = [X(arguments[0]), Y(arguments[1]), X(arguments[2]), Y(arguments[3])];
        const d = cv.getContext('2d').getImageData(x0, y0, x1 - x0, y1 - y0).data;
        let n = 0;
        for (let i = 0; i < d.length; i += 4) if (d[i] < 200 || d[i + 1] < 200 || d[i + 2] < 200) n++;
        return n;
        """,
        x0, y0, x1, y1,
    )


def shapes(app):
    return app.frame(0, 0)


def texts(app):
    return [s for s in shapes(app) if s["kind"].startswith("Texto")]


def type_text(app, at, text, to=None):
    app.tool("text")
    if to:
        app.drag(at, to)
    else:
        app.click(at)
    app.wait(lambda: app.els(".text-edit"), msg="cuadro de edición")
    ActionChains(app.d).send_keys(text).perform()
    time.sleep(0.1)


# ───────── Gradients ─────────

def test_muestra_de_degradado_y_rectangulo(app):
    app.el('[data-gsw="0"]').click()  # linear white → black
    s = app.state()
    assert s["fillGrad"]["kind"] == "linear" and len(s["fillGrad"]["stops"]) == 2
    assert "gradient" in app.js("return document.querySelector('#well-fill i').style.background")
    app.js("window.omf.ed.set_stroke('#000000', 255, false); window.omf.refresh()")
    app.rect((100, 100), (300, 200))
    sh = shapes(app)[0]
    assert sh["grad"] and sh["grad"]["kind"] == "linear"
    left, right = app.pixel(104, 150), app.pixel(296, 150)
    assert left[0] > 230 and right[0] < 30, (left, right)
    # Lock Fill: the gradient fits the Stage and continues across shapes.
    app.tool("bucket")
    app.el('#tb-opts [data-cmd="lockFill"]').click()
    assert app.state()["lockFill"]
    app.object_mode(True)
    app.rect((20, 300), (60, 340))
    g = [x for x in shapes(app) if x["x0"] < 30][0]["grad"]
    assert abs(g["m"]["tx"] - 275) < 0.5 and abs(g["m"]["a"] - 275) < 0.5


def test_mezclador_tipo_paradas_y_desbordamiento(app):
    Select(app.el("#mx-type")).select_by_value("radial")
    g = app.state()["fillGrad"]
    assert g["kind"] == "radial"
    assert app.el("#mx-grad").is_displayed()
    # Click on the bar: one more color.
    strip = app.el(".gbar-strip")
    app.js("arguments[0].scrollIntoView({block: 'center'})", strip)
    ActionChains(app.d).move_to_element_with_offset(strip, 0, 0).click().perform()
    time.sleep(0.1)
    assert len(app.state()["fillGrad"]["stops"]) == 3
    # The selected pointer's color is changed with the hex field.
    app.set_input("#mx-hex", "#00ff00")
    stops = app.state()["fillGrad"]["stops"]
    assert any(s["c"] == {"r": 0, "g": 255, "b": 0, "a": 255} for s in stops)
    # Dragging a pointer down removes it.
    mark = app.els(".gmark")[1]
    app.js("arguments[0].scrollIntoView({block: 'center'})", mark)
    r = mark.rect
    app.drag_xy((r["x"] + r["width"] / 2, r["y"] + 4), (r["x"] + r["width"] / 2, r["y"] + 70), steps=6)
    assert len(app.state()["fillGrad"]["stops"]) == 2
    Select(app.el("#mx-spread")).select_by_value("reflect")
    assert app.state()["fillGrad"]["spread"] == "reflect"
    Select(app.el("#mx-type")).select_by_value("solid")
    assert app.state()["fillGrad"] is None


def test_degradado_a_la_seleccion_y_cubo(app):
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.click((150, 150))
    app.el('[data-gsw="2"]').click()  # red radial
    assert shapes(app)[0]["grad"]["kind"] == "radial"
    assert app.pixel(150, 150)[0] > 200 and app.pixel(150, 150)[1] < 60, "centro rojo"
    # The Paint Bucket with a solid color turns it back to solid.
    app.key(Keys.ESCAPE)
    app.js("window.omf.ed.set_fill('#00ff00', 255, true); window.omf.refresh()")
    app.tool("bucket")
    app.click((150, 150))
    assert all(not s["grad"] for s in shapes(app) if s["fill"])
    assert app.pixel(150, 150) == [0, 255, 0]


def test_transformacion_de_relleno(app):
    app.el('[data-gsw="0"]').click()
    app.object_mode(True)
    app.rect((100, 100), (300, 200))
    app.tool("filltransform")
    app.click((200, 150))
    assert app.state()["sel"], "el relleno degradado queda seleccionado"
    g0 = shapes(app)[0]["grad"]["m"]
    assert abs(g0["tx"] - 200) < 0.5
    # Dragging the center moves the gradient.
    app.drag((200, 150), (240, 150))
    g1 = shapes(app)[0]["grad"]["m"]
    assert abs(g1["tx"] - 240) < 0.5
    # The square (width) handle narrows it.
    app.drag((340, 150), (290, 150))
    assert abs(shapes(app)[0]["grad"]["m"]["a"] - 50) < 1
    # The corner circle rotates it.
    app.drag((290, 100), (240, 60))
    m = shapes(app)[0]["grad"]["m"]
    assert abs(m["b"]) > 5, "girado"
    app.combo(Keys.CONTROL, "z")
    assert abs(shapes(app)[0]["grad"]["m"]["b"]) < 1e-3


# ───────── Text ─────────

def test_texto_con_un_clic(app):
    type_text(app, (100, 100), "Hola mundo")
    box = app.el(".text-edit")
    assert box.text == "Hola mundo"
    assert app.state()["textEdit"]
    app.key(Keys.ESCAPE)
    assert not app.els(".text-edit")
    t = texts(app)
    assert len(t) == 1 and t[0]["kind"] == "Texto estático"
    assert t[0]["x1"] - t[0]["x0"] > 80, "ancho automático según el texto"
    assert ink(app, t[0]["x0"], t[0]["y0"], t[0]["x1"], t[0]["y1"]) > 100, "el texto se dibuja"
    app.combo(Keys.CONTROL, "z")
    assert texts(app) == [], "crear y escribir es un solo paso de deshacer"


def test_texto_de_ancho_fijo_parte_lineas(app):
    type_text(app, (100, 100), "uno dos tres cuatro cinco seis siete", to=(220, 100))
    app.key(Keys.ESCAPE)
    t = texts(app)[0]
    assert abs((t["x1"] - t["x0"]) - 120) < 1
    assert t["y1"] - t["y0"] > 80, "varias líneas"


def test_campo_vacio_desaparece_y_clic_fuera_termina(app):
    app.tool("text")
    app.click((100, 100))
    app.wait(lambda: app.els(".text-edit"))
    app.click((300, 300))  # another new field: the empty one disappears
    ActionChains(app.d).send_keys("B").perform()
    app.tool("select")
    t = texts(app)
    assert len(t) == 1 and abs(t[0]["x0"] - 300) < 1


def test_formato_desde_propiedades(app):
    type_text(app, (100, 100), "Formato")
    app.key(Keys.ESCAPE)
    app.tool("select")
    app.click((130, 115))
    assert app.state()["textSel"]
    w0 = texts(app)[0]["x1"] - texts(app)[0]["x0"]
    app.set_input("#pt-size", 48)
    assert app.state()["textStyle"]["size"] == 48
    w1 = texts(app)[0]["x1"] - texts(app)[0]["x0"]
    assert w1 > w0 * 1.7
    app.el("#pt-bold").click()
    app.el("#pt-italic").click()
    st = app.state()["textStyle"]
    assert st["bold"] and st["italic"]
    Select(app.el("#pt-font")).select_by_value("_serif")
    assert app.state()["textStyle"]["font"] == "_serif"
    app.el('[data-talign="right"]').click()
    assert app.state()["textStyle"]["align"] == "right"
    # Dynamic text: instance name, border and line type.
    Select(app.el("#pt-kind")).select_by_value("dynamic")
    assert texts(app)[0]["kind"] == "Texto dinámico"
    app.set_input("#pt-name", "marcador")
    app.el("#pt-border").click()
    Select(app.el("#pt-lines")).select_by_value("single")
    st = app.state()["textStyle"]
    assert st["name"] == "marcador" and st["border"] and st["lines"] == "single"
    app.combo(Keys.CONTROL, "z")
    assert app.state()["textStyle"]["lines"] == "multi", "cada cambio se deshace por separado"


def test_doble_clic_edita_y_menu_texto(app):
    type_text(app, (100, 100), "Abc")
    app.key(Keys.ESCAPE)
    app.tool("select")
    app.dblclick((110, 115))
    assert app.state()["textEdit"] and app.state()["tool"] == "text"
    ActionChains(app.d).send_keys("def").perform()
    app.key(Keys.ESCAPE)
    doc_text = [s for s in app.doc()["layers"][0]["keys"]["0"]["shapes"]][0]["geom"]["text"]
    assert doc_text == "Abcdef"
    app.tool("select")
    app.click((110, 115))
    app.menu("Texto", "Estilo", "Negrita")
    assert app.state()["textStyle"]["bold"]
    app.menu("Texto", "Alinear", "Centrar")
    assert app.state()["textStyle"]["align"] == "center"
    app.menu("Texto", "Tamaño", "72")
    assert app.state()["textStyle"]["size"] == 72
    app.combo(Keys.CONTROL, Keys.SHIFT, "i")
    assert app.state()["textStyle"]["italic"]


def test_separar_texto_en_letras_y_formas(app):
    type_text(app, (100, 100), "OMF")
    app.key(Keys.ESCAPE)
    app.tool("select")
    app.click((110, 115))
    app.combo(Keys.CONTROL, "b")
    letters = sorted(x["geom"]["text"] for x in app.doc()["layers"][0]["keys"]["0"]["shapes"])
    assert letters == ["F", "M", "O"], "una letra por campo"
    assert app.state()["sel"]["count"] == 3
    app.combo(Keys.CONTROL, "b")
    assert texts(app) == [], "segunda vez: formas"
    assert any(s["raw"] for s in shapes(app))


def test_texto_de_entrada_y_swf(app):
    type_text(app, (100, 100), "Escribe")
    Select(app.el("#pt-kind")).select_by_value("input")
    app.set_input("#pt-max", 5)
    app.key(Keys.ESCAPE)
    assert texts(app)[0]["kind"] == "Texto de entrada"
    swf = swf_plain(app.js("return Array.from(window.omf.ed.export_swf())"))
    nbits = swf[8] >> 3
    i = 8 + (5 + nbits * 4 + 7) // 8 + 4
    codes = []
    while i + 2 <= len(swf):
        h = int.from_bytes(swf[i:i + 2], "little")
        i += 2
        n = h & 0x3F
        if n == 0x3F:
            n = int.from_bytes(swf[i:i + 4], "little")
            i += 4
        codes.append(h >> 6)
        i += n
    assert 37 in codes and 48 in codes, "DefineEditText con su fuente de dispositivo"


# ───────── Lasso, PolyStar and transformations ─────────

def test_lazo_a_mano_y_poligonal(app):
    app.object_mode(True)
    app.rect((100, 100), (150, 150))
    app.rect((300, 100), (350, 150))
    app.object_mode(False)
    app.tool("lasso")
    app.stroke([(80, 80), (180, 80), (180, 180), (80, 180), (80, 90)])
    sel = app.state()["sel"]
    assert sel and sel["count"] == 1 and abs(sel["x"] - 100) < 1
    # Polygon mode on a raw shape: cuts it along the polygon's edge.
    app.key(Keys.ESCAPE)
    app.js("window.omf.ed.set_stroke('#000000', 255, false); window.omf.refresh()")
    app.tool("rect")
    app.drag((100, 250), (300, 350))
    app.tool("lasso")
    app.el('#tb-opts [data-cmd="lassoPoly"]').click()
    assert app.state()["lassoPoly"]
    for p in [(50, 230), (200, 230), (200, 380)]:
        app.click(p)
    app.dblclick((50, 380))
    s = app.state()
    assert not s["lassoActive"] and s["sel"]
    assert abs(s["sel"]["x"] - 100) < 1.5 and abs(s["sel"]["w"] - 100) < 1.5, s["sel"]
    assert len([x for x in shapes(app) if x["fill"] and x["raw"]]) == 2, "el lazo corta el relleno"


def test_polystar_desde_el_menu_del_rectangulo(app):
    app.object_mode(True)
    ActionChains(app.d).context_click(app.el('#toolbar [data-tool="rect"]')).perform()
    time.sleep(0.1)
    app._walk(["Herramienta PolyStar"])
    assert app.state()["tool"] == "polystar"
    app.el('#optionsbar [data-cmd="polystarDialog"]').click()
    Select(app.el("#ps-style")).select_by_value("star")
    app.set_input("#ps-sides", 6)
    app.el("#ps-ok").click()
    assert app.state()["polystar"] == {"style": "star", "sides": 6, "point": 0.5}
    app.drag((200, 200), (200, 120))
    s = shapes(app)[0]
    assert s["nodes"] == 12
    assert abs(s["y0"] - 120) < 1 and abs((s["y1"] + s["y0"]) / 2 - 200) < 15
    # The tool slot remembers PolyStar; R goes back to the rectangle.
    assert app.el("#toolbar .flyout").get_attribute("data-tool") == "polystar"
    app.key("r")
    assert app.state()["tool"] == "rect"


def test_sesgar_distorsionar_y_envoltura(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    app.tool("transform")
    app.click((150, 150))
    # Skew: drag the top edge between handles.
    app.drag((130, 100), (180, 100))
    s = shapes(app)[0]
    assert abs((s["x1"] - s["x0"]) - 200) < 2, s
    app.combo(Keys.CONTROL, "z")
    # Distort from the Modify > Transform menu.
    app.menu("Modificar", "Transformar", "Distorsionar")
    assert app.state()["xformMode"] == "distort"
    app.drag((200, 200), (260, 240))
    s = shapes(app)[0]
    assert abs(s["x1"] - 260) < 1.5 and abs(s["y1"] - 240) < 1.5
    app.combo(Keys.CONTROL, "z")
    # Envelope: the top midpoint curves the side.
    app.el('#tb-opts [data-xmode="envelope"]').click()
    assert app.state()["xformMode"] == "envelope"
    app.drag((150, 100), (150, 60))
    s = shapes(app)[0]
    assert abs(s["y0"] - 60) < 2 and s["nodes"] > 4
    assert app.pixel(105, 65) == WHITE and app.pixel(150, 70) != WHITE, "la parte de arriba es curva"
    app.el('#tb-opts [data-xmode="envelope"]').click()
    assert app.state()["xformMode"] == "free"


def test_guardar_y_abrir_con_degradado_y_texto(app):
    app.el('[data-gsw="1"]').click()
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    type_text(app, (250, 100), "Guardado")
    app.key(Keys.ESCAPE)
    data = app.js("return window.omf.ed.to_json()")
    app.js("window.omf.ed.new_document(550, 400); window.omf.refresh()")
    app.js("window.omf.ed.load_json(arguments[0]); window.omf.refresh()", data)
    s = shapes(app)
    assert any(x["grad"] for x in s) and len(texts(app)) == 1
    assert ink(app, 250, 100, 360, 140) > 100
