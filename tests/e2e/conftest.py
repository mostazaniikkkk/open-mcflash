"""Infrastructure for Open McFlash's end-to-end tests (Selenium).

Actions go through the real UI: native WebDriver mouse and keyboard on the
canvas, the menus and the panels. Results are checked by reading the editor
state (``window.omf``), which exposes the Rust/WASM core.

Environment variables:
    OMF_URL     page to test (default: the project's OMF.html via file://)
    OMF_HEADED  "1" to watch the browser while testing
"""

import json
import os
import pathlib
import time

import pytest
from selenium import webdriver
from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.actions.action_builder import ActionBuilder
from selenium.webdriver.common.actions.wheel_input import ScrollOrigin
from selenium.webdriver.common.by import By
from selenium.webdriver.common.keys import Keys

ROOT = pathlib.Path(__file__).resolve().parents[2]
URL = os.environ.get("OMF_URL", (ROOT / "OMF.html").as_uri())
ARTIFACTS = ROOT / "tests" / "e2e" / "artifacts"

# Console errors unrelated to the application.
IGNORED_LOGS = ("favicon", "DevTools")


@pytest.hookimpl(hookwrapper=True)
def pytest_runtest_makereport(item, call):
    outcome = yield
    rep = outcome.get_result()
    setattr(item, "rep_" + rep.when, rep)


@pytest.fixture(scope="session")
def driver(tmp_path_factory):
    opts = webdriver.ChromeOptions()
    if os.environ.get("OMF_HEADED") != "1":
        opts.add_argument("--headless=new")
    opts.add_argument("--window-size=1400,900")
    opts.add_argument("--force-device-scale-factor=1")
    opts.add_argument("--disable-search-engine-choice-screen")
    opts.set_capability("goog:loggingPrefs", {"browser": "ALL"})
    downloads = tmp_path_factory.mktemp("descargas")
    opts.add_experimental_option(
        "prefs", {"download.default_directory": str(downloads), "download.prompt_for_download": False}
    )
    d = webdriver.Chrome(options=opts)
    d.execute_cdp_cmd("Browser.setDownloadBehavior", {"behavior": "allow", "downloadPath": str(downloads)})
    d.downloads = downloads
    yield d
    d.quit()


@pytest.fixture
def app(driver, request):
    a = App(driver)
    a.open(fresh=True)
    yield a
    failed = getattr(request.node, "rep_call", None) and request.node.rep_call.failed
    if failed:
        ARTIFACTS.mkdir(parents=True, exist_ok=True)
        driver.save_screenshot(str(ARTIFACTS / f"{request.node.name}.png"))
    errors = [
        e["message"]
        for e in driver.get_log("browser")
        if e["level"] == "SEVERE" and not any(s in e["message"] for s in IGNORED_LOGS)
    ]
    assert not errors, "Errores en la consola del navegador:\n" + "\n".join(errors)


class App:
    """Browser wrapper with user actions on Open McFlash."""

    def __init__(self, driver):
        self.d = driver

    # ───────── Loading ─────────

    def open(self, fresh=True):
        self.d.get(URL)
        self.wait(lambda: self.js("return !!(window.omf && window.omf.state())"), 15)
        if fresh:
            self.js("localStorage.clear()")
            self.d.get(URL)
            self.wait(lambda: self.js("return !!(window.omf && window.omf.state())"), 15)
        self.d.get_log("browser")  # discard earlier entries
        self.exact_view()

    def exact_view(self):
        """100% view with the Stage on whole pixels: 1 unit = 1 pixel.

        Retries until the canvas stops resizing (the first load may reposition
        it afterwards) and the origin lands on a whole pixel."""
        for _ in range(20):
            self._exact_view_once()
            x0, y0 = self.screen(0, 0)
            x1, _ = self.screen(1, 0)
            if abs(x0 - round(x0)) < 1e-3 and abs(y0 - round(y0)) < 1e-3 and abs(x1 - x0 - 1) < 1e-3:
                time.sleep(0.1)
                x0b, y0b = self.screen(0, 0)
                if (x0b, y0b) == (x0, y0):
                    return
        raise AssertionError("no se pudo fijar una vista exacta")

    def _exact_view_once(self):
        self.js(
            """
            const e = window.omf.ed, r = document.querySelector('#stage').getBoundingClientRect();
            e.set_zoom(100);
            const a = e.doc_coords(0, 0), z = e.doc_coords(1, 0)[0] - a[0];
            const pan = [-a[0] / z, -a[1] / z];
            // The document origin lands on a whole CSS pixel of the window.
            const want = [Math.round(pan[0] + r.left) - r.left + 40, Math.round(pan[1] + r.top) - r.top];
            e.pan_by(want[0] - pan[0], want[1] - pan[1]);
            // Snapping (on by default, as in Flash) is tested separately.
            e.set_snap('objects', false);
            window.omf.refresh();
            """
        )
        time.sleep(0.15)

    def reload(self):
        self.d.refresh()
        self.wait(lambda: self.js("return !!(window.omf && window.omf.state())"), 15)
        self.exact_view()

    # ───────── Reading state ─────────

    def js(self, script, *args):
        return self.d.execute_script(script, *args)

    def state(self):
        return self.js("return window.omf.state()")

    def frame(self, layer=0, frame=None):
        if frame is None:
            frame = self.state()["frame"]
        return json.loads(self.js("return window.omf.ed.debug_frame(arguments[0], arguments[1])", layer, frame))

    def fills(self, layer=0, frame=None):
        return [s for s in self.frame(layer, frame) if s["fill"] and not s["group"]]

    def strokes(self, layer=0, frame=None):
        return [s for s in self.frame(layer, frame) if not s["fill"] and not s["group"]]

    def doc(self):
        return json.loads(self.js("return window.omf.ed.to_json()"))["document"]

    def wait(self, cond, timeout=4.0, msg="la condición no se cumplió"):
        end = time.time() + timeout
        last = None
        while time.time() < end:
            try:
                last = cond()
                if last:
                    return last
            except Exception as e:  # noqa: BLE001 - retry while loading
                last = e
            time.sleep(0.05)
        raise AssertionError(f"{msg} (último valor: {last!r})")

    def toast(self):
        el = self.d.find_element(By.ID, "toast")
        return el.text if el.is_displayed() else ""

    def el(self, css):
        return self.d.find_element(By.CSS_SELECTOR, css)

    def els(self, css):
        return self.d.find_elements(By.CSS_SELECTOR, css)

    # ───────── Coordinates ─────────

    def screen(self, x, y):
        """Document point → window coordinates (CSS)."""
        return self.js(
            """
            const o = window.omf, c = document.querySelector('#stage').getBoundingClientRect();
            const dpr = devicePixelRatio;
            const a = o.ed.doc_coords(0, 0), b = o.ed.doc_coords(1000 * dpr, 1000 * dpr);
            const kx = (b[0] - a[0]) / 1000, ky = (b[1] - a[1]) / 1000;
            return [c.left + (arguments[0] - a[0]) / kx / dpr, c.top + (arguments[1] - a[1]) / ky / dpr];
            """,
            x,
            y,
        )

    def pixel(self, x, y):
        """Color [r, g, b] painted on the canvas at a document point."""
        time.sleep(0.1)  # let the canvas repaint (requestAnimationFrame)
        return self.js(
            """
            const o = window.omf, cv = document.querySelector('#stage');
            const a = o.ed.doc_coords(0, 0), b = o.ed.doc_coords(1000, 1000);
            const kx = (b[0] - a[0]) / 1000, ky = (b[1] - a[1]) / 1000;
            const px = Math.round((arguments[0] - a[0]) / kx), py = Math.round((arguments[1] - a[1]) / ky);
            const d = cv.getContext('2d').getImageData(px, py, 1, 1).data;
            return [d[0], d[1], d[2]];
            """,
            x,
            y,
        )

    # ───────── Mouse ─────────

    def _builder(self, mods=()):
        ab = ActionBuilder(self.d, duration=8)
        kb, pt = ab.key_action, ab.pointer_action

        def ptr(fn, *args):
            fn(*args)
            kb.pause(0)

        def key(fn, *args):
            fn(*args)
            pt.pause(0)

        for m in mods:
            key(kb.key_down, m)
        return ab, kb, pt, ptr, key

    def _xy(self, p):
        x, y = self.screen(*p)
        return int(round(x)), int(round(y))

    def drag(self, a, b, steps=8, mods=()):
        ab, kb, pt, ptr, key = self._builder(mods)
        x0, y0 = self._xy(a)
        x1, y1 = self._xy(b)
        ptr(pt.move_to_location, x0, y0)
        ptr(pt.pointer_down)
        for i in range(1, steps + 1):
            t = i / steps
            ptr(pt.move_to_location, int(round(x0 + (x1 - x0) * t)), int(round(y0 + (y1 - y0) * t)))
        ptr(pt.pointer_up)
        for m in mods:
            key(kb.key_up, m)
        ab.perform()
        time.sleep(0.05)

    def stroke(self, points, mods=()):
        """Freehand stroke through several document points."""
        ab, kb, pt, ptr, key = self._builder(mods)
        xs = [self._xy(p) for p in points]
        ptr(pt.move_to_location, *xs[0])
        ptr(pt.pointer_down)
        for x, y in xs[1:]:
            ptr(pt.move_to_location, x, y)
        ptr(pt.pointer_up)
        for m in mods:
            key(kb.key_up, m)
        ab.perform()
        time.sleep(0.05)

    def click(self, p, mods=()):
        ab, kb, pt, ptr, key = self._builder(mods)
        ptr(pt.move_to_location, *self._xy(p))
        ptr(pt.click)
        for m in mods:
            key(kb.key_up, m)
        ab.perform()
        time.sleep(0.05)

    def dblclick(self, p):
        ab, kb, pt, ptr, key = self._builder()
        ptr(pt.move_to_location, *self._xy(p))
        ptr(pt.double_click)
        ab.perform()
        time.sleep(0.08)

    def hover(self, p):
        ab, kb, pt, ptr, key = self._builder()
        ptr(pt.move_to_location, *self._xy(p))
        ab.perform()
        time.sleep(0.03)

    def right_click(self, p):
        ab, kb, pt, ptr, key = self._builder()
        ptr(pt.move_to_location, *self._xy(p))
        ptr(pt.context_click)
        ab.perform()
        time.sleep(0.08)

    def wheel(self, p, dy, mods=()):
        x, y = self._xy(p)
        ac = ActionChains(self.d)
        for m in mods:
            ac.key_down(m)
        ac.scroll_from_origin(ScrollOrigin.from_viewport(x, y), 0, dy)
        for m in mods:
            ac.key_up(m)
        ac.perform()
        time.sleep(0.1)

    def cursor_at(self, p):
        self.hover(p)
        return self.js("return document.querySelector('#stage').style.cursor")

    # ───────── Keyboard ─────────

    def key(self, *keys):
        self.blur()
        ActionChains(self.d).send_keys(*keys).perform()
        time.sleep(0.05)

    def combo(self, *mods_and_key):
        """combo(Keys.CONTROL, Keys.SHIFT, 'g')"""
        self.blur()
        *mods, k = mods_and_key
        ac = ActionChains(self.d)
        for m in mods:
            ac.key_down(m)
        ac.send_keys(k)
        for m in reversed(mods):
            ac.key_up(m)
        ac.perform()
        time.sleep(0.08)

    def blur(self):
        self.js("if (document.activeElement && document.activeElement !== document.body) document.activeElement.blur()")

    # ───────── UI ─────────

    def tool(self, name):
        self.el(f'#toolbar .tool[data-tool="{name}"]').click()
        self.wait(lambda: self.state()["tool"] == name, msg=f"herramienta {name}")

    def menu(self, *labels):
        """Opens a menu bar menu and walks submenus by their text."""
        btn = next(b for b in self.els("#menus .menu-btn") if b.text.strip() == labels[0])
        btn.click()
        time.sleep(0.08)
        self._walk(labels[1:])

    def ctx_menu(self, p, *labels):
        self.right_click(p)
        self._walk(labels)

    @staticmethod
    def item_label(e):
        lines = [l.strip() for l in e.text.split("\n") if l.strip() and l.strip() != "✓"]
        return lines[0] if lines else ""

    def menu_items(self):
        return [self.item_label(e) for e in self.els(".menu-dropdown .menu-item") if e.is_displayed()]

    def _walk(self, labels):
        for i, label in enumerate(labels):
            items = [e for e in self.els(".menu-dropdown .menu-item") if e.is_displayed()]
            item = next((e for e in reversed(items) if self.item_label(e) == label), None)
            assert item is not None, f"no está el elemento de menú «{label}»: {[e.text for e in items]}"
            if i < len(labels) - 1:
                ActionChains(self.d).move_to_element(item).perform()
                time.sleep(0.12)
            else:
                item.click()
        time.sleep(0.1)

    # ───────── Drawing shortcuts ─────────

    def rect(self, a, b):
        self.tool("rect")
        self.drag(a, b)

    def object_mode(self, on=True):
        if self.state()["objectMode"] != on:
            self.key("j")
        assert self.state()["objectMode"] == on

    def select_mode(self, owner, value):
        """Picks a mode in the options bar dropdown (data-modesel)."""
        from selenium.webdriver.support.ui import Select

        Select(self.el(f'[data-modesel="{owner}"]')).select_by_value(str(value))
        time.sleep(0.05)

    def cell(self, layer, frame):
        """Center of a Timeline cell in window coordinates."""
        r = self.el("#tl-grid").rect
        cw, rows = self.js(
            "const t = window.omf.timeline; return [t.cellW, t.rows.map(r => [r.li, r.y, r.h])]"
        )
        row = next(rr for rr in rows if rr[0] == layer)
        return r["x"] + frame * cw + cw / 2, r["y"] + 20 + row[1] + row[2] / 2

    def drag_xy(self, a, b, steps=6, mods=()):
        """Drag in window coordinates (Timeline, panels…)."""
        ab, kb, pt, ptr, key = self._builder(mods)
        ptr(pt.move_to_location, int(a[0]), int(a[1]))
        ptr(pt.pointer_down)
        for i in range(1, steps + 1):
            t = i / steps
            ptr(pt.move_to_location, int(a[0] + (b[0] - a[0]) * t), int(a[1] + (b[1] - a[1]) * t))
        ptr(pt.pointer_up)
        for m in mods:
            key(kb.key_up, m)
        ab.perform()
        time.sleep(0.08)

    def dblclick_xy(self, x, y):
        ab, kb, pt, ptr, key = self._builder()
        ptr(pt.move_to_location, int(x), int(y))
        ptr(pt.double_click)
        ab.perform()
        time.sleep(0.1)

    def keys_of(self, layer=0):
        return [k["f"] for k in self.state()["layers"][layer]["keys"] if k["f"] < self.state()["layers"][layer]["length"]]

    def click_xy(self, x, y, mods=(), button="left"):
        ab, kb, pt, ptr, key = self._builder(mods)
        ptr(pt.move_to_location, int(x), int(y))
        ptr(pt.context_click if button == "right" else pt.click)
        for m in mods:
            key(kb.key_up, m)
        ab.perform()
        time.sleep(0.08)

    def modal_open(self):
        return self.el("#modal-back").is_displayed()

    # ───────── Test Movie (Ruffle) ─────────

    def test_movie(self, timeout=40):
        """Ctrl+Enter and wait for Ruffle to load the movie (canvas renderer: readable)."""
        self.js("localStorage.setItem('omf.ruffle', JSON.stringify({renderer: 'canvas'}))")
        self.combo(Keys.CONTROL, Keys.ENTER)
        self.wait(lambda: self.js("""
            const p = document.querySelector('#tm-player');
            const c = p && p.shadowRoot && p.shadowRoot.querySelector('canvas');
            return !!(c && c.width > 0);"""), timeout, "Ruffle carga la película")
        time.sleep(0.4)
        return self.el("#tm-player")

    def ruffle_pixel(self, x, y):
        """Color in the player at a Stage point."""
        return self.js(
            """
            const p = document.querySelector('#tm-player'), c = p.shadowRoot.querySelector('canvas');
            const S = window.omf.state();
            const k = Math.min(c.width / S.width, c.height / S.height);
            const ox = (c.width - S.width * k) / 2, oy = (c.height - S.height * k) / 2;
            return Array.from(c.getContext('2d').getImageData(ox + arguments[0] * k, oy + arguments[1] * k, 1, 1).data.slice(0, 3));
            """, x, y)

    def ruffle_point(self, x, y):
        """Stage point → offset from the player's center (for ActionChains)."""
        p = self.el("#tm-player")
        r = p.rect
        st = self.state()
        k = min(r["width"] / st["width"], r["height"] / st["height"])
        ox, oy = (r["width"] - st["width"] * k) / 2, (r["height"] - st["height"] * k) / 2
        return p, int(ox + x * k - r["width"] / 2), int(oy + y * k - r["height"] / 2)

    def output(self):
        return self.el("#output").get_attribute("textContent")

    def set_input(self, css, value):
        el = self.el(css)
        el.click()
        el.send_keys(Keys.CONTROL, "a")
        el.send_keys(str(value))
        el.send_keys(Keys.TAB)
        time.sleep(0.08)
        return el


def swf_plain(data):
    """Uncompressed SWF: Flash 8 publishes compressed ("CWS", zlib) by default."""
    import zlib

    data = bytes(data)
    if data[:3] == b"CWS":
        return b"FWS" + data[3:8] + zlib.decompress(data[8:])
    return data


def swf_codes(data):
    """Codes of a SWF's top-level tags."""
    swf = swf_plain(data)
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
    return codes


# Import shortcuts for the test modules.
__all__ = ["App", "Keys", "By", "swf_plain", "swf_codes"]
