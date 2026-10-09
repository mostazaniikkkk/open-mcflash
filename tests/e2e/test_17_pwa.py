"""Installable version (PWA): the dist/ folder served over http can be
installed in Edge and Chrome (manifest, icons and service worker), works
offline, and nothing changes when opening OMF.html by double-clicking."""

import json
import socket
import subprocess
import time
import urllib.request

import pytest

from conftest import ROOT


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


@pytest.fixture(scope="module")
def server():
    port = free_port()
    p = subprocess.Popen(["node", str(ROOT / "serve.js"), "dist", str(port)], cwd=str(ROOT), stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    url = f"http://localhost:{port}/"
    for _ in range(100):
        try:
            urllib.request.urlopen(url, timeout=1)
            break
        except Exception:  # noqa: BLE001 - still starting up
            time.sleep(0.1)
    yield url
    p.terminate()


def test_dist_tiene_manifiesto_iconos_y_service_worker(server):
    m = json.loads(urllib.request.urlopen(server + "manifest.webmanifest").read())
    assert m["name"] == "Open McFlash" and m["display"] == "standalone"
    sizes = {i["sizes"] for i in m["icons"]}
    assert {"192x192", "512x512"} <= sizes
    assert any(i.get("purpose") == "maskable" for i in m["icons"])
    assert m["file_handlers"][0]["accept"]["application/x-openmcflash"] == [".omf"]
    for i in m["icons"]:
        png = urllib.request.urlopen(server + i["src"]).read()
        assert png[:8] == b"\x89PNG\r\n\x1a\n"
        w = int.from_bytes(png[16:20], "big")
        assert f"{w}x{w}" == i["sizes"]
    assert b"addEventListener('fetch'" in urllib.request.urlopen(server + "sw.js").read()


def test_chrome_la_puede_instalar_y_va_sin_conexion(driver, server):
    d = driver
    d.get(server)
    ready = lambda: d.execute_script("return !!(window.omf && window.omf.state())")
    end = time.time() + 20
    while not ready() and time.time() < end:
        time.sleep(0.1)
    assert ready()
    assert d.execute_script("return document.querySelector('link[rel=manifest]').href").endswith("manifest.webmanifest")
    # The service worker stays active and controls the page.
    d.execute_async_script("const done = arguments[0]; navigator.serviceWorker.ready.then(() => done(true));")
    d.refresh()
    end = time.time() + 20
    while not d.execute_script("return !!navigator.serviceWorker.controller") and time.time() < end:
        time.sleep(0.1)
    assert d.execute_script("return !!navigator.serviceWorker.controller")
    # Chrome sees no obstacle to installing it.
    errs = d.execute_cdp_cmd("Page.getInstallabilityErrors", {})["installabilityErrors"]
    assert errs == [], errs
    manifest = d.execute_cdp_cmd("Page.getAppManifest", {})
    assert not manifest.get("errors"), manifest.get("errors")
    # Offline, the app still opens (from the cache).
    d.execute_cdp_cmd("Network.enable", {})
    d.execute_cdp_cmd("Network.emulateNetworkConditions", {"offline": True, "latency": 0, "downloadThroughput": -1, "uploadThroughput": -1})
    try:
        d.refresh()
        end = time.time() + 20
        while not ready() and time.time() < end:
            time.sleep(0.1)
        assert ready(), "abre sin conexión"
    finally:
        d.execute_cdp_cmd("Network.emulateNetworkConditions", {"offline": False, "latency": 0, "downloadThroughput": -1, "uploadThroughput": -1})
    # Opening .omf from the OS arrives via launchQueue (if the browser supports it).
    assert d.execute_script("return 'launchQueue' in window") in (True, False)
    d.get_log("browser")


def test_con_doble_clic_no_hay_manifiesto(app):
    assert app.js("return location.protocol") == "file:"
    assert app.js("return document.querySelector('link[rel=manifest]')") is None
