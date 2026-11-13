# Revive internal security and functionality audit

November 2026. Scope: the coin, guard and factory contracts, the web app, the scripts and the CI
workflows in this repository, plus the factories and coins deployed on NEAR testnet.

This is an internal audit by the team, not an independent one. Everything marked PASS below was
run, and the commands are listed in section D. Anything that couldn't be run is marked UNVERIFIED
or NOT TESTED.

## A. Summary

Revive runs on testnet, and the contracts held up. Nothing found lets anyone move a coin's tokens
or treasury without the rules' authorization:

- only the keeper can pulse, spend or change links;
- only holders from before the death can vote, and only once;
- the creator's own account never votes;
- callbacks are private;
- every factory and every listed coin has no access keys, and its code matches its factory.

The real problems were around **trust**, not the contract logic:

1. **The Verify tab trusted any factory (fixed).** A copycat factory, deployed and locked by anyone,
   made coins that passed every Verify check. The app now pins Revive's own factories and their code
   hashes, and flags everything else.
2. **One earlier factory has an empty code slot (mitigated in the app; open on-chain).**
   `revive-bhtat` never stored coin code. Its owner account can still store code there once.
3. **A creator's second wallet can outvote holders (open, a design limit).** Vote weight belongs to
   accounts, and the contract can't tell that two accounts are one person.

Before any mainnet use, the open items in section F need decisions, and an independent audit is
needed.

## B. Findings

| ID | Severity | Location | What was wrong | Impact | Evidence | Fix | Status |
|---|---|---|---|---|---|---|---|
| R-01 | High | `web/index.html` `liveCoin().verify`, `verifyPanel` | Verify compared a coin with whatever factory its account name pointed to. | A copycat factory, locked and keyless, made coins showing "Same code as every Revive coin" and "The factory is locked". | Browser test serves a coin under `copycat-factory.testnet`. | `OFFICIAL` pins Revive's 4 factories and their code hashes, read from chain. New first check: "Made by an official Revive factory". Coin page banner: "Not an official Revive coin". | **Fixed**, verified (`site_test.py`: 3 checks) |
| R-02 | High | Factory `revive-bhtat.revive.testnet` (on chain) | It stored guard code but never coin code. `store_code` is owner-only and once-only, so the owner `revive.testnet` can still fill that slot. That account's key is in GitHub secrets. | Coins with any code could appear under a factory the site lists. Before the R-01 fix, they would have passed Verify. | Live read (CI `ops-check`): `get_code_hash` = `null` on `revive-bhtat`. | App: bhtat's coin hash is pinned as "none expected", so any coin from it fails Verify. On-chain: the slot can only be closed by storing code. | **Mitigated** (app). On-chain slot **Open** |
| R-03 | High (design) | `contract/src/lib.rs` `weight_at`, same in the guard | Weight belongs to accounts. A creator who moves liquid tokens to a second wallet before the death keeps that wallet's full weight. | The creator can vote a takeover down, which defeats the core promise for creators who plan ahead. | `contract/src/tests/known_limits.rs`: 500M in a second wallet beats 300M of honest yes votes. | None possible without identity. Documented in the README; the transfers are public and show on the Activity tab. | **Open**, documented |
| R-04 | Medium | `web/index.html` `FACTORY_ID`, `CONNECT_FACTORY` | `?factory=` and `?connect=` link parameters changed where Launch and Connect send NEAR, with no warning. | A phishing link could send a launch deposit (about 2.4 NEAR or more) to an attacker's contract. The wallet still shows the receiver. | Browser test with `?factory=copycat-factory.testnet`. | Launch and Connect show "Not an official Revive factory" for any unpinned factory. | **Fixed**, verified (2 checks) |
| R-05 | Medium | `guard/src/lib.rs` `ft_on_transfer`; factory `connect` | (a) The guard trusts its token contract. A token whose team holds keys can be upgraded, mint, or report transfers it never made. (b) Anyone can connect any token. (c) The guard governs only tokens inside it. | Holders could overrate how much a guard protects them. | Code review; live `meme-cia7y` guard. | Verify now shows the share of supply the guard covers, the token's access-key count, and who connected it. | **Mitigated** (UI). Contract limit **Open** by design |
| R-06 | Medium | `web/index.html` `setupWallet` | The wallet library (`@hot-labs/near-connect@0.12.0`) loads from esm.sh with no integrity check. | If the CDN were compromised, the code that builds transactions could change. | Code review. | Recommended: bundle it into the repo before mainnet. Left as is so the testnet wallet flow isn't changed a day before submission. | **Open** |
| R-17 | Medium | Build pipeline; `web/index.html` Verify text | The wasm isn't built reproducibly. In CI the coin built from this source hashes to `CR5qCUDA…`, while the deployed coins run `6Unt3cdM…`. The Verify tab told people to "compare with the hash printed by CI". | Nobody can confirm that the deployed coins run exactly this repository's code. The suggested comparison would fail and could mislead. | CI run on `audit-fixes`, sandbox-e2e log, against the live read. | Verify now says what the hash proves (every coin from the factory runs the same code) and that it can't be matched to the source yet. The real fix is `cargo near build reproducible-wasm` and redeploying a factory, which is a new deployment and needs the owner's OK. | **Mitigated** (wording). Root cause **Open** |
| R-07 | Low | `spend`, bond refunds (coin and guard) | A NEAR transfer to an account that doesn't exist bounces back to the contract's balance, but not to the `treasury` counter. | The NEAR stays in the contract, uncounted. Nobody can take it. | Code review. UNVERIFIED: the unit harness doesn't run promises, and the sandbox e2e has no such case. | Future version: re-credit in a callback. | **Open** (already in README) |
| R-08 | Low | `start_takeover` | One takeover at a time. A griefer can occupy the slot for one vote period. | Delay only. The griefer loses the 0.1 NEAR bond if the takeover fails. | Existing tests (`launch_rules.rs`). | None; documented. | **Open**, documented |
| R-09 | Low | `web/index.html` `watchView` | During an RPC outage the list said "Loading coins…" forever. | Confusing. Nothing was hidden, since the retry continued. | Browser test with every RPC returning 503. | After 3 failed tries it says "Can't reach NEAR right now. Still trying…". | **Fixed**, verified |
| R-10 | Low | Hosting | No security headers (clickjacking, MIME sniffing). | The page could be framed. The wallet still asks before signing. | Code review. | `web/vercel.json` adds `X-Frame-Options: DENY`, `frame-ancestors 'none'`, `nosniff`, `Referrer-Policy` and `Permissions-Policy`. GitHub Pages can't set headers. | **Fixed** for Vercel. UNVERIFIED until deployed |
| R-11 | Low | `.github/workflows/contract.yml`, `keep-alive.yml`, `testnet-demo.yml` | No explicit token permissions, so they used the repository default. | A compromised step would get more GitHub access than it needs. | Code review. | `permissions: contents: read`. | **Fixed**, verified by CI |
| R-12 | Low | `scripts/` dev dependency `tar` (through `near-sandbox`) | `npm audit`: 4 tar advisories, all denial of service on crafted archives. | CI only: the sandbox downloads and unpacks a NEAR binary. | `npm audit` before: 2 vulnerabilities (1 critical). | `overrides: tar ^7.5.23`. | **Fixed**: `npm audit` reports 0 |
| R-13 | Info | GitHub secrets | `revive.testnet` holds a full-access key used by the keep-alive and demo workflows. It owns every factory, which matters only for R-02. | Testnet funds; R-02. | Live read: 1 key. | For mainnet, a separate keeper with a function-call key limited to `pulse`. | Open, advisory |
| R-14 | Info | `contract/examples/simulate.rs` | With `--release` (panic = abort) the simulation died silently. | Confusing; the documented command was fine. | Ran both ways. | It now prints how to run it and exits 2. | **Fixed** |
| R-16 | Info | Rust dependencies (`cargo audit`) | 1 advisory and 2 "unmaintained" warnings, the same in all three crates. `crypto-bigint` (RUSTSEC-2026-0387, timing) and `unsafe-libyaml` (RUSTSEC-2024-0449) come in only through near-sdk's native test runtime (`near-vm-runner`, `near-parameters`), and `cargo tree --target wasm32-unknown-unknown -e normal` shows neither in the deployed wasm. `wee_alloc` (RUSTSEC-2022-0054, unmaintained) is near-sdk's default allocator and is in the wasm. | None for the deployed contracts from the first two. For wee_alloc, every NEAR call starts with fresh memory, so its known leak can't build up. | `cargo audit`, `cargo tree -i`. | None. Revisit when near-sdk changes its allocator. | Open, advisory |
| R-15 | Info | `web/index.html` `safeIcon` | `https` icons from connected tokens load from third-party hosts. | Those hosts see visitors' IP addresses. | Code review. | None. Coins launched on Revive store icons on-chain as data URLs. | Open, advisory |

**Checked and found safe** (evidence in D):

- Script injection through coin names, symbols, links, icons, history notes, DAO suffixes and token
  metadata: 9 browser checks, and no payload ran.
- Keeper-only `pulse`, `spend` and `set_links`; the spend cap; storage cannot be spent; only the
  creator claims vested tokens. Covered by unit tests.
- No double votes; tokens bought after the death get no vote; splitting or hopping wallets adds no
  weight. Covered by unit tests.
- Code is stored once and only by the owner; failed launches and connects are refunded, including
  tokens with malformed metadata; callbacks are `#[private]`. Covered by unit tests.
- On chain:
  - all 4 factories have 0 keys;
  - all 11 listed coins and guards have 0 keys;
  - every coin's code hash equals its factory's stored hash.
- No private keys in the git history: a scan of every commit found none.

## C. Functionality

| Feature | Expected | Test | Result |
|---|---|---|---|
| Flatline Watch list | Lists coins from every Revive factory, sorted by urgency | Browser test against the fake node | PASS |
| Coin page tabs (Monitor, Activity, Verify, Lore) | Each renders without errors, even for a hostile coin | Browser test, 4 tabs | PASS |
| Verify checks | Official coins pass; copycats and unknown factories fail | Browser test | PASS |
| Connected-token Verify | Shows coverage, the token's keys and who connected it | Browser test | PASS |
| Launch and Connect forms | Clean the account name; refuse a 1-letter name; warn on other factories | Browser test | PASS |
| RPC outage | Never shows an empty list; says when NEAR is unreachable | Browser test (503 on every RPC) | PASS |
| Simulator story, no wallet | Steps 1 to 11 end with the coin revived by a DAO | Browser test | PASS |
| Simulator pages (Watch, Launch, Connect, Guide) | Render with no page errors | Browser test | PASS |
| Revival on testnet ($NINE) | Flatline, whale refused, real DAO voted in, 100M burned | `testnet-demo` workflow, 12 Nov 2026 | PASS |
| DAO runs a coin on testnet ($CIVIC) | DAO pulses and spends by proposal; one council member can't spend | `testnet-demo` workflow, 12 Nov 2026 | PASS |
| Connected token on the current factory | Lock, stake, takeover, seal | Not re-run: about 0.05 testnet NEAR short. It passed on the earlier factory (`meme-cia7y`). | BLOCKED |
| Wallet sign-in and signing in the live site | The wallet prompt opens and signs | Needs a person with a wallet | NOT TESTED in this audit |
| 60-day simulation | Same results as published | `cargo run --example simulate`; `docs/simulation.json` came out byte for byte the same | PASS |

## D. Test execution

| Suite | Command | Discovered | Ran | Passed | Failed |
|---|---|---|---|---|---|
| Coin unit tests | `cd contract && cargo test` | 90 | 90 | 90 | 0 |
| Guard unit tests | `cd guard && cargo test` | 44 | 44 | 44 | 0 |
| Factory unit tests | `cd factory && cargo test` | 30 | 30 | 30 | 0 |
| Clippy, all three crates | `cargo clippy --all-targets -- -D warnings` | – | 3 | 3 clean | 0 |
| Browser tests | `python tests/browser/site_test.py` | 28 | 28 | 28 | 0 |
| Simulation | `cd contract && cargo run --example simulate` | 1 | 1 | 1 (output identical) | 0 |
| npm audit (scripts) | `cd scripts && npm audit --omit=dev` | – | 1 | 0 vulnerabilities after the fix | – |
| cargo audit | `cargo audit` in each crate | – | 3 | 1 advisory and 2 warnings, none affecting the deployed wasm except an unmaintained allocator (R-16) | – |

**New tests:**
- `contract/src/tests/known_limits.rs` (2 tests);
- `tests/browser/site_test.py` (28 checks), which also runs in CI as the `site` job.

CI on branch `audit-fixes` re-ran everything on GitHub's runner, and all jobs passed:

- `test-and-build`: unit tests, clippy, the simulation and the wasm build;
- `site`: the browser tests;
- `sandbox-e2e`: all 5 stories on a local NEAR sandbox, every check passing.

## E. Changes made

- `web/index.html`
  - Pinned official factories and their code hashes; a new "Made by an official Revive factory" check.
  - A banner on coins that aren't from an official factory.
  - Warnings on Launch and Connect for unofficial factory links.
  - Connected-token checks: coverage, the token's keys, who connected it.
  - An "earlier version" tag on coins from older factories.
  - An outage message.
- `web/vercel.json`: security headers.
- `.github/workflows/contract.yml`, `keep-alive.yml`, `testnet-demo.yml`: read-only GitHub token.
  `contract.yml` also gains a `site` job that runs the browser tests.
- `scripts/package.json`: patched `tar`.
- `contract/examples/simulate.rs`: explains a release-mode run instead of aborting silently.
- `contract/src/tests.rs`, `contract/src/tests/known_limits.rs`: the known-limit tests.
- `tests/browser/site_test.py`: the browser tests.
- `README.md`:
  - the live coins now point at the current factory;
  - new limits (second wallets, copycat factories, the empty slot, the wallet CDN);
  - test counts updated;
  - a link to this report.

No contract code changed. The deployed contracts are immutable, and no finding needed a new deployment.

## F. Remaining risks

1. **R-03, second wallets.** A design limit of token-weighted voting. Options for a future version:
   - lock the creator's liquid supply into a launch pool;
   - make quorum count only yes votes from accounts older than the launch;
   - keep it as is, and lean on the on-chain record.
2. **R-02, the empty slot on `revive-bhtat`.** The owner can close it for good by storing the genuine
   coin wasm there once. That is a testnet transaction by `revive.testnet`, needing the owner's OK.
   The app already flags any coin from that factory.
3. **R-06, the wallet library from a CDN.** Bundle it before mainnet.
4. **R-17, the build isn't reproducible.** The deployed code can't be matched to the source until the wasm is built reproducibly and a factory is redeployed from that build.
5. **R-05, connected tokens.** A guard is only as trustworthy as its token, so the app now shows the
   token's keys and the guard's coverage.
6. **R-07, uncounted bounced transfers.** Fix in the next contract version.
7. **Not tested here:** wallet signing in a real browser, and the connected-token story on the
   current factory.

## G. Readiness decision

**NOT READY FOR DEPLOYMENT** with real money on mainnet. Three reasons:

- a High design risk (R-03) remains open;
- a High on-chain issue (R-02) is only mitigated in the app;
- the deployed code can't yet be matched to the source (R-17);
- there has been no independent audit.

As a **testnet demonstration**, everything tested works. The fixes above remove the ways the app
could mislead someone about which coins are genuinely Revive's.
