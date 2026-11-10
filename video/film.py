"""Films every scene of the Revive demo as its own clip (Playwright, 1280x720).
Live scenes read NEAR testnet; simulator scenes run the same rules in the browser (?demo)."""
import asyncio, json, sys, time
from playwright.async_api import async_playwright

BASE = "http://localhost:8080"
SITE = BASE + "/web/index.html"
VP = {"width": 1280, "height": 720}
CIVIC = "civic-vft8z.revive-ztw7t.revive.testnet"
MEME = "meme-cia7y.revive-bhtat.revive.testnet"
ONLY = sys.argv[1:]
TIMES = {}

OVERLAY = r"""
window.addEventListener('DOMContentLoaded', () => {
  const c = document.createElement('div'); c.id = '__cur';
  c.style.cssText = 'position:fixed;z-index:99999;left:-50px;top:-50px;width:22px;height:22px;margin:-3px 0 0 -3px;pointer-events:none;transition:transform .08s';
  c.innerHTML = '<svg width="22" height="22" viewBox="0 0 22 22"><path d="M3 2 L3 18 L7.5 14 L10.5 20.5 L13 19.4 L10.1 13 L16 13 Z" fill="#0B0F19" stroke="#fff" stroke-width="1.4" stroke-linejoin="round"/></svg>';
  document.body.appendChild(c);
  document.addEventListener('mousemove', e => { c.style.left = e.clientX + 'px'; c.style.top = e.clientY + 'px'; }, true);
  document.addEventListener('mousedown', () => c.style.transform = 'scale(.82)', true);
  document.addEventListener('mouseup', () => c.style.transform = '', true);
  const cap = document.createElement('div'); cap.id = '__cap';
  cap.style.cssText = 'position:fixed;z-index:99998;left:50%;top:100px;transform:translateX(-50%);white-space:nowrap;background:rgba(11,15,25,.93);color:#fff;font:600 21px "DM Sans",system-ui,sans-serif;padding:12px 24px;border-radius:12px;opacity:0;transition:opacity .35s;box-shadow:0 8px 28px rgba(0,0,0,.2)';
  document.body.appendChild(cap);
  window.__say = (t) => { cap.style.opacity = t ? 1 : 0; if (t) cap.innerHTML = t; };
  const b = document.createElement('div'); b.id = '__badge';
  b.style.cssText = 'position:fixed;z-index:99997;left:20px;bottom:18px;font:600 13px "IBM Plex Mono",monospace;letter-spacing:1.2px;padding:7px 12px;border-radius:999px;color:#fff;display:none';
  document.body.appendChild(b);
  window.__badge = (live) => { b.style.display = 'block'; b.style.background = live ? '#15803D' : '#B45309'; b.textContent = live ? '● LIVE ON NEAR TESTNET' : '◆ SIMULATOR · SAME RULES, IN YOUR BROWSER'; };
});
"""

class Scene:
    def __init__(self, pg): self.pg = pg
    async def say(self, t, hold=0):
        await self.pg.evaluate("t => window.__say && window.__say(t)", t)
        if hold: await self.pg.wait_for_timeout(int(hold * 1000))
    async def badge(self, live): await self.pg.evaluate("l => window.__badge && window.__badge(l)", live)
    async def to(self, sel, click=True, steps=28, dx=None):
        for attempt in range(4):          # pages that refresh themselves can swap the element out
            try:
                el = self.pg.locator(sel).first
                await el.scroll_into_view_if_needed(timeout=4000); await self.pg.wait_for_timeout(250)
                bb = await el.bounding_box()
                if bb: break
            except Exception:
                await self.pg.wait_for_timeout(400)
        x = bb["x"] + (dx if dx is not None else min(46, bb["width"] / 2)); y = bb["y"] + bb["height"] / 2
        await self.pg.mouse.move(x, y, steps=steps)
        if click:
            await self.pg.wait_for_timeout(180); await self.pg.mouse.down(); await self.pg.mouse.up()
    async def scroll(self, y): await self.pg.evaluate(f"window.scrollTo({{top:{y},behavior:'smooth'}})"); await self.pg.wait_for_timeout(900)
    async def wait(self, s): await self.pg.wait_for_timeout(int(s * 1000))

async def card(sc, name, secs):
    await sc.pg.goto(f"{BASE}/video/cards.html?s={name}"); await sc.pg.wait_for_load_state("networkidle"); await sc.wait(secs)

async def s_title(sc): await card(sc, "title", 6.5)
async def s_problem(sc): await card(sc, "problem", 10)
async def s_idea(sc): await card(sc, "idea", 12)
async def s_trust(sc): await card(sc, "trust", 11)
async def s_close(sc): await card(sc, "close", 7)

async def s_watch(sc):
    pg = sc.pg
    await pg.goto(SITE + "#/"); await pg.wait_for_selector("a.coinrow", timeout=60000); await sc.wait(1.2)
    await sc.badge(True); await pg.mouse.move(640, 360)
    await sc.say("Flatline Watch: every Revive coin, read live from NEAR", 2.8)
    await sc.scroll(330)
    await sc.say("Green coins have a pulse. Red ones missed their deadline", 0)
    for sel in ["a.coinrow >> text=Still Here", "a.coinrow >> text=Back From The Dead", "a.coinrow >> text=Civic Heart"]:
        try: await sc.to(sel, click=False, steps=22); await sc.wait(1.4)
        except Exception as e: print("watch hover", sel, e)
    await sc.wait(1.2)

async def s_launch(sc):
    pg = sc.pg
    await pg.goto(SITE + "?demo#/launch"); await pg.wait_for_selector("form#launch"); await sc.badge(False)
    await pg.mouse.move(700, 300); await sc.wait(0.6)
    await sc.say("Anyone can launch a coin", 1.6)
    async with pg.expect_file_chooser() as fc: await sc.to(".pick")
    await (await fc.value).set_files("video/coin.png"); await sc.wait(0.8)
    await sc.to("input[name=name]"); await pg.keyboard.type("pulsecat", delay=60)
    await sc.to("input[name=cname]"); await pg.keyboard.type("Pulse Cat", delay=55)
    await sc.to("input[name=symbol]"); await pg.keyboard.type("PCAT", delay=80)
    await sc.say("The rules are fixed at launch, and nobody can change them later", 0)
    await sc.to("input[name=pulse]", click=False); await sc.wait(1.6)
    await sc.say("Pulse every 7 days, or the coin flatlines", 0)
    await sc.wait(1.6)
    await sc.to("input[name=dev]", click=False)
    await sc.say("The dev's 10% stays locked, vesting only while they pulse", 2.6)
    await sc.to("input[name=quorum]", click=False)
    await sc.say("10% of before-death holders must vote yes to take over", 2.6)
    await sc.to("form#launch button.primary"); await sc.wait(1.2)
    await sc.scroll(0); await pg.mouse.move(700, 320, steps=14)
    await sc.say("It's live, with a heartbeat", 3)

async def s_story(sc):
    pg = sc.pg
    await pg.goto(SITE + "?demo#/"); await pg.wait_for_selector("a.coinrow"); await sc.badge(False)
    await pg.mouse.move(640, 360); await sc.to("a.coinrow >> text=Nine Lives"); await pg.wait_for_selector(".steps button"); await sc.wait(0.8)
    steps = pg.locator(".steps button")
    async def step(i, caption, hold, view="top"):
        await sc.to(f".steps button >> nth={i}", click=False, steps=18)
        await sc.say(caption, 0); await sc.wait(0.5)
        await pg.mouse.down(); await pg.mouse.up(); await sc.wait(0.5)
        if view == "top": await sc.scroll(0)
        elif view == "vote":
            y = await pg.evaluate("(()=>{const h=[...document.querySelectorAll('#view h2')].find(x=>x.textContent.trim()==='Takeover'); return h ? h.getBoundingClientRect().top + scrollY - 150 : 0})()")
            await sc.scroll(y)
        await sc.wait(hold)
    await sc.say("Nine Lives: let's kill a coin and bring it back", 2.4)
    await step(0, "1 · The dev pulses. The coin is alive", 2.4)
    await step(1, "2 · The dev goes silent for 3 days. FLATLINE", 3.2)
    await step(2, "3 · A whale buys 250M tokens after the death", 2.4, view="none")
    await step(3, "4 · Alice, an early holder, opens a takeover for her DAO", 2.8, view="vote")
    await step(4, "5 · Alice votes yes", 2.0, view="vote")
    await step(5, "6 · The whale tries to vote no, and is refused: no weight from after the death", 3.6, view="vote")
    await step(6, "7 · Bob votes yes", 1.8, view="vote")
    await step(7, "8 · Carol votes no", 1.8, view="vote")
    await step(8, "9 · The vote closes", 1.8, view="vote")
    await step(9, "10 · Finalize: REVIVED. The dev's locked tokens burn, the DAO is keeper", 4.2)
    await step(10, "11 · The DAO keeps the heartbeat going, by proposal", 3.2)

async def s_comeback(sc):
    pg = sc.pg
    await pg.goto(SITE + "?demo#/"); await pg.wait_for_selector("a.coinrow"); await sc.badge(False)
    await sc.to("a.coinrow >> text=Nine Lives"); await pg.wait_for_selector(".steps button")
    await sc.say("The other ending: the dev comes back", 0)
    for i in (0, 1, 3):
        await pg.locator(f".steps button >> nth={i}").click(); await sc.wait(0.35)
    await sc.scroll(0); await sc.wait(1.6)
    await sc.say("A takeover is open. Then the dev returns, late", 2.4)
    await sc.to(".steps button >> nth=11", click=False, steps=18); await sc.wait(0.4)
    await pg.mouse.down(); await pg.mouse.up(); await sc.wait(0.5); await sc.scroll(0)
    await sc.say("They pay a 2 NEAR penalty to the treasury, and the vote is cancelled", 4)

async def coin_live(sc, cid, tab=""):
    await sc.pg.goto(SITE + f"#/coin/{cid}{'/' + tab if tab else ''}")
    await sc.pg.wait_for_function("document.querySelector('#view') && document.querySelector('#view').innerText.includes('Verify')", timeout=60000)

async def s_civic(sc):
    pg = sc.pg
    await coin_live(sc, CIVIC); await sc.badge(True); await pg.mouse.move(640, 300); await sc.wait(1.5)
    await sc.say("This really happened on testnet: Civic Heart", 2.6)
    await sc.say("Its keeper is now a Sputnik DAO that holders voted in", 0)
    await sc.to("#view .row >> text=Keeper", click=False, dx=400); await sc.wait(2.6)
    await sc.scroll(0); await sc.to("#view .tabs >> text=Lore"); await sc.wait(1.8)
    await sc.say("Its on-chain history: flatline, takeover, votes, revival", 0); await sc.scroll(260); await sc.wait(2.6)
    await sc.say("Then the DAO pulsed and paid a holder, each by proposal", 0); await sc.scroll(520); await sc.wait(2.8)
    await sc.scroll(0); await sc.to("#view .tabs >> text=Verify"); await sc.wait(3.0)
    await sc.say("Verify reads it straight from NEAR: no keys, same code, locked factory", 0); await sc.scroll(300); await sc.wait(3.0)
    await sc.say("…and the keeper really is a Sputnik DAO", 0); await sc.scroll(640); await sc.wait(3.4)

async def s_meme(sc):
    pg = sc.pg
    await coin_live(sc, MEME); await sc.badge(True); await pg.mouse.move(640, 300); await sc.wait(1.4)
    await sc.say("Coins launched elsewhere can get the same rules", 2.6)
    await sc.say("Meme Cooked is a plain NEP-141 token with a Revive guard next to it", 3)
    await sc.say("Holders staked 800M in the guard to get a vote; the token itself is untouched", 0)
    await sc.to("#view >> text=STAKED", click=False, steps=20); await sc.wait(3)
    await sc.to("#view .tabs >> text=Activity"); await sc.wait(1.5)
    await sc.say("Its activity, with the flatline and takeover marked", 0); await sc.scroll(330); await sc.wait(3.4)

SCENES = [("01_title", s_title, False), ("02_problem", s_problem, False), ("03_idea", s_idea, False), ("04_watch", s_watch, True),
          ("05_launch", s_launch, True), ("06_story", s_story, True), ("07_comeback", s_comeback, True), ("08_civic", s_civic, True),
          ("09_meme", s_meme, True), ("10_trust", s_trust, False), ("11_close", s_close, False)]

async def main():
    async with async_playwright() as p:
        b = await p.chromium.launch()
        for name, fn, overlay in SCENES:
            if ONLY and name not in ONLY: continue
            ctx = await b.new_context(viewport=VP, device_scale_factor=1, record_video_dir=f"out/{name}", record_video_size=VP)
            pg = await ctx.new_page()
            if overlay: await pg.add_init_script(OVERLAY)
            t0 = time.time()
            try: await fn(Scene(pg))
            except Exception as e: print("SCENE FAILED", name, repr(e)); await pg.screenshot(path=f"out/{name}-fail.png")
            await pg.screenshot(path=f"out/{name}-last.jpg", type="jpeg", quality=90)
            TIMES[name] = round(time.time() - t0, 2)
            await ctx.close()
            print(name, TIMES[name], flush=True)
        await b.close()
    json.dump(TIMES, open("out/times.json", "w"), indent=1)

asyncio.run(main())
