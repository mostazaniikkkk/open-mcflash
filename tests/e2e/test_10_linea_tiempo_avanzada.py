"""Block 2 — advanced Timeline: ranges, copy/paste/reverse/move frames,
shape tweens and shape hints, motion tween options, custom easing,
motion guides, masks, folders, layer properties, labels, full onion
skinning, frame view and scenes."""

import json
import math
import time

from selenium.webdriver.support.ui import Select

from conftest import Keys, swf_plain


def obj_rect(app, a, b):
    app.object_mode(True)
    app.rect(a, b)


def animate(app, frames=10, dx=200):
    """Rectangle object at 0 moved `dx` at keyframe `frames`, with a motion tween."""
    obj_rect(app, (100, 100), (150, 150))
    app.click_xy(*app.cell(0, frames))
    app.key(Keys.F6)
    app.tool("select")
    app.click((125, 125))
    app.drag((125, 125), (125 + dx, 125))
    app.key(Keys.ESCAPE)
    app.click_xy(*app.cell(0, 0))
    Select(app.el("#pp-tween")).select_by_value("motion")
    assert app.state()["tween"]["tweenKind"] == "motion"


def center_x(app, f):
    s = app.frame(0, f)[0]
    return (s["x0"] + s["x1"]) / 2


def test_seleccionar_rango_arrastrando_y_con_mayus(app):
    app.rect((100, 100), (150, 150))
    app.drag_xy(app.cell(0, 2), app.cell(0, 6))
    s = app.state()["frameSel"]
    assert (s["f0"], s["f1"]) == (2, 6)
    app.click_xy(*app.cell(0, 9), mods=(Keys.SHIFT,))
    s = app.state()["frameSel"]
    assert (s["f0"], s["f1"]) == (2, 9)


def test_f5_y_f6_sobre_un_rango(app):
    app.rect((100, 100), (150, 150))
    app.drag_xy(app.cell(0, 0), app.cell(0, 3))
    app.key(Keys.F5)
    assert app.state()["total"] == 5
    # Dragging over an already selected keyframe would move it (as in Flash): range with click + Shift+click.
    app.click_xy(*app.cell(0, 0))
    app.click_xy(*app.cell(0, 4), mods=(Keys.SHIFT,))
    app.key(Keys.F6)
    assert app.keys_of() == [0, 1, 2, 3, 4], "convertir en fotogramas clave"


def test_copiar_y_pegar_fotogramas_con_teclado(app):
    animate(app)
    app.click_xy(*app.cell(0, 0))
    app.click_xy(*app.cell(0, 10), mods=(Keys.SHIFT,))
    app.combo(Keys.CONTROL, Keys.ALT, "c")
    app.click_xy(*app.cell(0, 20))
    app.combo(Keys.CONTROL, Keys.ALT, "v")
    assert 20 in app.keys_of() and 30 in app.keys_of()
    assert abs(center_x(app, 25) - center_x(app, 5)) < 0.6, "el tramo pegado conserva la interpolación"


def test_mover_un_clave_arrastrando(app):
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 5))
    app.key(Keys.F6)
    app.click_xy(*app.cell(0, 5))
    app.drag_xy(app.cell(0, 5), app.cell(0, 9))
    assert app.keys_of() == [0, 9]


def test_invertir_fotogramas(app):
    animate(app)
    app.click_xy(*app.cell(0, 0))
    app.click_xy(*app.cell(0, 10), mods=(Keys.SHIFT,))
    x_end = center_x(app, 10)
    app.menu("Modificar", "Línea de tiempo", "Invertir fotogramas")
    assert abs(center_x(app, 0) - x_end) < 0.6


def test_borrar_fotogramas_alt_retroceso_y_cortar(app):
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 9))
    app.key(Keys.F5)
    app.drag_xy(app.cell(0, 3), app.cell(0, 5))
    app.combo(Keys.ALT, Keys.BACKSPACE)
    assert app.frame(0, 4) == [] and app.frame(0, 7) != []
    app.combo(Keys.CONTROL, Keys.ALT, "a")
    s = app.state()["frameSel"]
    assert (s["f0"], s["f1"]) == (0, 9)


def test_interpolacion_de_forma(app):
    app.rect((100, 100), (200, 200))
    app.click_xy(*app.cell(0, 10))
    app.key(Keys.F7)
    app.rect((100, 100), (400, 200))
    app.click_xy(*app.cell(0, 0))
    Select(app.el("#pp-tween")).select_by_value("shape")
    k = app.state()["layers"][0]["keys"][0]
    assert k["shape"] and k["tween"]
    mid = [s for s in app.frame(0, 5) if s["fill"]][0]
    assert abs((mid["x1"] - mid["x0"]) - 200) < 3, mid


def test_consejos_de_forma(app):
    app.rect((100, 100), (200, 200))
    app.click_xy(*app.cell(0, 10))
    app.key(Keys.F7)
    app.tool("oval")
    app.drag((300, 100), (400, 200))
    app.click_xy(*app.cell(0, 0))
    Select(app.el("#pp-tween")).select_by_value("shape")
    app.combo(Keys.CONTROL, Keys.SHIFT, "h")
    s = app.state()
    assert s["showHints"] and s["tween"]["hints"] == 1
    assert app.pixel(150, 150) != [51, 102, 204], "se dibuja el consejo «a»"
    app.tool("select")
    app.drag((150, 150), (100, 100))
    hint = app.doc()["layers"][0]["keys"]["0"]["hints"][0]
    assert abs(hint[0] - 100) < 1 and abs(hint[1] - 100) < 1, hint
    app.combo(Keys.CONTROL, Keys.ALT, "h")
    assert not app.state()["showHints"]


def test_rotacion_horaria_con_vueltas_y_sin_escala(app):
    animate(app, dx=0)
    Select(app.el("#pp-rot")).select_by_value("cw")
    app.set_input("#pp-spins", 1)
    t = app.state()["tween"]
    assert t["rotate"] == "cw" and t["spins"] == 1
    assert abs(abs(app.frame(0, 5)[0]["rot"]) - math.pi) < 0.05, "media vuelta a mitad de camino"
    app.el("#pp-scale").click()
    assert app.state()["tween"]["scale"] is False


def test_aceleracion_personalizada(app):
    animate(app)
    linear = center_x(app, 3)
    app.el('#pb-props [data-cmd="easeDialog"]').click()
    assert app.modal_open()
    app.el('[data-ez="0,0,0.58,1"]').click()
    app.el("#ez-ok").click()
    assert app.state()["tween"]["easeCurve"] is not None
    assert center_x(app, 3) > linear + 5, "con salida suave avanza más al principio"


def test_guia_de_movimiento(app):
    animate(app, frames=20, dx=300)
    app.el('.tl-foot [data-cmd="addMotionGuide"]').click()
    s = app.state()
    assert s["layers"][0]["kind"] == "motionguide" and s["layers"][1]["guided"]
    app.click_xy(*app.cell(0, 0))
    app.object_mode(False)
    app.tool("pencil")
    app.select_mode("pencil", "smooth")
    pts = [(125 + i * 7.5, 125 + 140 * math.sin(math.pi * i / 40)) for i in range(41)]
    app.stroke(pts)
    mid = app.frame(1, 10)[0]
    cy = (mid["y0"] + mid["y1"]) / 2
    assert cy > 220, f"el objeto sigue la curva de la guía (y={cy})"


def test_mascara_desde_el_menu_de_capa(app):
    app.js("window.omf.ed.set_fill('#ff0000', 255, true)")
    app.rect((50, 50), (350, 300))
    app.el('.tl-foot [data-cmd="addLayer"]').click()
    app.js("window.omf.ed.set_fill('#000000', 255, true)")
    app.rect((150, 150), (250, 200))
    row = app.el('#tl-layers .layer[data-i="0"]')
    from selenium.webdriver.common.action_chains import ActionChains

    ActionChains(app.d).context_click(row).perform()
    app._walk(["Máscara"])
    s = app.state()
    assert s["layers"][0]["kind"] == "mask" and s["layers"][1]["masked"]
    assert s["layers"][0]["locked"] and s["layers"][1]["locked"]
    assert app.pixel(200, 175) == [255, 0, 0], "dentro de la máscara se ve la capa enmascarada"
    assert app.pixel(80, 80) == [255, 255, 255], "fuera de la máscara no"


def test_carpeta_plegar_y_meter_capas(app):
    app.rect((100, 100), (150, 150))
    app.el('.tl-foot [data-cmd="addFolder"]').click()
    s = app.state()
    assert s["layers"][0]["kind"] == "folder"
    # Drag the layer below into the folder (right under it).
    app.el('.tl-foot [data-cmd="addLayer"]').click()  # new layer at the very top
    rows = app.els("#tl-layers .layer")
    a, b = rows[0].rect, rows[1].rect
    app.drag_xy((a["x"] + 80, a["y"] + a["height"] / 2), (b["x"] + 80, b["y"] + b["height"] - 2), steps=8)
    s = app.state()
    folder = next(l for l in s["layers"] if l["kind"] == "folder")
    assert any(l["parent"] == folder["uid"] for l in s["layers"]), "la capa entra en la carpeta"
    n = len(app.els("#tl-layers .layer"))
    app.el("#tl-layers [data-fold]").click()
    assert len(app.els("#tl-layers .layer")) < n, "carpeta plegada"


def test_propiedades_de_capa(app):
    icon = app.el('#tl-layers .layer[data-i="0"] .licon')
    from selenium.webdriver.common.action_chains import ActionChains

    ActionChains(app.d).double_click(icon).perform()
    assert app.modal_open()
    app.set_input("#lp-name", "Fondo")
    app.el('input[name=lp-kind][value="guide"]').click()
    Select(app.el("#lp-height")).select_by_value("2")
    app.el("#lp-ok").click()
    l = app.state()["layers"][0]
    assert l["name"] == "Fondo" and l["kind"] == "guide" and l["height"] == 2
    assert app.el('#tl-layers .layer[data-i="0"]').size["height"] == 44


def test_etiqueta_de_fotograma(app):
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 0))
    app.set_input("#pp-label", "inicio")
    Select(app.el("#pp-labelkind")).select_by_value("comment")
    k = app.state()["layers"][0]["keys"][0]
    assert k["label"] == "inicio" and k["labelKind"] == "comment"


def test_papel_cebolla_completo(app):
    app.el("#onionbtn").click()
    app.el("#onionmarks").click()
    app._walk(["Papel cebolla 5"])
    s = app.state()
    assert s["onionBefore"] == 5 and s["onionAfter"] == 5
    app.el("#onionoutl").click()
    assert app.state()["onionOutlines"]
    # Drag the right marker on the ruler.
    x, y = app.cell(0, 5)
    ruler_y = app.el("#tl-grid").rect["y"] + 10
    app.drag_xy((x, ruler_y), (app.cell(0, 8)[0], ruler_y))
    assert app.state()["onionAfter"] == 8


def test_editar_varios_fotogramas(app):
    obj_rect(app, (100, 100), (150, 150))
    app.click_xy(*app.cell(0, 4))
    app.key(Keys.F6)
    app.click_xy(*app.cell(0, 0))
    app.el("#editmulti").click()
    app.el("#onionmarks").click()
    app._walk(["Papel cebolla 5"])
    assert app.state()["editMultiple"]
    app.tool("select")
    app.click((125, 125))
    app.drag((125, 125), (175, 125))
    assert abs(app.frame(0, 4)[0]["x0"] - 150) < 0.6, "el cambio llega a los claves del rango"


def test_vista_de_fotogramas_y_vista_previa(app):
    app.rect((100, 100), (150, 150))
    app.el(".tl-viewbtn").click()
    app._walk(["Grande"])
    assert app.js("return window.omf.timeline.cellW") == 18
    app.click_xy(*app.cell(0, 3))
    assert app.state()["frame"] == 3
    app.el(".tl-viewbtn").click()
    app._walk(["Vista previa"])
    assert app.el('#tl-layers .layer[data-i="0"]').size["height"] == 34
    app.el(".tl-viewbtn").click()
    app._walk(["Vista previa"])
    app.el(".tl-viewbtn").click()
    app._walk(["Normal"])


def test_escenas(app):
    app.rect((100, 100), (150, 150))
    app.click_xy(*app.cell(0, 4))
    app.key(Keys.F5)
    app.el('#pb-scene [data-cmd="sceneAdd"]').click()
    s = app.state()
    assert s["scenes"] == ["Escena 1", "Escena 2"] and s["scene"] == 1
    assert app.frame() == [] and "Escena 2" in app.el("#crumbs").text
    app.click_xy(*app.cell(0, 2))
    app.key(Keys.F5)
    assert app.state()["movieTotal"] == 8
    # Rename by double-clicking in the panel.
    row = app.el('#pb-scene [data-scene="0"]')
    from selenium.webdriver.common.action_chains import ActionChains

    ActionChains(app.d).double_click(row).perform()
    inp = app.wait(lambda: app.el("#pb-scene input"))
    inp.send_keys(Keys.CONTROL, "a")
    inp.send_keys("Intro", Keys.ENTER)
    assert app.state()["scenes"][0] == "Intro"
    # Switch scenes with the edit bar button.
    app.el("#scene-btn").click()
    app._walk(["Intro"])
    assert app.state()["scene"] == 0 and len(app.frame()) == 5
    # Test Movie plays through both scenes.
    assert app.js("return window.omf.ed.movie_total()") == 8
    app.js("""const e = window.omf.ed; e.scene_go(1); e.set_frame(0); e.set_actions('trace("escena dos")'); e.scene_go(0); window.omf.refresh()""")
    app.test_movie()
    app.wait(lambda: "escena dos" in app.output(), 10, "la película pasa a la segunda escena")
    app.key(Keys.ESCAPE)
    # Shift+F2 hides the Scene panel.
    app.combo(Keys.SHIFT, Keys.F2)
    assert not app.el('[data-panel="scene"]').is_displayed()


def test_swf_con_escenas_y_mascara(app):
    for p in app.d.downloads.iterdir():
        p.unlink()
    app.rect((50, 50), (300, 300))
    app.el('.tl-foot [data-cmd="addLayer"]').click()
    app.rect((100, 100), (150, 150))
    app.js("window.omf.ed.set_layer_kind(0, 'mask'); window.omf.refresh()")
    app.el('#pb-scene [data-cmd="sceneAdd"]').click()
    app.click_xy(*app.cell(0, 2))
    app.key(Keys.F5)
    app.menu("Archivo", "Exportar", "Exportar película (SWF)…")
    end = time.time() + 6
    f = None
    while time.time() < end and not f:
        f = next((p for p in app.d.downloads.iterdir() if p.suffix == ".swf"), None)
        time.sleep(0.1)
    time.sleep(0.2)
    swf = swf_plain(f.read_bytes())
    nbits = swf[8] >> 3
    rect = (5 + 4 * nbits + 7) // 8
    frames = int.from_bytes(swf[8 + rect + 2: 8 + rect + 4], "little")
    assert frames == 4, "1 fotograma de la escena 1 + 3 de la escena 2"
    # There is at least one PlaceObject2 with ClipDepth (the mask).
    i, clip = 8 + rect + 4, False
    while i + 2 <= len(swf):
        h = int.from_bytes(swf[i:i + 2], "little")
        code, ln = h >> 6, h & 0x3F
        i += 2
        if ln == 0x3F:
            ln = int.from_bytes(swf[i:i + 4], "little")
            i += 4
        if code == 26 and swf[i] & 0x40:
            clip = True
        if code == 0:
            break
        i += ln
    assert clip
