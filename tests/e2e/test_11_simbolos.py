"""Block 3 — symbols and Library: Convert to Symbol (F8), New Symbol
(Ctrl+F8), dragging from the Library, Edit in Place and in symbol-editing mode,
edit path, color effects, filters, blending, graphics with their own Timeline,
buttons (states and simple buttons), Swap, Break Apart, folders and SWF."""

import json
import time

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.support.ui import Select

from conftest import Keys, swf_plain

BLUE = [51, 102, 204]  # default fill (#3366cc)
WHITE = [255, 255, 255]


def convert(app, kind="movieclip", name=None, reg=None):
    """Selects all and presses F8; picks the type (and registration) in the dialog."""
    app.tool("select")
    app.combo(Keys.CONTROL, "a")
    app.key(Keys.F8)
    assert app.modal_open(), "F8 abre «Convertir en símbolo»"
    if name is not None:
        app.set_input("#cs-name", name)
    app.el(f'input[name="cs-kind"][value="{kind}"]').click()
    if reg is not None:
        app.el(f'#cs-reg [data-reg="{reg}"]').click()
    app.el("#cs-ok").click()
    time.sleep(0.1)
    assert not app.modal_open()
    return app.state()["library"][-1]


def lib_rows(app):
    return app.els("#lib-list [data-sym]")


def lib_row(app, name):
    return next(r for r in lib_rows(app) if r.find_element("css selector", ".lname").text == name)


def center(r):
    return r["x"] + r["width"] / 2, r["y"] + r["height"] / 2


def instances(app, layer=0, frame=0):
    return [s for s in app.frame(layer, frame) if s["instance"]]


def test_f8_convierte_en_clip_de_pelicula(app):
    app.rect((100, 100), (200, 160))
    sym = convert(app, "movieclip", name="Caja")
    s = app.state()
    assert sym["name"] == "Caja" and sym["kind"] == "movieclip" and sym["uses"] == 1
    inst = instances(app)
    assert len(inst) == 1 and inst[0]["instance"] == sym["id"]
    assert abs(inst[0]["x0"] - 100) < 1 and abs(inst[0]["x1"] - 200) < 1, "la instancia ocupa el lugar del dibujo"
    assert s["inst"]["kind"] == "movieclip" and s["inst"]["symbolName"] == "Caja"
    assert app.pixel(150, 130) == BLUE
    # Library: the symbol appears selected, with its preview.
    assert [r.find_element("css selector", ".lname").text for r in lib_rows(app)] == ["Caja"]
    assert "sel" in lib_row(app, "Caja").get_attribute("class")
    assert "1 elemento" in app.el("#lib-count").text
    # Instance properties in the Property inspector.
    assert Select(app.el("#pi-kind")).first_selected_option.text == "Clip de película"
    assert app.el("#pi-of").text == "Caja"


def test_registro_en_la_esquina(app):
    app.rect((100, 100), (200, 160))
    convert(app, "graphic", reg=0)
    doc = app.doc()
    inst = doc["layers"][0]["keys"]["0"]["shapes"][0]
    assert abs(inst["m"]["tx"] - 100) < 0.6 and abs(inst["m"]["ty"] - 100) < 0.6, "el registro es la esquina superior izquierda"


def test_arrastrar_desde_la_biblioteca(app):
    app.rect((100, 100), (150, 150))
    convert(app, "graphic", reg=4)
    app.key(Keys.ESCAPE)
    a = center(lib_row(app, app.state()["library"][0]["name"]).rect)
    b = app.screen(400, 250)
    app.drag_xy(a, b, steps=10)
    inst = instances(app)
    assert len(inst) == 2
    new = [i for i in inst if i["x0"] > 300][0]
    assert abs((new["x0"] + new["x1"]) / 2 - 400) < 1.5 and abs((new["y0"] + new["y1"]) / 2 - 250) < 1.5
    assert app.state()["library"][0]["uses"] == 2


def test_editar_en_contexto_y_salir_con_doble_clic(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip", reg=4)
    app.click_xy(*center(lib_row(app, app.state()["library"][0]["name"]).rect))
    app.drag_xy(center(lib_row(app, app.state()["library"][0]["name"]).rect), app.screen(400, 300), steps=10)
    outside = app.screen(480, 60)
    app.tool("select")
    app.dblclick((150, 150))
    s = app.state()
    assert len(s["symPath"]) == 1 and s["symPath"][0]["inPlace"], "doble clic: editar en contexto"
    assert "Símbolo 1" in app.el("#crumbs").text
    # Symbol coordinates: the registration point (center) is at (150,150) on the Stage.
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.tool("oval")
    app.drag((-20, -20), (20, 20))
    app.tool("select")
    app.dblclick_xy(*outside)
    assert app.state()["symPath"] == [], "doble clic fuera vuelve a la escena"
    # The change shows up in both instances.
    assert app.pixel(150, 150) == [255, 0, 0]
    assert app.pixel(400, 300) == [255, 0, 0], "también en la otra instancia"


def test_editar_aparte_desde_la_biblioteca_y_ctrl_e(app):
    app.rect((100, 100), (200, 200))
    sym = convert(app, "movieclip")
    row = lib_row(app, sym["name"])
    ActionChains(app.d).double_click(row.find_element("css selector", ".licon")).perform()
    time.sleep(0.1)
    s = app.state()
    assert len(s["symPath"]) == 1 and not s["symPath"][0]["inPlace"]
    assert s["layers"][0]["name"] == "Capa 1" and len(s["layers"]) == 1, "la línea de tiempo es la del símbolo"
    # The whole window is the symbol's stage (no gray work area).
    assert app.js("const c = document.querySelector('#stage'); return Array.from(c.getContext('2d').getImageData(5, 5, 1, 1).data.slice(0, 3))") == WHITE
    app.tool("rect")
    app.drag((-200, -150), (-150, -100))
    app.combo(Keys.CONTROL, "e")
    s = app.state()
    assert s["symPath"] == [] and s["library"][0]["uses"] == 1
    inst = instances(app)[0]
    assert inst["x1"] - inst["x0"] > 150, "el contenido nuevo está en la instancia"


def test_ruta_de_edicion_vuelve_a_la_escena(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip")
    app.dblclick((150, 150))
    crumbs = app.els("#crumbs .crumb")
    assert len(crumbs) == 2
    crumbs[0].click()
    time.sleep(0.05)
    assert app.state()["symPath"] == []
    # The "Back" button also exits.
    app.dblclick((150, 150))
    app.el("#scope-back").click()
    assert app.state()["symPath"] == []


def test_simbolo_nuevo_ctrl_f8(app):
    app.combo(Keys.CONTROL, Keys.F8)
    assert app.modal_open()
    app.set_input("#ns-name", "Logo")
    app.el('input[name="ns-kind"][value="graphic"]').click()
    app.el("#ns-ok").click()
    s = app.state()
    assert s["symPath"][0]["name"] == "Logo" and s["editingKind"] == "graphic"
    app.rect((-20, -20), (20, 20))
    app.combo(Keys.CONTROL, "e")
    s = app.state()
    assert s["library"][0]["name"] == "Logo" and s["library"][0]["uses"] == 0
    assert instances(app) == [], "Ctrl+F8 no pone nada en el escenario"


def test_efecto_de_color_tinta_y_alfa(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip")
    Select(app.el("#pi-color")).select_by_value("tint")
    assert app.state()["inst"]["color"]["mode"] == "tint"
    app.set_input("#pi-amount", 100)
    assert app.pixel(150, 150) == WHITE, "tinta blanca al 100 %"
    Select(app.el("#pi-color")).select_by_value("alpha")
    app.set_input("#pi-fxv", 0)
    assert app.state()["inst"]["color"] == {"mode": "alpha", "v": 0.0}
    assert app.pixel(150, 150) == WHITE, "alfa 0: invisible"
    Select(app.el("#pi-color")).select_by_value("brightness")
    app.set_input("#pi-fxv", -100)
    assert app.pixel(150, 150) == [0, 0, 0], "brillo −100 %: negro"


def test_filtros_sombra_y_desenfoque(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip")
    before = app.pixel(205, 205)
    assert before == WHITE
    app.el("#flt-add").click()
    app._walk(["Sombra"])
    f = app.state()["inst"]["filters"]
    assert len(f) == 1 and f[0]["kind"] == "dropshadow"
    assert app.pixel(205, 205) != WHITE, "la sombra asoma abajo a la derecha"
    # Change the distance in the parameters.
    app.set_input('[data-fp="distance"]', 20)
    assert app.state()["inst"]["filters"][0]["distance"] == 20
    # Disable the filter with its checkmark.
    app.el('[data-fon="0"]').click()
    assert app.state()["inst"]["filters"][0]["on"] is False
    assert app.pixel(205, 205) == WHITE
    app.el('[data-fon="0"]').click()
    app.el("#flt-add").click()
    app._walk(["Desenfocar"])
    assert [x["kind"] for x in app.state()["inst"]["filters"]] == ["dropshadow", "blur"]
    app.el("#flt-del").click()
    assert len(app.state()["inst"]["filters"]) == 1


def test_modo_de_mezcla(app):
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.rect((50, 50), (250, 250))
    app.js("window.omf.ed.set_fill('#00ff00', 255, true)")
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    app.tool("select")
    app.click((150, 150))
    app.key(Keys.F8)
    app.el('input[name="cs-kind"][value="movieclip"]').click()
    app.el("#cs-ok").click()
    Select(app.el("#pi-blend")).select_by_value("multiply")
    assert app.state()["inst"]["blend"] == "multiply"
    assert app.pixel(150, 150) == [0, 0, 0], "rojo × verde = negro"
    Select(app.el("#pi-blend")).select_by_value("screen")
    assert app.pixel(150, 150) == [255, 255, 0], "trama: amarillo"


def test_grafico_sigue_la_linea_de_tiempo(app):
    app.object_mode(True)
    app.rect((0, 100), (40, 140))
    sym = convert(app, "graphic", reg=0)
    row = lib_row(app, sym["name"])
    ActionChains(app.d).double_click(row.find_element("css selector", ".licon")).perform()
    time.sleep(0.1)
    app.click_xy(*app.cell(0, 4))
    app.key(Keys.F6)
    assert app.state()["editingKind"] == "graphic"
    app.tool("select")
    app.click((20, 20))
    app.drag((20, 20), (220, 20))
    app.combo(Keys.CONTROL, "e")
    app.click_xy(*app.cell(0, 9))
    app.key(Keys.F5)
    app.js("window.omf.ed.set_frame(0); window.omf.refresh()")
    assert app.pixel(20, 120) == BLUE and app.pixel(220, 120) == WHITE
    app.js("window.omf.ed.set_frame(4); window.omf.refresh()")
    assert app.pixel(220, 120) == BLUE, "fotograma 5 del gráfico"
    app.js("window.omf.ed.set_frame(5); window.omf.refresh()")
    assert app.pixel(20, 120) == BLUE, "reproducir indefinidamente: vuelve al principio"
    # Single Frame: always the first one.
    app.js("window.omf.ed.set_frame(0); window.omf.refresh()")
    app.click((20, 120))
    Select(app.el("#pi-loop")).select_by_value("single")
    app.js("window.omf.ed.set_frame(4); window.omf.refresh()")
    assert app.pixel(20, 120) == BLUE and app.pixel(220, 120) == WHITE


def make_button(app):
    app.rect((100, 100), (200, 160))
    sym = convert(app, "button")
    app.dblclick((150, 130))
    s = app.state()
    assert s["editingKind"] == "button"
    # "Over" state: a different color.
    app.click_xy(*app.cell(0, 1))
    app.key(Keys.F6)
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.tool("bucket")
    app.click((50, 30))  # symbol coordinates (registration at top left)
    app.combo(Keys.CONTROL, "e")
    app.tool("select")
    app.key(Keys.ESCAPE)
    return sym


def test_boton_estados_y_regla(app):
    make_button(app)
    app.dblclick((150, 130))
    assert app.js("return window.omf.timeline.cellW") == 64, "celdas anchas con los estados"
    app.combo(Keys.CONTROL, "e")
    # In the editor the button shows "Up".
    assert app.pixel(150, 130) == BLUE


def test_botones_simples_y_probar_pelicula(app):
    make_button(app)
    app.menu("Control", "Habilitar botones simples")
    assert app.state()["simpleButtons"]
    app.hover((150, 130))
    assert app.pixel(150, 130) == [255, 0, 0], "con botones simples, el puntero encima muestra «Sobre»"
    app.hover((400, 300))
    assert app.pixel(150, 130) == BLUE
    app.combo(Keys.CONTROL, Keys.ALT, "b")
    assert not app.state()["simpleButtons"]
    # Test Movie: the button responds to the mouse.
    app.test_movie()
    assert app.ruffle_pixel(150, 130) == BLUE
    el, ox, oy = app.ruffle_point(150, 130)
    ActionChains(app.d).move_to_element_with_offset(el, ox, oy).perform()
    app.wait(lambda: app.ruffle_pixel(150, 130) == [255, 0, 0], 4, "el botón pasa a «Sobre» en Ruffle")
    app.key(Keys.ESCAPE)


def test_intercambiar_simbolo(app):
    app.object_mode(True)
    app.rect((100, 100), (200, 200))
    convert(app, "graphic", name="A")
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.rect((300, 100), (340, 140))
    app.tool("select")
    app.click((320, 120))
    app.key(Keys.F8)
    app.set_input("#cs-name", "B")
    app.el("#cs-ok").click()
    app.click((150, 150))
    assert app.state()["inst"]["symbolName"] == "A"
    app.el("#pi-swap").click()
    row = next(r for r in app.els("#sw-list [data-sym]") if r.text == "B")
    row.click()
    app.el("#sw-ok").click()
    assert app.state()["inst"]["symbolName"] == "B"
    assert app.pixel(120, 120) == [255, 0, 0] and app.pixel(170, 170) == WHITE


def test_separar_instancia(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip")
    Select(app.el("#pi-color")).select_by_value("tint")
    app.set_input("#pi-amount", 100)
    app.combo(Keys.CONTROL, "b")
    s = app.frame()
    assert not any(x["instance"] for x in s)
    assert any(x["fill"] == "#ffffff" for x in s), "el efecto de color queda aplicado"
    assert app.state()["library"][0]["uses"] == 0, "el símbolo sigue en la biblioteca"


def test_renombrar_carpetas_y_eliminar(app):
    app.rect((100, 100), (200, 200))
    sym = convert(app, "movieclip")
    row = lib_row(app, sym["name"])
    ActionChains(app.d).double_click(row.find_element("css selector", ".lname")).perform()
    inp = app.el("#lib-list .lname input")
    inp.send_keys(Keys.CONTROL, "a")
    inp.send_keys("Personaje", Keys.ENTER)
    time.sleep(0.1)
    assert app.state()["library"][0]["name"] == "Personaje"
    assert app.state()["inst"]["symbolName"] == "Personaje"
    # New folder and drag the symbol into it.
    app.el("#lib-newfolder").click()
    folders = app.state()["libFolders"]
    assert len(folders) == 1
    frow = app.el(f'#lib-list [data-folder="{folders[0]["id"]}"]')
    app.drag_xy(center(lib_row(app, "Personaje").rect), center(frow.rect), steps=8)
    assert app.state()["library"][0]["folder"] == folders[0]["id"]
    # Delete: asks for confirmation because there are instances, and removes them too.
    app.click_xy(*center(lib_row(app, "Personaje").rect))
    app.el("#lib-delete").click()
    assert app.modal_open()
    app.el("#ld-ok").click()
    s = app.state()
    assert s["library"] == [] and instances(app) == []
    app.combo(Keys.CONTROL, "z")
    assert len(app.state()["library"]) == 1 and len(instances(app)) == 1, "deshacer recupera símbolo e instancia"


def test_menu_contextual_de_la_biblioteca(app):
    app.rect((100, 100), (200, 200))
    sym = convert(app, "movieclip")
    ActionChains(app.d).context_click(lib_row(app, sym["name"])).perform()
    time.sleep(0.1)
    app._walk(["Duplicar…"])
    names = sorted(x["name"] for x in app.state()["library"])
    assert names == sorted([sym["name"], sym["name"] + " copia"])
    ActionChains(app.d).context_click(lib_row(app, sym["name"])).perform()
    time.sleep(0.1)
    app._walk(["Tipo", "Botón"])
    assert next(x for x in app.state()["library"] if x["name"] == sym["name"])["kind"] == "button"


def test_escala_9_divisiones(app):
    app.object_mode(True)
    app.tool("rect")
    app.js("window.omf.ed.set_rect_radius(20)")
    app.rect((100, 100), (200, 160))
    app.tool("select")
    app.combo(Keys.CONTROL, "a")
    app.key(Keys.F8)
    app.el('input[name="cs-kind"][value="movieclip"]').click()
    app.el("#cs-9").click()
    app.el("#cs-ok").click()
    assert app.state()["library"][0]["scale9"]
    # Scale to triple size: the rounded corners don't get distorted.
    app.js("window.omf.ed.set_sel_bounds(100, 100, 300, 60); window.omf.refresh()")
    inst = instances(app)[0]
    assert abs(inst["x1"] - 400) < 2
    assert app.pixel(103, 103) == WHITE, "la esquina sigue redondeada"
    assert app.pixel(115, 106) == BLUE, "con su radio original (sin 9 divisiones sería una elipse más ancha)"
    assert app.pixel(125, 130) == BLUE
    # Guides are visible while editing the symbol.
    app.dblclick((250, 130))
    assert app.state()["editingScale9"] is not None


def test_swf_con_sprites_botones_y_filtros(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip")
    app.el("#pi-name").send_keys("caja", Keys.TAB)
    app.el("#flt-add").click()
    app._walk(["Iluminado"])
    app.object_mode(True)
    app.rect((300, 100), (350, 150))
    app.tool("select")
    app.click((325, 125))
    app.key(Keys.F8)
    app.el('input[name="cs-kind"][value="button"]').click()
    app.el("#cs-ok").click()
    swf = swf_plain(app.js("return Array.from(window.omf.ed.export_swf())"))
    assert swf[:3] == b"FWS" and int.from_bytes(swf[4:8], "little") == len(swf)
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
    assert 39 in codes, "DefineSprite"
    assert 34 in codes, "DefineButton2"
    assert 70 in codes, "PlaceObject3 (filtro)"
    assert b"caja\x00" in swf, "nombre de instancia"


def test_guardar_y_abrir_conserva_la_biblioteca(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip", name="Caja")
    Select(app.el("#pi-color")).select_by_value("alpha")
    app.set_input("#pi-fxv", 50)
    data = app.js("return window.omf.ed.to_json()")
    app.js("window.omf.ed.new_document(550, 400); window.omf.refresh()")
    assert app.state()["library"] == []
    app.js("window.omf.ed.load_json(arguments[0]); window.omf.refresh()", data)
    s = app.state()
    assert s["library"][0]["name"] == "Caja"
    app.tool("select")
    app.click((150, 150))
    assert app.state()["inst"]["color"]["v"] == 50
    px = app.pixel(150, 150)
    assert all(abs(a - b) <= 2 for a, b in zip(px, [153, 178, 229])), f"alfa 50 % sobre blanco: {px}"


def test_deshacer_dentro_y_fuera_del_simbolo(app):
    app.rect((100, 100), (200, 200))
    convert(app, "movieclip")
    app.dblclick((150, 150))
    n0 = len(app.frame())
    app.object_mode(True)
    app.tool("oval")
    app.drag((-10, -10), (10, 10))
    assert len(app.frame()) == n0 + 1
    app.combo(Keys.CONTROL, "e")
    assert app.state()["symPath"] == []
    app.combo(Keys.CONTROL, "z")
    assert len(app.state()["symPath"]) == 1, "deshacer la salida vuelve a la edición del símbolo"
    app.combo(Keys.CONTROL, "z")
    assert len(app.state()["symPath"]) == 1 and len(app.frame()) == n0
    app.combo(Keys.CONTROL, "e")
    assert app.state()["symPath"] == []
