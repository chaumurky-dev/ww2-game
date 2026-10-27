#!/usr/bin/env python3
"""Phone audit of the Wartide web client, driven over the Chrome DevTools Protocol.

For each route (optionally with a scripted scenario, `path::scenario`) it emulates a phone
(Emulation.setDeviceMetricsOverride + touch + iPhone UA), waits for the wasm app, takes a
screenshot with Page.captureScreenshot and prints:
  * innerWidth and documentElement.scrollWidth (horizontal page scroll = scrollWidth > innerWidth)
  * every element sticking out of the viewport (not clipped by a scrolling ancestor)
  * tap targets under 44x44 CSS px among `a, button, input, select, [role=button], .drag-handle`
  * text nodes with a computed font-size under 13px (SVG text is skipped: it scales with the map)

Usage:
  scripts/mobile-audit.py http://localhost:8103 / /help /sandbox::combat --out docs/screenshots/mobile/before
  scripts/mobile-audit.py http://localhost:8103 --routes-file routes.txt --viewport 430x932 --login me:pw
Options: --viewport WxH (default 390x844; `844x390` = landscape), --login user:password (POSTs
/api/auth/login in the page first), --port (Chrome debug port), --json FILE (machine readable).
Needs: python3 `websocket-client`, Google Chrome. Scenarios live in SCENARIOS below.
"""
import argparse
import base64
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

import websocket

CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
UA = ("Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 "
      "(KHTML, like Gecko) Version/17.4 Mobile/15E148 Safari/604.1")

# JS helpers available to scenarios (installed on every page load).
HELPERS = r"""
window.__sleep = ms => new Promise(r => setTimeout(r, ms));
window.__tap = async (sel, nth) => {
  const els = [...document.querySelectorAll(sel)];
  const el = els[nth || 0];
  if (!el) throw new Error('no element for ' + sel);
  el.scrollIntoView({block: 'center'});
  el.click();
  await window.__sleep(250);
};
window.__tapText = async (sel, text) => {
  let el;
  for (let i = 0; i < 40 && !el; i++) {
    el = [...document.querySelectorAll(sel)].find(e => e.textContent.trim().startsWith(text));
    if (!el) await window.__sleep(250);
  }
  if (!el) throw new Error('no ' + sel + ' with text ' + text);
  el.click();
  await window.__sleep(250);
};
window.__submit = async (o) => {
  const r = window.wartideSubmit(JSON.stringify(o));
  await window.__sleep(150);
  return r;
};
window.__phase = () => JSON.parse(window.wartideState()).phase;
window.__advance = async (phase) => {
  for (let i = 0; i < 20 && window.__phase() !== phase; i++) await window.__submit({type: 'end_phase'});
};
"""

# Scenario name -> async JS body run after the page loaded (sandbox game screen).
SCENARIOS = {
    "research": "await __sleep(300);",
    "purchase": "await __advance('purchase'); await __tapText('.mtab','Actions');",
    "combat-move": "await __advance('combat_move'); await __tapText('.mtab','Actions');",
    "combat-move-board": "await __advance('combat_move'); await __tap('.board-wrap');",
    "combat": "await __advance('combat'); await __tapText('.mtab','Actions');",
    "noncombat": "await __advance('non_combat_move'); await __tapText('.mtab','Actions');",
    "mobilize": "await __advance('mobilize'); await __tapText('.mtab','Actions');",
    "chat": "await __tapText('.mtab','Chat');",
    "log": "await __tapText('.mtab','Log');",
    "actions": "await __tapText('.mtab','Actions');",
    "menu": "await __tap('.nav-toggle');",
    "topbar-open": "await __tap('.topbar-toggle');",
    "create": "await __tapText('button','New private game'); await __sleep(400);",
    "create-bottom": "await __tapText('button','New private game'); await __sleep(400); const m=document.querySelector('.modal'); m.scrollTop=m.scrollHeight; await __sleep(200);",
}

def _touch(c, kind, x=0, y=0):
    pts = [] if kind == "touchEnd" else [{"x": x, "y": y, "id": 1}]
    c.call("Input.dispatchTouchEvent", type=kind, touchPoints=pts)


def tap_xy(c, x, y):
    _touch(c, "touchStart", x, y)
    time.sleep(0.05)
    _touch(c, "touchEnd")
    time.sleep(0.3)


def drag_xy(c, x1, y1, x2, y2, steps=14):
    _touch(c, "touchStart", x1, y1)
    for i in range(1, steps + 1):
        _touch(c, "touchMove", x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps)
        time.sleep(0.03)
    _touch(c, "touchEnd")
    time.sleep(0.4)


def loc_point(c, loc):
    """A client point that hit-tests to territory `loc` (inside the board), or None."""
    js = ("(()=>{const w=document.querySelector('.board-wrap').getBoundingClientRect();"
          "for(const p of document.querySelectorAll('path.loc[data-id=\"%s\"]')){const b=p.getBoundingClientRect();"
          "for(let i=1;i<40;i++)for(let j=1;j<40;j++){const x=b.x+b.width*i/40,y=b.y+b.height*j/40;"
          "if(x<w.left+4||x>w.right-4||y<w.top+4||y>w.bottom-4)continue;"
          "const e=document.elementFromPoint(x,y);if(e&&e.getAttribute&&e.getAttribute('data-id')==='%s')"
          "return JSON.stringify([x,y]);}}return 'null';})()") % (loc, loc)
    return json.loads(c.eval(js))


def wheel_zoom(c, x, y, notches=6):
    for _ in range(notches):
        c.call("Input.dispatchMouseEvent", type="mouseWheel", x=x, y=y, deltaX=0, deltaY=-120)
        time.sleep(0.05)
    time.sleep(0.3)


def ev(c, js):
    return c.eval("(async()=>{" + js + "})()", await_promise=True)


def sc_select(c):
    ev(c, "await __advance('combat_move'); await __tapText('.mtab','Board');")
    p = loc_point(c, "caucasus")
    if p:
        tap_xy(c, *p)
    ev(c, "await __tapText('.mtab','Actions');")


def sc_drag_picker(c):
    ev(c, "await __advance('combat_move'); await __tapText('.mtab','Board');")
    p = loc_point(c, "caucasus")
    if not p:
        raise RuntimeError("caucasus not hit-testable")
    u = loc_point(c, "ukraine_s_s_r") or p
    # zoom about the middle of the two territories, then pan so they sit near the board's centre
    wheel_zoom(c, (p[0] + u[0]) / 2, (p[1] + u[1]) / 2, notches=5)
    h = c.eval("(()=>{const w=document.querySelector('.board-wrap').getBoundingClientRect();"
               "const hs=[...document.querySelectorAll('.drag-handle.on')].map(e=>{const r=e.getBoundingClientRect();"
               "return {x:r.x+r.width/2,y:r.y+r.height/2,t:e.textContent.trim()}}).filter(h=>h.x>w.left&&h.x<w.right&&h.y>w.top&&h.y<w.bottom);"
               "return JSON.stringify(hs)})()")
    hs = json.loads(h)
    hs = [x for x in hs if x["t"] == "×5"] or hs
    d = loc_point(c, "ukraine_s_s_r")
    if not hs or not d:
        raise RuntimeError("no handle/dest visible: %s %s" % (hs, d))
    drag_xy(c, hs[0]["x"], hs[0]["y"], d[0], d[1])


def sc_battle(c):
    ev(c, "await __advance('combat_move');"
          "const s=JSON.parse(wartideState());"
          "for (const [from,to] of [['caucasus','ukraine_s_s_r'],['karelia_s_s_r','finland_norway']]) {"
          " const ids=s.units.filter(u=>u.owner=='ussr'&&u.location==from&&['infantry','armor'].includes(u.kind)).map(u=>u.id);"
          " await __submit({type:'move',units:ids,path:[to]}); }"
          "await __submit({type:'end_phase'});"
          "await __submit({type:'begin_battle',location:'ukraine_s_s_r'});"
          "await __sleep(600);")


def sc_undo(c):
    ev(c, "await __advance('non_combat_move');"
          "const s=JSON.parse(wartideState());"
          "const ids=s.units.filter(u=>u.owner=='ussr'&&u.location=='russia'&&u.kind=='infantry').map(u=>u.id).slice(0,2);"
          "for (const to of ['caucasus','karelia_s_s_r','evenki_national_okrug']) { if(!(await __submit({type:'move',units:ids,path:[to]}))) break; }"
          "await __tapText('.mtab','Actions');")


SCENARIOS.update({"select": sc_select, "drag-picker": sc_drag_picker, "battle": sc_battle, "undo": sc_undo})
# Extra scenarios may be appended from a JSON file: --scenarios FILE ({"name": "js"}).

AUDIT = r"""
(() => {
  const W = window.innerWidth;
  const sel = el => {
    let s = el.tagName.toLowerCase();
    if (el.id) s += '#' + el.id;
    const c = (el.getAttribute('class') || '').trim().split(/\s+/).filter(Boolean).slice(0, 3);
    if (c.length) s += '.' + c.join('.');
    return s;
  };
  const visible = el => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    const cs = getComputedStyle(el);
    return cs.visibility !== 'hidden' && cs.display !== 'none';
  };
  const inSvg = el => !!el.closest('svg') && el.tagName.toLowerCase() !== 'svg';
  // clipped: some ancestor with overflow-x other than visible whose box lies within the viewport
  const clipped = el => {
    for (let p = el.parentElement; p && p !== document.documentElement; p = p.parentElement) {
      const cs = getComputedStyle(p);
      if (cs.overflowX !== 'visible') {
        const r = p.getBoundingClientRect();
        if (r.left >= -0.5 && r.right <= W + 0.5) return true;
      }
      if (cs.position === 'fixed') break;
    }
    return false;
  };
  const out = {innerWidth: W, innerHeight: window.innerHeight,
    scrollWidth: document.documentElement.scrollWidth,
    bodyScrollWidth: document.body.scrollWidth, overflow: [], small: [], inline: [], handles: [], fonts: {}};
  for (const el of document.querySelectorAll('body *')) {
    if (inSvg(el) || !visible(el)) continue;
    const r = el.getBoundingClientRect();
    if ((r.right > W + 0.5 || r.left < -0.5) && !clipped(el))
      out.overflow.push({sel: sel(el), left: Math.round(r.left), right: Math.round(r.right),
        px: Math.round(Math.max(r.right - W, -r.left))});
  }
  out.overflow.sort((a, b) => b.px - a.px);
  const tap = 'a, button, input, select, textarea, [role=button], .drag-handle';
  for (const el of document.querySelectorAll(tap)) {
    if (!visible(el)) continue;
    if (el.type === 'hidden') continue;
    const r = el.getBoundingClientRect();
    let w = r.width, h = r.height;
    for (const ps of ['::before', '::after']) {
      const cs = getComputedStyle(el, ps);
      if (cs.position === 'absolute' && cs.content !== 'none') {
        w = Math.max(w, parseFloat(cs.width) || 0);
        h = Math.max(h, parseFloat(cs.height) || 0);
      }
    }
    if (w < 43.5 || h < 43.5) {
      const item = {sel: sel(el), w: Math.round(w), h: Math.round(h),
        text: (el.textContent || el.getAttribute('aria-label') || '').trim().slice(0, 24)};
      const lab = el.closest('label');
      if (el.matches('input[type=checkbox],input[type=radio]') && lab) {
        const lr = lab.getBoundingClientRect();
        if (lr.height >= 43.5 && lr.width >= 43.5) continue;  // the whole label row is the target
      }
      // WCAG 2.5.8 inline exception: a link inside a sentence.
      const par = el.parentElement;
      if (el.tagName === 'A' && getComputedStyle(el).display === 'inline' && par &&
          (par.textContent || '').trim().length > (el.textContent || '').trim().length + 8) {
        out.inline.push(item); continue;
      }
      // Counters on the map scale with the zoom level; they are tapped via the territory.
      if (el.classList.contains('drag-handle')) { out.handles.push(item); continue; }
      out.small.push(item);
    }
  }
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (!n.nodeValue.trim()) continue;
    const p = n.parentElement;
    if (!p || p.closest('svg, script, style') || !visible(p)) continue;
    const fs = parseFloat(getComputedStyle(p).fontSize);
    // badges / pills may be 12px; everything else needs 13px+
    const min = p.closest('.pill,.tag,.badge,.unit-badge,.step,.cbadge,.hitnum') ? 11.95 : 12.95;
    if (fs < min) {
      const k = sel(p) + ' ' + fs + 'px';
      out.fonts[k] = (out.fonts[k] || 0) + 1;
    }
  }
  return JSON.stringify(out);
})()
"""


class CDP:
    def __init__(self, ws_url):
        self.ws = websocket.create_connection(ws_url, max_size=None, suppress_origin=True)
        self.n = 0

    def call(self, method, **params):
        self.n += 1
        self.ws.send(json.dumps({"id": self.n, "method": method, "params": params}))
        while True:
            msg = json.loads(self.ws.recv())
            if msg.get("id") == self.n:
                if "error" in msg:
                    raise RuntimeError(f"{method}: {msg['error']}")
                return msg.get("result", {})

    def eval(self, js, await_promise=False):
        r = self.call("Runtime.evaluate", expression=js, returnByValue=True,
                      awaitPromise=await_promise)
        if "exceptionDetails" in r:
            d = r["exceptionDetails"]
            raise RuntimeError(d.get("exception", {}).get("description") or d.get("text"))
        return r["result"].get("value")


def launch_chrome(port, profile):
    proc = subprocess.Popen(
        [CHROME, "--headless=new", f"--remote-debugging-port={port}", f"--user-data-dir={profile}",
         "--no-first-run", "--disable-gpu", "--hide-scrollbars", "about:blank"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        try:
            tabs = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json"))
            page = next(t for t in tabs if t["type"] == "page")
            return proc, page["webSocketDebuggerUrl"]
        except Exception:
            time.sleep(0.2)
    proc.kill()
    sys.exit("chrome did not start")


def emulate(c, w, h, landscape):
    c.call("Emulation.setDeviceMetricsOverride", width=w, height=h, deviceScaleFactor=3, mobile=True,
           screenOrientation={"type": "landscapePrimary" if landscape else "portraitPrimary",
                              "angle": 90 if landscape else 0})
    c.call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
    c.call("Emulation.setUserAgentOverride", userAgent=UA, platform="iPhone")


def wait_app(c, timeout=40):
    t0 = time.time()
    good = 0
    while time.time() - t0 < timeout:
        try:
            ok = c.eval("document.readyState==='complete' && document.body.children.length>0 "
                        "&& !document.querySelector('.skeleton')")
        except Exception:
            ok = False
        good = good + 1 if ok else 0
        if good >= 4:
            time.sleep(0.5)
            return True
        time.sleep(0.4)
    return False


def slug(route, vp):
    s = re.sub(r"[^A-Za-z0-9]+", "-", route).strip("-") or "home"
    return f"{s}__{vp}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("base")
    ap.add_argument("routes", nargs="*")
    ap.add_argument("--routes-file")
    ap.add_argument("--out", default="docs/screenshots/mobile/run")
    ap.add_argument("--viewport", default="390x844")
    ap.add_argument("--login")
    ap.add_argument("--port", type=int, default=9333)
    ap.add_argument("--json")
    ap.add_argument("--game", help="game id substituted for GAME in routes")
    ap.add_argument("--scenarios")
    ap.add_argument("--full-page", action="store_true", help="capture beyond the viewport")
    a = ap.parse_args()
    routes = list(a.routes)
    if a.routes_file:
        routes += [l.strip() for l in open(a.routes_file) if l.strip() and not l.startswith("#")]
    if a.game:
        routes = [r.replace("GAME", a.game) for r in routes]
    else:
        routes = [r for r in routes if "GAME" not in r]
    scen = dict(SCENARIOS)
    if a.scenarios:
        scen.update(json.load(open(a.scenarios)))
    w, h = (int(x) for x in a.viewport.split("x"))
    os.makedirs(a.out, exist_ok=True)
    profile = tempfile.mkdtemp(prefix="mobile-audit-")
    proc, ws = launch_chrome(a.port, profile)
    results = {}
    try:
        c = CDP(ws)
        c.call("Page.enable")
        c.call("Runtime.enable")
        emulate(c, w, h, w > h)
        c.call("Page.addScriptToEvaluateOnNewDocument", source=HELPERS)
        if a.login:
            user, pw = a.login.split(":", 1)
            c.call("Page.navigate", url=a.base + "/help")
            wait_app(c)
            r = c.eval("fetch('/api/auth/login',{method:'POST',headers:{'content-type':'application/json'},"
                       f"body:JSON.stringify({{login:{json.dumps(user)},password:{json.dumps(pw)}}})}})"
                       ".then(r=>r.status)", await_promise=True)
            print(f"login -> HTTP {r}")
        vp = f"{w}x{h}"
        hdr = f"{'route':34} {'iw':>4} {'sw':>4} {'ovf':>4} {'#out':>4} {'#tap<44':>7} {'#font<13':>8}"
        print(hdr)
        for route in routes:
            path, _, sc = route.partition("::")
            c.call("Page.navigate", url=a.base + path)
            ok = wait_app(c)
            err = None
            if sc:
                js = scen.get(sc)
                try:
                    if js is None:
                        raise RuntimeError("unknown scenario " + sc)
                    if callable(js):
                        js(c)
                    else:
                        c.eval("(async()=>{" + js + "})()", await_promise=True)
                    time.sleep(0.5)
                except Exception as e:  # keep auditing the other routes
                    err = str(e)[:120]
            res = json.loads(c.eval(AUDIT))
            res["loaded"] = ok
            res["error"] = err
            res["maxOverflow"] = max([0, res["scrollWidth"] - res["innerWidth"]] +
                                     [o["px"] for o in res["overflow"]])
            shot = c.call("Page.captureScreenshot", format="png", captureBeyondViewport=a.full_page)
            fn = os.path.join(a.out, slug(route, vp) + ".png")
            with open(fn, "wb") as f:
                f.write(base64.b64decode(shot["data"]))
            res["screenshot"] = fn
            results[route + "@" + vp] = res
            print(f"{route:34} {res['innerWidth']:>4} {res['scrollWidth']:>4} {res['maxOverflow']:>4} "
                  f"{len(res['overflow']):>4} {len(res['small']):>7} {sum(res['fonts'].values()):>8}"
                  + (f"   !! {err}" if err else "") + ("" if ok else "   (app did not finish loading)"))
            for o in res["overflow"][:8]:
                print(f"    overflow {o['px']:>4}px  {o['sel']}  [{o['left']}..{o['right']}]")
            sm = {}
            for s in res["small"]:
                sm.setdefault(f"{s['sel']} {s['w']}x{s['h']}", []).append(s["text"])
            for k, v in list(sm.items())[:10]:
                print(f"    small tap  x{len(v):<3} {k}  {v[0]!r}")
            if res["inline"] or res["handles"]:
                print(f"    (exempt: {len(res['inline'])} inline links, {len(res['handles'])} map counters)")
            for k, v in list(res["fonts"].items())[:6]:
                print(f"    small font x{v:<3} {k}")
    finally:
        proc.terminate()
        shutil.rmtree(profile, ignore_errors=True)
    if a.json:
        with open(a.json, "w") as f:
            json.dump(results, f, indent=1)


if __name__ == "__main__":
    main()
