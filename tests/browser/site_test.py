"""Browser tests for the Revive site, run against a fake NEAR node.

Every RPC request the page makes is answered here, so the tests need no network and
can serve a hostile coin: script payloads in its name, symbol, links, icon, history
and account fields. A test fails if any payload runs, if a page errors, or if a
flow doesn't reach the state it should.

    pip install playwright && python -m playwright install chromium
    python tests/browser/site_test.py          # serves web/ itself on a free port
"""
import base64, functools, http.server, json, socket, sys, threading, time
from pathlib import Path
from playwright.sync_api import sync_playwright

WEB = Path(__file__).resolve().parents[2] / "web"
FACTORY = "revive-5svgh.revive.testnet"
EVIL = f"evil.{FACTORY}"
X = "window.__xss=(window.__xss||[]).concat"  # every payload records itself here if it runs
NOW = 1_800_000_000


def payload(tag):
    return f'<img src=x onerror="{X}([\'{tag}\'])">'


def hostile_status(**over):
    s = {
        "status": "flatlined", "now": NOW, "keeper": "dev.testnet", "creator": "dev.testnet", "lives": 1,
        "launched_at": NOW - 10 * 86400, "last_pulse": NOW - 4 * 86400, "flatline_at": NOW - 86400,
        "total_supply": str(10**27), "circulating": str(9 * 10**26), "treasury": str(10**24),
        "spent_in_period": "0", "period_resets_at": NOW + 86400, "dev_locked": str(10**26), "dev_claimed": "0",
        "dev_burned": "0", "takeover": None, "takeover_count": 0,
        "config": {"name": payload("name"), "symbol": f"<svg onload=\"{X}(['symbol'])\">", "decimals": 18,
                   "dev_allocation": str(10**26), "pulse_interval_secs": 3 * 86400, "vote_duration_secs": 86400,
                   "quorum_bps": 2000, "vest_duration_secs": 30 * 86400, "spend_cap": str(10**24),
                   "spend_period_secs": 86400, "comeback_penalty": str(10**24),
                   "dao_suffix": '.sputnikv2.testnet"><img src=x onerror="' + X + "(['suffix'])\">"},
        "icon": f"javascript:{X}(['icon'])",
        "links": f"javascript:{X}(['link1']) https://ok.example/\"><img src=x onerror=\"{X}(['link2'])\"> https://fine.example",
    }
    s.update(over)
    return s


COIN_HASH = "6Unt3cdMzSNsXSSwveYTaqTfTazFq6UL3sriKbJg1kRi"     # pinned in the app for revive-5svgh
GUARD_HASH = "Au6wxAAPsAN6UXSWYJducNQnwb1i6LtVfs66HfbqzEiV"
TOKEN = "tok.testnet"
GUARD = f"g.{FACTORY}"
GUARD_STATUS = hostile_status(kind="guard", token_id=TOKEN, keeper="dev.testnet", creator="dev.testnet", staked=str(15 * 10**25),
                              stakers=3, dev_locked=str(10**26), dev_burned="0", status="alive", flatline_at=NOW + 86400,
                              icon=None, links=None)
GUARD_STATUS["config"] = dict(GUARD_STATUS["config"], name="Guarded", symbol="TOK", dao_suffix=".sputnikv2.testnet")

LORE = [{"at": NOW - 86400, "kind": f"<b onmouseover=\"{X}(['kind'])\">x</b>", "actor": "dev.testnet", "note": payload("note")},
        {"at": NOW - 3600, "kind": "pulse", "actor": "dev.testnet", "note": "plain"}]


def answer(params):
    """The fake node: returns a JSON-RPC result for one query."""
    rt, acc = params.get("request_type"), params.get("account_id")
    if rt == "view_account":
        code = GUARD_HASH if acc.startswith("g.") else COIN_HASH
        return {"amount": str(3 * 10**24), "locked": "0", "code_hash": code, "storage_usage": 1000}
    if rt == "view_access_key_list":
        return {"keys": [{"public_key": "ed25519:x", "access_key": {}}] * 2 if acc == TOKEN else []}
    if rt != "call_function":
        return {"error": "unsupported"}
    m = params["method_name"]
    args = json.loads(base64.b64decode(params.get("args_base64") or "e30=") or b"{}")
    v = None
    if m == "get_coin_count":
        v = 1 if acc == FACTORY else 0
    elif m == "get_coins":
        v = [{"account_id": EVIL, "name": payload("list-name"), "symbol": payload("list-sym"), "creator": "dev.testnet",
              "launched_at": NOW - 10 * 86400, "kind": "coin", "token_id": None}] if acc == FACTORY else []
    elif m == "get_status":
        v = GUARD_STATUS if acc.startswith("g.") else hostile_status()
    elif m == "get_lore_count":
        v = len(LORE)
    elif m == "get_lore":
        v = LORE
    elif m == "get_code_hash":
        v = COIN_HASH
    elif m == "get_guard_code_hash":
        v = GUARD_HASH
    elif m == "get_dao_suffixes":
        v = [".sputnikv2.testnet"]
    elif m == "ft_metadata":
        v = {"spec": "ft-1.0.0", "name": payload("meta-name"), "symbol": "EVIL", "decimals": 18, "icon": None}
    elif m == "ft_total_supply":
        v = str(10**27)
    elif m == "ft_balance_of" and acc == TOKEN:
        v = str(25 * 10**25)
    elif m in ("get_vote_weight", "ft_balance_of", "get_stake"):
        v = "0"
    elif m in ("get_ballot", "storage_balance_of"):
        v = None
    elif m == "get_launch_cost" or m == "get_connect_cost":
        v = str(24 * 10**23)
    else:
        return {"error": f"no method {m}"}
    return {"result": list(json.dumps(v).encode()), "logs": [], "block_height": 1, "block_hash": "x"}


def rpc_route(route):
    req = json.loads(route.request.post_data or "{}")
    res = answer(req.get("params", {}))
    route.fulfill(status=200, content_type="application/json", body=json.dumps({"jsonrpc": "2.0", "id": req.get("id"), "result": res}))


def serve():
    s = socket.socket(); s.bind(("127.0.0.1", 0)); port = s.getsockname()[1]; s.close()
    h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(WEB))
    http.server.SimpleHTTPRequestHandler.log_message = lambda *a: None
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", port), h)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return f"http://127.0.0.1:{port}/index.html"


RESULTS = []


def check(name, ok, detail=""):
    RESULTS.append((name, bool(ok), detail))
    print(("PASS " if ok else "FAIL ") + name + (f"  ({detail})" if detail and not ok else ""))


def main():
    base = serve()
    with sync_playwright() as p:
        b = p.chromium.launch(executable_path="/usr/bin/google-chrome" if Path("/usr/bin/google-chrome").exists() else None)
        ctx = b.new_context(viewport={"width": 1280, "height": 900})
        # Offline: fonts and the wallet library are not needed for these checks.
        ctx.route("**/*", lambda r: r.continue_() if r.request.url.startswith(base.rsplit("/", 1)[0]) else
                  (rpc_route(r) if r.request.method == "POST" and "rpc" in r.request.url else r.abort()))
        pg = ctx.new_page()
        errors = []
        pg.on("pageerror", lambda e: errors.append(str(e)))

        def xss():
            return pg.evaluate("window.__xss || []")

        # ---- live mode against the hostile node
        pg.goto(base + "#/"); pg.wait_for_selector("text=Flatline Watch", timeout=15000); pg.wait_for_timeout(800)
        check("watch list loads from the node", pg.locator(f'a[href*="{EVIL}"]').count() > 0)
        for tab in ["monitor", "activity", "verify", "lore"]:
            pg.goto(base + f"#/coin/{EVIL}/{tab}"); pg.wait_for_timeout(1200)
            pg.mouse.move(400, 400); pg.locator("body").hover()
            for el in pg.locator(".lore .k, .lore li").all()[:4]:
                el.hover()
            check(f"hostile coin {tab}: no injected script ran", xss() == [], str(xss()))
        html = pg.content()
        check("no javascript: link rendered", "href=\"javascript:" not in html.lower())
        pg.goto(base + f"#/coin/{EVIL}/monitor"); pg.wait_for_timeout(1000)
        hrefs = pg.eval_on_selector_all(".links a", "els => els.map(e => e.getAttribute('href'))")
        check("only plain https links shown", hrefs == ["https://fine.example"], str(hrefs))
        check("hostile icon not used as image src", pg.locator('img[src^="javascript"]').count() == 0)
        check("no page errors on hostile coin", not errors, "; ".join(errors[:3]))

        # ---- official vs copycat factories
        pg.goto(base + f"#/coin/{EVIL}/verify"); pg.wait_for_timeout(1200)
        t = pg.inner_text("#view")
        check("coin from the official factory passes the factory check", "Made by an official Revive factory" in t, t[:200])
        copy = "x.copycat-factory.testnet"
        pg.goto(base + f"#/coin/{copy}/verify"); pg.wait_for_timeout(1200)
        t = pg.inner_text("#view")
        check("copycat factory: coin page warns it isn't official", "Not an official Revive coin" in t, t[:200])
        check("copycat factory: Verify fails the factory check", "Not made by an official Revive factory" in t, t[:300])
        pg.goto(base + "?factory=copycat-factory.testnet#/launch"); pg.wait_for_timeout(900)
        check("launch form warns when a link points at another factory", "Not an official Revive factory" in pg.inner_text("#view"))
        pg.goto(base + "#/launch"); pg.wait_for_timeout(900)
        check("launch form has no warning on the official factory", "Not an official Revive factory" not in pg.inner_text("#view"))
        # ---- a connected token: coverage, the token's own keys, who connected it
        pg.goto(base + f"#/coin/{GUARD}/verify"); pg.wait_for_timeout(1500)
        t = pg.inner_text("#view")
        check("guard Verify shows how much of the supply it covers", "The guard covers 25% of the supply" in t, t[:400])
        check("guard Verify flags a token that still has keys", "The token's own team can still change it" in t)
        check("guard Verify says anyone can connect a token", "Anyone can connect any token" in t)

        # ---- a broken node: the page must say so, not show an empty list
        errors.clear()
        ctx.unroute("**/*")
        ctx.route("**/*", lambda r: r.continue_() if r.request.url.startswith(base.rsplit("/", 1)[0]) else
                  (r.fulfill(status=503, body="down") if r.request.method == "POST" else r.abort()))
        pg.goto(base + "#/"); pg.reload(); pg.wait_for_timeout(4000)
        txt = pg.inner_text("body")
        check("RPC outage never shows an empty coin list", ("Loading coins" in txt or "Can't reach NEAR" in txt) and "No coins" not in txt, txt[:200])
        pg.wait_for_timeout(6000)
        check("a long RPC outage says NEAR can't be reached", "Can't reach NEAR right now" in pg.inner_text("body"))

        # ---- simulator: the full story with no wallet
        errors.clear()
        pg.goto(base + "?demo#/coin/nine.revive.testnet/monitor"); pg.wait_for_timeout(1200)
        n = pg.locator(".steps button").count()
        for i in range(min(n, 11)):                       # steps 1-11: the full story (not "Alt" or "Reset")
            pg.locator(".steps button").nth(i).click(); pg.wait_for_timeout(300)
        check("simulator story has steps", n >= 8, f"{n} steps")
        pg.evaluate("location.hash = '#/coin/nine.revive.testnet/lore'"); pg.wait_for_timeout(900)  # same page: a reload resets the simulator
        lore = pg.inner_text("#view").lower()
        check("simulator story ends with the coin revived by a DAO", "revived" in lore and "sputnikv2" in lore, lore[:300])
        check("simulator: no page errors", not errors, "; ".join(errors[:3]))
        for page in ["#/", "#/launch", "#/connect", "#/how"]:
            errors.clear(); pg.goto(base + "?demo" + page); pg.wait_for_timeout(700)
            check(f"simulator page {page} renders", len(pg.inner_text("#view")) > 50 and not errors, "; ".join(errors[:2]))

        # ---- launch form rejects bad input before any wallet prompt
        pg.goto(base + "?demo#/launch"); pg.wait_for_timeout(700)
        name = pg.locator('input[name="name"]')
        name.fill("Bad Name!"); name.dispatch_event("input")
        v = name.input_value()
        check("launch form: account name is cleaned to a-z 0-9 - _", all(c.isdigit() or c in "abcdefghijklmnopqrstuvwxyz-_" for c in v), v)
        name.fill("a"); name.dispatch_event("input")
        check("launch form: a 1-letter name is refused", pg.evaluate("document.querySelector('input[name=\"name\"]').checkValidity()") is False)
        b.close()

    failed = [r for r in RESULTS if not r[1]]
    print(f"\n{len(RESULTS) - len(failed)} passed, {len(failed)} failed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
