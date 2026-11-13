# Revive

**Onchain CTO for NEAR memecoins.**

Revive is a NEAR memecoin with a community takeover built into the contract.
The keeper has to send an on-chain **pulse** before every deadline. If the pulse
stops, the coin **flatlines**. Holders who held *before* the flatline can then
vote a Sputnik DAO in as the new keeper. If the vote passes, the creator's
locked tokens are **burned**, the treasury and the coin's links pass to the
DAO, and the coin gains a **life**. Every death and revival is written to
on-chain lore.

> Built at NEAR HACKATHON (November 2026).
> Tracks: **Memecoins With Mechanics** (primary), **Social Money** (Sputnik DAO as the steward).

**Live on NEAR testnet:** [mhiah.github.io/Revive](https://mhiah.github.io/Revive/) reads the current factory
[`revive-5svgh.revive.testnet`](https://testnet.nearblocks.io/address/revive-5svgh.revive.testnet) (locked, no keys) and its coins over RPC.
Both stories below ran on this factory's code, which has every fix and launch limit described under [Tests](#tests):

* [`nine.revive-5svgh.revive.testnet`](https://testnet.nearblocks.io/address/nine.revive-5svgh.revive.testnet) ($NINE): flatlined, a whale's post-death vote refused, holders voted in the Sputnik DAO [`civic-vft8z.sputnikv2.testnet`](https://testnet.nearblocks.io/address/civic-vft8z.sputnikv2.testnet); 100M creator tokens burned and the old dev can no longer pulse.
* [`civic-sl257.revive-5svgh.revive.testnet`](https://testnet.nearblocks.io/address/civic-sl257.revive-5svgh.revive.testnet) ($CIVIC), a takeover by a **real Sputnik DAO**: its dev went silent, holders voted in the same DAO (council: two holders), and the DAO then ran the coin by proposal: it pulsed $CIVIC, then paid a contributor 0.05 NEAR from the treasury. A council member acting alone can't spend.

**Earlier factories.** Every factory is locked, so coins keep the code they were born with. The site still lists the
coins from earlier factories, tagged **earlier version**:
[`revive-ztw7t`](https://testnet.nearblocks.io/address/revive-ztw7t.revive.testnet) (the first $NINE, $BACK where the dev came back mid-vote and paid the penalty, and the first $CIVIC),
[`revive-6tx39`](https://testnet.nearblocks.io/address/revive-6tx39.revive.testnet) ($HERE, below) and
[`revive-bhtat`](https://testnet.nearblocks.io/address/revive-bhtat.revive.testnet), which made the connected-token guards:
[`meme-cia7y.revive-bhtat.revive.testnet`](https://testnet.nearblocks.io/address/meme-cia7y.revive-bhtat.revive.testnet) guards the plain NEP-141 [`meme-cia7y.revive.testnet`](https://testnet.nearblocks.io/address/meme-cia7y.revive.testnet): the dev locked 100M, holders staked, the dev went silent, a whale's post-death stake got no vote, holders took over and the dev's 100M is sealed in the guard.
Coins from the first two factories keep the quorum count from before the fixes, and none of the earlier coins has the
launch limits; the **Verify** tab checks each coin's own settings and says so.

**A coin that stays alive:** [`here.revive-6tx39.revive.testnet`](https://testnet.nearblocks.io/address/here.revive-6tx39.revive.testnet)
($HERE, "Still Here") has a 3-day heartbeat. The [keep-alive workflow](.github/workflows/keep-alive.yml)
pulses it twice a day, so Flatline Watch always shows a healthy coin next to the revived ones.

All stories were played by the [testnet-demo workflow](.github/workflows/testnet-demo.yml) with 60-second timers,
so every story coin except $HERE has since flatlined again: anyone holding from before can open a takeover.

**Already launched somewhere else?** Any NEP-141 token can be **connected**: Revive deploys a guard next to it
with the same heartbeat and takeover rules ([below](#connect-a-coin-launched-elsewhere)).

**No wallet?** [mhiah.github.io/Revive/?demo](https://mhiah.github.io/Revive/?demo) runs a simulator with the same rules.
Open **Nine Lives** and click the story buttons in order.

## Judge quickstart

| Do this | You should see |
| --- | --- |
| Open [mhiah.github.io/Revive](https://mhiah.github.io/Revive/) | **Flatline Watch** read live from NEAR testnet: `$HERE` alive, the revived `$NINE` and `$CIVIC`, `$BACK`, and the connected `$MEME` (tagged *connected*), each with its pulse, shelf life and trend |
| Click **Nine Lives** → **Verify** | Checks read from RPC: made by an official Revive factory (pinned in the app), no access keys on the coin, its code hash equals the factory's, the factory has no keys, treasury backed, supply adds up (100M burned), settings can't switch the takeover off, and the keeper is a Sputnik DAO |
| Click **Lore** | The coin's on-chain history: born, flatline, takeover, revived |
| Click **Activity** | Token transfers over time, every pulse and vote marked, flatlined stretches shaded red |
| In the simulator, open **Meme Cooked** (tagged *connected*) | A token launched elsewhere: stake, skip time, let the whale buy and stake after the death (no vote), take it over, then **Verify** shows the guard holds every staked and sealed token |
| Open [mhiah.github.io/Revive/?demo](https://mhiah.github.io/Revive/?demo) → **Nine Lives**, click steps 1–11 | The full story with no wallet: a whale's post-death vote is refused, the coin revives, the DAO pulses |
| `cd contract && cargo test` | `90 passed` (plus `guard`: `44 passed`, `factory`: `30 passed`): [what each group proves](#tests) |
| `python tests/browser/site_test.py` | `28 passed, 0 failed`: the site in Chrome against a fake NEAR node serving a hostile coin ([details](#tests)) |
| `cd contract && cargo run --example simulate` | The [60-day simulation](#60-day-simulation): six scenarios' day-60 results printed; `docs/simulation.json` rewritten |
| Latest [CI run](https://github.com/Mhiah/Revive/actions/workflows/contract.yml) → **sandbox-e2e** | All five stories on a local NEAR node, including connecting an outside token and a takeover by a real Sputnik DAO, every step checked (`All checks passed`) |
| [testnet-demo run](https://github.com/Mhiah/Revive/actions/workflows/testnet-demo.yml) | The same stories on NEAR testnet, against the coins above |
| Read [docs/AUDIT.md](docs/AUDIT.md) | The internal security and functionality audit: every finding, what was fixed, what is still open, and the test evidence |

## Why

When a memecoin's dev walks away, holders often run a *community takeover*
(CTO): they open a new Telegram, a new X account and carry on. It is one of the
best-known rituals in memecoin culture
([crypto.news](https://crypto.news/what-is-a-community-takeover-cto-when-a-memecoins-holders-seize-the-wheel/)),
and some coins, such as Billy, survived being abandoned this way
([Decrypt](https://decrypt.co/241846/how-solana-meme-coin-billy-survived-being-abandoned-by-its-dev)).

But today a CTO happens entirely off-chain. The contract, the dev's tokens and
any treasury stay where they were, so the takeover can be "mostly cosmetic"
([DEXTools](https://www.dextools.io/tutorials/what-is-a-cto-in-crypto-guide-2026)).
Revive turns the CTO into contract rules.

## The mechanic

| Step | What happens | Where |
| --- | --- | --- |
| **Pulse** | The keeper calls `pulse` before `last_pulse + pulse_interval`. Free when on time. | `pulse` |
| **Flatline** | No pulse by the deadline: status becomes `flatlined`. The keeper can no longer spend the treasury, claim vested tokens or change links. | `status`, `assert_alive` |
| **Takeover** | Any holder with weight nominates a Sputnik DAO (`*.sputnikv2.testnet` / `*.sputnik-dao.near`) as steward. A vote opens. | `start_takeover` |
| **Vote** | Weight = token-seconds held **before the flatline**. Keeper and creator cannot vote. | `vote`, `weight_at` |
| **Comeback** | During the vote the old keeper can still pulse, but must pay `comeback_penalty` into the treasury. The vote is cancelled. Once the vote closes, comebacks are blocked. | `pulse` |
| **Revival** | Yes > No and Yes ≥ quorum: the DAO becomes keeper, the creator's unvested tokens are burned, `lives += 1`. | `finalize_takeover` |
| **Lives** | A DAO keeper can flatline too. The cycle repeats, and the coin keeps counting lives. | `lives`, `get_lore` |

Creator tokens (at most 10% of supply) are locked in the contract and vest only
while the creator is alive and still keeper. The keeper can spend treasury NEAR
only within a per-period cap, and every spend is written to the lore.

### Why buying votes after the death doesn't work

Each holder stores `since`: the balance-weighted average time their tokens
arrived. Vote weight is `balance × (flatline_at − since)`.

* A wallet that buys after the flatline has `since > flatline_at`, so its weight is **0**.
* Moving tokens into an older wallet after the flatline pulls that wallet's `since` forward. The new tokens *subtract* weight instead of adding it.
* Voting and then sending your tokens to a second wallet gives the second wallet nothing.
* Averages round **up**, so rounding never makes tokens look older.
* Refunds from `ft_transfer_call` (for example a DEX returning unused tokens) restore the sender's own `since`, so using a DEX costs no hold time.

This matters: in 2026 an attacker drained roughly $20M from BonkDAO after
buying just over 1% of supply to pass a malicious proposal
([Crypto Daily](https://cryptodaily.co.uk/2026/07/are-memecoins-dead-liquidity)).

The quorum compares Yes weight with **all** eligible weight. The contract keeps
a running `Σ balance × since`, so the total is one multiplication, whether the
coin has 10 holders or 10 million.

## Connect a coin launched elsewhere

Most memecoins already exist: on meme.cooking, another launchpad, or a contract
someone wrote. Revive doesn't ask anyone to migrate. **Connect a coin** gives an
existing NEP-141 token the same CTO rules without touching the token:

| Step | What happens | Where |
| --- | --- | --- |
| **Connect** | Anyone holding the token calls the factory with its account. The factory reads `ft_metadata` (name, symbol, decimals), deploys a **guard** at `<name>.<factory>` with no keys, and registers the guard on the token. The caller becomes creator and first keeper. | `factory.connect` |
| **Lock the bag** | The creator sends tokens to the guard with `ft_transfer_call(msg: "lock")`. They vest only while the creator keeps pulsing. | `ft_on_transfer` |
| **Stake** | Holders send tokens with `msg: "stake"`. Stake carries the same `since` clock as a Revive coin, so weight is stake × time staked before the flatline. `unstake` returns tokens at any time. | `ft_on_transfer`, `unstake` |
| **Pulse, flatline, takeover, comeback** | Same rules and the same lore as a Revive coin. | `pulse`, `start_takeover`, `vote`, `finalize_takeover` |
| **Revival** | The DAO becomes keeper. The token can't be burned by someone else's contract, so the creator's unvested bag is **sealed** in the guard forever: it can never be claimed or moved. | `finalize_takeover` |

Anything other than `lock` or `stake` is refused and refunded by the token. A
failed `unstake` or `claim_vested` transfer is rolled back with the holder's
original `since`. **Verify** reads the token's own `ft_balance_of(guard)` and
checks it covers staked + locked + sealed.

## 60-day simulation

The track's example prompt asks to "simulate 60 days of community behavior first and show supply
over time under a few scenarios." [`contract/examples/simulate.rs`](contract/examples/simulate.rs) does that
with the **real contract**, not a model: every pulse, buy, vote and takeover is a call into `Revive`
under a mocked NEAR runtime with a moving clock. 12 holders, daily buys and tips, a keeper who pulses
every other day until they don't. Parameters: 1B supply, 10% creator allocation vesting over 90 days,
3-day pulse, 1-day vote, 20% quorum, 2 NEAR comeback penalty.

![Supply over 60 days in six scenarios, with alive, voting and flatlined days marked under each line](docs/simulation.png)

| Scenario | Day 60 | Lives | Supply | Creator tokens burned | Treasury |
| --- | --- | --- | --- | --- | --- |
| Dev stays | alive | 0 | 1,000M | 0 | 12.0 NEAR |
| Dev quits on day 12, holders revive | alive | 1 | 907.2M | 92.8M | 12.5 NEAR |
| Whale buys 300M after the death | alive (whale's vote refused) | 1 | 907.2M | 92.8M | 12.5 NEAR |
| Dev comes back mid-vote | alive | 0 | 1,000M | 0 | 18.5 NEAR (+2 penalty) |
| The steward DAO dies too | alive | 2 | 907.2M | 92.8M | 13.0 NEAR |
| Holders vote no | flatlined | 0 | 1,000M | 0 | 16.6 NEAR (includes the failed takeover's 0.1 NEAR bond) |

What it shows: supply only ever falls, and only by the creator's *unvested* tokens on the first revival
(the 7.2M the dev vested while alive stay theirs). A whale arriving after the death changes nothing. A comeback is
possible but costs the dev, and the penalty stays with holders. Each extra life costs nothing to supply.
Full day-by-day data: [docs/simulation.json](docs/simulation.json).

### Picking the parameters

The same run then sweeps the two settings that matter most, again on the real contract
(`sweeps` in [docs/simulation.json](docs/simulation.json)).

**Quorum.** Each row is a revival vote where only some holders turn up. "Yes" is their share of the
eligible (pre-death, hold-time weighted) total.

| Who votes yes | Their weight | 10% quorum | 20% quorum | 35% quorum |
| --- | --- | --- | --- | --- |
| The 2 smallest holders | 0.9% | fails | fails | fails |
| 3 mid-size holders (a quarter of holders) | 8.2% | fails | fails | fails |
| Half of holders | 17.6% | **passes** | fails | fails |

**Heartbeat.** A keeper who remembers to pulse on half of all days, over 60 days, 20 runs each.

| Pulse every | Accidental deaths | Days spent dead by accident | Holders wait after a real quit |
| --- | --- | --- | --- |
| 1 day | 15.4 | 31.4 | 1 day |
| 3 days | 4.2 | 8.6 | 3 days |
| 7 days | 0.4 | 0.6 | 7 days |

What we picked:

* **Quorum 10% for most coins.** A handful of small or mid-size wallets can't take a coin over, but
  half of the real holders can. At 20% even half of holders fail, because tokens in pools and in
  wallets that never vote still count in the total. Raise it only for coins with few, active holders.
* **7-day pulse for a keeper who pulses by hand**: under one accidental death in 60 days, and holders
  wait a week at most. **3 days when a bot pulses**, like [$HERE](#revive), which pulses twice a day
  from a scheduled workflow, so missed days aren't a risk and holders wait less.
* **1-day vote, 2 NEAR comeback penalty** stay as in the scenarios: long enough to vote, and a
  comeback costs the dev something that stays with holders.

Both are fixed at launch; nobody can change them later. The scenarios above use 20% so the attack
cases are tested against a stricter bar.

## The app

| Page | What it shows |
| --- | --- |
| **Flatline Watch** | A table of every coin and connected token: its pulse status, its shelf life (time left before it flatlines, with a bar), and a trend arrow (▲ healthy, ▼ fading, dead or in a vote). Open takeovers come first, then dead coins, then whoever is closest to death. |
| **Launch** | A form that calls the factory: coin image, name, supply, locked share, pulse interval, vote length, quorum, penalty, treasury seed and links. The image is cropped square, shrunk to 96×96 and stored in the coin as its NEP-148 icon (at most 4 KB), so wallets and explorers show it. Defaults follow [Picking the parameters](#picking-the-parameters): 7-day pulse, 10% quorum. |
| **Connect a coin** | Paste a token account: the app checks it answers `ft_metadata`, shows its supply and your balance, and connects it. The coin page then has **Lock your bag** for the creator and **Stake / Unstake** for holders. |
| **Coin · Monitor** | A live heart monitor (green alive, red flatlined, amber voting), countdown, lives, treasury, the coin's links, the takeover vote and the keeper. The keeper can pulse and change the coin image; the creator can claim vested tokens while they keep the coin alive. |
| **Coin · Activity** | One time axis: token transfers per bucket, every pulse, vote, takeover and revival marked above, and every stretch without a pulse shaded as flatlined. Hover for a bucket's details; a table view is one click away. |
| **Coin · Verify** | Checks read live from NEAR RPC: no access keys on the coin, same code hash as the factory (with the hex to compare against CI), factory locked, treasury backed, supply adds up (for a connected coin: the token says the guard holds everything it owes), keeper is a DAO. |
| **Coin · Lore** | Every birth, death, takeover, comeback and revival, from the contract. |

State is read directly over RPC; there is no backend. The **Activity** chart also needs past
transactions, which RPC nodes don't keep, so it reads them from FastNEAR's public transactions API
and counts the token's own NEP-141 `ft_transfer` events. If that API is unavailable, the chart falls
back to the contract's lore and current deadline and says so.

## How we used NEAR

| NEAR feature | Use in Revive |
| --- | --- |
| NEP-141 / 145 / 148 / 297 | Full fungible token with storage registration, metadata and events (`nep141` plus a `revive` event standard). |
| Factory sub-accounts | `revive.<you>` launches every coin as `<name>.revive.<you>` with the same stored code. Coin accounts are created with no keys. |
| Sputnik DAO | A revived coin's keeper is a Sputnik DAO. The DAO pulses, spends and updates links through FunctionCall proposals ([scripts/dao-pulse.md](scripts/dao-pulse.md)). |
| Named accounts | Stewards are checked by account suffix: only accounts created by the Sputnik factory can be nominated. |
| Access keys | Coins never get a key; the factory's key is deleted after the code is stored, so nobody can change how coins are made. |
| `ft_transfer_call` | Works with Rhea pools and other receivers; refunds keep hold time. |

## Run it

### Tests

**164 unit tests, no network needed.** One command per contract:

```bash
(cd contract && cargo test)   # 90 tests, ~70 s (the random histories take most of it)
(cd guard && cargo test)      # 44 tests
(cd factory && cargo test)    # 30 tests
```

**The site in a real browser:** `python tests/browser/site_test.py` (needs `pip install playwright` and Chrome) opens the
site against a fake NEAR node that serves a hostile coin, with script payloads in its name, symbol, links, icon and
history, and checks that none of them runs; that a coin from a copycat factory is flagged; that a connected token's
coverage and keys are shown; that an RPC outage is reported; and that the simulator's whole story plays. 28 checks; CI runs it.

Each group proves one kind of claim:

| Group | File | Tests | What it proves |
| --- | --- | --- | --- |
| Lifecycle | [contract/src/tests.rs](contract/src/tests.rs) | 21 | Launch, pulse, flatline, takeover, revival, comeback, two deaths and two revivals, DEX refunds keep hold time, only the factory names a creator |
| Attacks | [contract/src/tests/attacks.rs](contract/src/tests/attacks.rs) | 19 | Buying after the death neither votes nor moves the quorum; splitting or hopping wallets adds no weight; no double votes; the creator can't vote, start a takeover or move locked tokens; lookalike DAO names are refused |
| Timing | [contract/src/tests/timing.rs](contract/src/tests/timing.rs) | 12 | The exact second each deadline flips: pulse due, vote closing, finalizing, a last-second comeback; ties fail; takeovers back to back; a revival restarts the clock |
| Money | [contract/src/tests/money.rs](contract/src/tests/money.rs) | 21 | Spending exactly the cap, one yocto over, the reset; vesting is linear and never overpays; a revival burns only what wasn't claimed; supply always adds up |
| Launch rules | [contract/src/tests/launch_rules.rs](contract/src/tests/launch_rules.rs) | 12 | Settings that could switch the takeover off are refused (no vesting, a free comeback, a heartbeat over 30 days, oversized name/icon/links); opening a takeover needs the bond; votes, finalizing and new icons pay for their storage; bonds go back on a pass or a comeback and to the treasury on a fail; spending can't touch the NEAR that pays for storage |
| Known limits | [contract/src/tests/known_limits.rs](contract/src/tests/known_limits.rs) | 2 | Written down as tests so they stay visible: a creator's second wallet that held before the death can outvote honest holders; the creator's own account never has weight |
| Random histories | [contract/src/tests/invariants.rs](contract/src/tests/invariants.rs) | 3 | About 7,000 seeded random steps (transfers, pulses, comebacks, claims, spends, takeovers, votes). After every step: balances add up to supply, the quorum total equals a holder-by-holder sum, no weight beats its ceiling, the treasury matches every deposit and spend |
| Connected coins | [guard/src/tests.rs](guard/src/tests.rs) and [guard/src/tests/](guard/src/tests) | 44 | Lock and stake only through the real token; late stake buys no vote and doesn't move the quorum; a DAO that staked before its election doesn't inflate the next quorum; failed unstakes and claims roll back, always; a later lock vests from when it arrives; a trillion-token, 24-decimal coin fits the maths; stakers pay for their entry; random histories where the guard always holds exactly what it owes |
| Factory | [factory/src/lib.rs](factory/src/lib.rs) and [factory/src/tests_more.rs](factory/src/tests_more.rs) | 30 | Code stored once and never changed; coins may only name stewards under the factory's own DAO suffixes; failed launches and connects are refunded and not listed, including tokens with malformed metadata; a guard the token won't register is flagged; names and deposits are checked; the registry pages correctly |

**Two bugs these tests found, both fixed:** the quorum's yardstick (the total weight from before the death)
used a running sum that subtracted weight for tokens moved after the death. Heavy trading after a death
could push it to zero, so any single "yes" met the quorum. In the guard, a DAO that had staked before it
was elected kept its stake in that yardstick after winning, though it can't vote. The regression tests
are `eligible_total_ignores_tokens_bought_after_the_death`,
`late_stake_does_not_shrink_the_eligible_total` and
`a_dao_that_staked_before_its_election_does_not_inflate_the_next_quorum`.

**What an independent review found next, all fixed in the current factory:** someone calling the factory
directly could pick settings that switched the takeover off (any account as "DAO", no vesting, a free
comeback, a heartbeat that never ends); in the guard, a bag locked later could be claimed at once, very large
stakes of a 24-decimal token overflowed the weight maths, and staker entries cost nothing; votes and history
entries were stored at the coin's expense; one dust holder could keep the takeover slot busy; and a token
with malformed metadata left the connect deposit stuck in the factory. Each has a test in
[launch_rules.rs](contract/src/tests/launch_rules.rs), [guard/src/tests/fixes.rs](guard/src/tests/fixes.rs) or
[factory/src/tests_more.rs](factory/src/tests_more.rs). The review also showed four tests were weaker than
their names; they now check what they claim.

**End-to-end on a real NEAR node:** CI starts a local NEAR sandbox and runs
[scripts/demo-coins.mjs](scripts/demo-coins.mjs): it deploys the factory, stores
the code, deletes the factory's key, launches four coins, connects an outside
token and plays five stories with real wasm and a real clock, checking every step:

* **$NINE**: the dev goes silent, a whale buys 250M after the death and is refused a vote, holders vote a steward in, the coin revives, 100M creator tokens burn, the old dev can no longer pulse, the steward pulses.
* **$BACK**: the dev goes silent, a takeover opens, a late pulse without the penalty is refused, the dev pays the 0.2 NEAR penalty, the vote is cancelled and the penalty lands in the treasury.
* **$MEME**: a plain NEP-141 ([samples/plain-token](samples/plain-token), no Revive code) is connected. The factory reads its name from the token and registers the guard. The dev locks 100M, holders stake 600M, the dev goes silent, a whale stakes 250M after the death and gets weight 0, holders take over, the 100M bag is sealed, the token confirms the guard holds exactly staked + sealed, and a holder unstakes back to their wallet.
* **$CIVIC**: the takeover goes to a **real Sputnik DAO**, running the exact contract code testnet DAOs run (fetched from testnet). $CIVIC accepts only Sputnik DAOs. After the revival the old dev can't pulse; the DAO pulses $CIVIC through a proposal its council approves, then pays a contributor 0.05 NEAR from the coin's treasury the same way. A council member acting alone can't spend.
* **$HERE**: a coin with a 3-day heartbeat that its keeper pulses on time; it stays alive and nobody can open a takeover.

```bash
cd scripts && npm install && cd ..
node scripts/sandbox-e2e.mjs      # needs the two wasm files from CI or cargo near build
```

**The same stories on testnet:** run the **testnet-demo** workflow (Actions tab)
after adding `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` secrets for a funded
testnet account. Pick the stories with `stories` (any of `coins`, `guard`, `dao`, `alive`, comma-separated); the run prints
how much NEAR it needs before it starts. It uploads `demo-coins.json` with the factory
and coin names. Pass an existing Sputnik DAO as `steward` to nominate a real DAO.

**The app:** open `web/index.html` (demo mode), or `?factory=revive-xxxxx.you.testnet`
for live testnet data. Wallet signing uses [NEAR Connect](https://github.com/hot-dao/near-connect).
The site is published to GitHub Pages on every push.

## Contract API

| Method | Who | Notes |
| --- | --- | --- |
| `new(config)` | creator | Launch. Attached NEAR seeds the treasury. |
| `pulse()` | keeper | Late = comeback, needs `comeback_penalty`. |
| `claim_vested()` | creator | Only while alive and still keeper. |
| `fund_treasury()` | anyone | Payable. |
| `spend(receiver_id, amount, memo)` | keeper | Within `spend_cap` per `spend_period_secs`. |
| `set_links(icon, links)` | keeper | Attach NEAR for the new data's storage (about 0.043 NEAR for a 4 KB icon); the rest is refunded. |
| `start_takeover(steward)` | holder with weight | After the flatline. Attach the 0.1 NEAR bond plus 0.009 for storage: the bond goes back if the takeover passes or the keeper comes back, and to the treasury if it fails. |
| `vote(support)` | holder with weight | One ballot per takeover. Attach 0.0025 NEAR for the ballot's storage (excess refunded). |
| `finalize_takeover()` | anyone | After the vote ends. Attach 0.004 NEAR for the history entry. |
| `get_status`, `get_vote_weight`, `get_ballot`, `get_lore`, `get_lore_count` | view | |
| NEP-141 / NEP-145 methods | | `ft_balance_of(<contract>)` returns the creator's locked tokens. |

**Factory**

| Method | Who | Notes |
| --- | --- | --- |
| `new(owner, dao_suffixes)` | deployer | The only steward suffixes its coins may use, e.g. `[".sputnikv2.testnet"]`. Fixed for good. |
| `store_code()` | owner, once | The coin wasm as raw input. Can never change afterwards. |
| `launch(name, config)` | anyone | Attach ≥ `get_launch_cost()` (the coin's code size plus room for state, about 2.5 NEAR), with 300 TGas. Creates `<name>.<factory>`; the caller becomes creator and keeper; extra NEAR seeds the treasury. Refunds if the launch fails. |
| `store_guard_code()` | owner, once | The guard wasm as raw input. |
| `connect(name, config)` | anyone | Attach ≥ `get_connect_cost()` (about 2.2 NEAR), with 300 TGas. `config.token_id` is the NEP-141 to guard. Reads its metadata, creates `<name>.<factory>` with no keys, registers it on the token, records it with `kind: "guard"`. Refunds if the token check or the deploy fails. |
| `get_coins`, `get_coin_count`, `get_code_hash`, `get_guard_code_hash`, `get_launch_cost`, `get_connect_cost`, `get_owner`, `get_dao_suffixes` | view | |

**Guard** (a connected coin)

| Method | Who | Notes |
| --- | --- | --- |
| `storage_deposit()` | staker, once | 0.002 NEAR for the staker's entry. Required before the first stake. |
| `ft_on_transfer` | the token | `msg: "lock"` from the living creator, or `msg: "stake"` from a registered staker. Anything else is refunded. |
| `unstake(amount)` | staker | Attach 1 yoctoNEAR, 100 TGas. Rolled back if the token transfer fails. |
| `pulse`, `claim_vested`, `fund_treasury`, `spend`, `set_links`, `start_takeover`, `vote`, `finalize_takeover` | as above | Same rules as a Revive coin; on revival the creator's bag is sealed, not burned. |
| `get_status`, `get_stake`, `get_vote_weight`, `get_ballot`, `get_lore`, `get_lore_count` | view | `get_status` has `kind: "guard"`, `token_id`, `staked`, `stakers`. |

## Security audit (November 2026)

We audited Revive ourselves before submitting: the three contracts, the web app, the scripts, the CI
workflows, and the factories and coins live on testnet. This is an **internal** audit, not an independent
one. The full report, with the evidence for every finding, is [docs/AUDIT.md](docs/AUDIT.md).

**What we ran, and what came back:**

| Check | Command | Result |
| --- | --- | --- |
| Contract unit tests | `cargo test` in `contract`, `guard`, `factory` | 164 passed, 0 failed (90 + 44 + 30) |
| Lints | `cargo clippy --all-targets -- -D warnings` in each crate | clean |
| The site in Chrome, against a hostile coin | `python tests/browser/site_test.py` | 28 passed, 0 failed |
| All five stories on a local NEAR node | CI job `sandbox-e2e` | all checks passed |
| 60-day simulation | `cargo run --example simulate` | runs; output byte for byte the same as the published `docs/simulation.json` |
| The live chain | read every factory and listed coin over RPC | all 4 factories and all 11 coins and guards have no access keys; every coin's code hash equals its factory's |
| Leaked keys | scan of every commit in the git history | none |
| JavaScript dependencies | `npm audit` in `scripts` | 0 vulnerabilities (after the fix below) |
| Rust dependencies | `cargo audit` in each crate | 1 advisory and 2 "unmaintained" warnings; none affects the deployed wasm except near-sdk's unmaintained allocator (see below) |

**What held up.** Nothing we found lets anyone move a coin's tokens or treasury against its rules. Only the keeper
can pulse, spend or change links. Only holders from before the death vote, once each. The creator's own account
never votes. Callbacks are private. Code is stored once and only by the owner. Script payloads in a coin's name,
symbol, links, icon, history and token metadata never ran in the browser.

**What we fixed:**

| Severity | Finding | Fix |
| --- | --- | --- |
| High | The Verify tab compared a coin only with its own factory, so a copycat factory's coins passed every check | The app pins Revive's four factories and their code hashes; anything else is labelled **Not an official Revive coin** |
| Medium | A link with `?factory=` or `?connect=` could point Launch and Connect at another contract, with no warning | Launch and Connect warn when the factory isn't Revive's |
| Medium | A connected token's page didn't show how much the guard actually protects | Verify shows the share of supply the guard covers, whether the token's team still holds keys, and who connected it |
| Low | During an RPC outage the list said "Loading coins…" forever | After three failed tries it says NEAR can't be reached and keeps retrying |
| Low | No security headers on the hosted site | `web/vercel.json` blocks framing and MIME sniffing |
| Low | CI workflows used the repository's default token permissions | Every workflow now gets a read-only token |
| Low | A CI-only dependency (`tar`, through `near-sandbox`) had known denial-of-service advisories | Pinned to the patched version |
| Info | Verify said to compare the code hash with CI's, which can't match while the build isn't reproducible | Verify now says what the hash proves, and what it doesn't |

**What is still open**, and why:

| Severity | Finding | Status |
| --- | --- | --- |
| High (design) | **A creator's second wallet can outvote holders.** Weight belongs to accounts; a creator who moves tokens to another wallet long before the death keeps that wallet's weight. The test `a_creator_with_a_second_wallet_can_outvote_the_holders` shows 500M beating 300M of honest votes | No code fix without identity. The transfers are public and the Activity tab shows them |
| High | **One empty slot on an earlier factory.** `revive-bhtat` stored guard code but never coin code, so its owner account could still store coin code there once | The app expects no coins from it and fails any that appear. Closing the slot for good takes one testnet transaction by the owner |
| Medium | **The build isn't reproducible.** In CI the coin builds to a different hash (`CR5qCUDA…`) from the deployed one (`6Unt3cdM…`), so nobody can confirm the deployed coins run exactly this source | Needs `cargo near build reproducible-wasm` and a new factory from that build |
| Medium | **The wallet library loads from esm.sh** without an integrity hash | Bundle it into the repo before any mainnet use |
| Medium | **A guard trusts its token.** A token whose team still holds keys can be upgraded, mint more, or report transfers it never made; anyone can connect any token | By design; Verify now shows the token's keys and the guard's coverage |
| Low | Treasury transfers to accounts that don't exist bounce back to the contract but not to the `treasury` counter | Fix in the next contract version |
| Low | One takeover at a time: a griefer can hold the slot for one vote | Costs them the 0.1 NEAR bond if it fails |
| Info | `revive.testnet`, which owns the factories, has a full-access key in GitHub secrets for the demo and keep-alive workflows | Testnet only; for mainnet, a separate keeper with a key limited to `pulse` |
| Info | near-sdk's allocator (`wee_alloc`) is flagged unmaintained; two other advisories are only in the local test runtime, not the wasm | Every NEAR call starts with fresh memory, so the allocator's known leak can't build up |
| Info | Icons from connected tokens can load from other websites, which then see visitors' IP addresses | Coins launched on Revive store their icon on-chain |

**Not tested in this audit:** signing with a real wallet on the live site (needs a person and a wallet), and the
connected-token story on the current factory (about 2 testnet NEAR short; it passed on the earlier factory and on the
local sandbox).

**Verdict:** as a **testnet demonstration**, everything we tested works. For **real money on mainnet**, Revive is
**not ready**: the open High findings need decisions, the build needs to be reproducible, and it needs an independent
audit.

## Limits (honest list)

* **Off-chain assets don't move.** The contract can't take over the dev's X account, Telegram or website. Revive moves what lives on-chain: the keeper role, treasury, links, metadata icon and the dev's tokens. Takeover teams may also face IP questions about the original art ([Decrypt](https://decrypt.co/247781/meme-coin-takeover-teams-face-legal-entanglement-experts-say)).
* **Connected coins only count stake.** Tokens sitting in wallets or pools don't vote; holders have to stake, and the guard can't see who held before connecting. A sealed bag leaves circulation for good, but the token's `ft_total_supply` doesn't drop, because only the token can burn. And the guard can't stop powers the token itself grants, such as an owner who can still mint.
* **Hold-time is not identity.** Vote weight belongs to accounts, and the contract can't tell that two accounts are one person. A creator who moves liquid tokens to a second wallet long before the death gets that wallet's full weight and can vote a takeover down; a patient attacker can buy early and wait. Revive stops *buying a vote after the death*, and the creator's own account and locked tokens never vote, but it does not stop someone who planned ahead. The test `a_creator_with_a_second_wallet_can_outvote_the_holders` shows it. Holders' defence is the public record: such transfers are on-chain and the Activity tab shows them.
* **Pools count but never vote.** Tokens in a liquidity pool held before the death count toward the eligible total, but a pool can't vote. That's why we recommend a 10% quorum (see [Picking the parameters](#picking-the-parameters)).
* **Earlier testnet coins keep an older quorum count.** Coins from the first two factories (`revive-ztw7t`, `revive-bhtat`) count tokens bought after the death as slightly negative, which makes their quorum a little lenient. Our tests found it; coins from the current factory count exactly.
* **Demo stewards.** The quick demo stories use a stand-in steward account instead of a Sputnik DAO, because a DAO costs about 5.6 testnet NEAR. The [$CIVIC](#revive) story uses a real Sputnik DAO. The current factory only allows `.sputnikv2.testnet` stewards, fixed when it was made; the earlier factories took the suffix from the launch settings, so the Verify tab checks each coin's.
* **Anyone can connect any token.** Connecting doesn't prove you made the token, and one token can have several guards. The Verify tab says who connected it, how much of the supply the guard covers, and whether the token still has access keys (a token whose team holds keys can be upgraded, mint more, or report transfers it never made).
* **Only Revive's own factories are vouched for.** Anyone can deploy a copy of the factory, lock it, and make coins whose Verify checks pass against their own factory. The app pins Revive's factories and their code hashes, and marks anything else **Not an official Revive coin**. A link with `?factory=` or `?connect=` that points elsewhere shows a warning on Launch and Connect.
* **One empty slot on an earlier factory.** `revive-bhtat` stored guard code but never coin code. Its owner (`revive.testnet`) could still store coin code there once. The app expects no coins from it, so any that appears fails the factory check.
* **One takeover at a time.** A holder from before the death can still open a takeover nobody wants; it now costs a 0.1 NEAR bond that goes to the treasury when it fails.
* **meme.cooking untested.** Connect works with any NEP-141 token and was tested on our own plain token; we haven't run it against a real meme.cooking token yet.
* **Storage.** Each coin carries its own copy of the code, so a launch costs about 2.5 NEAR of storage. NEAR global contracts would cut that; they were left out to keep testnet costs predictable.
* **Treasury transfers to accounts that don't exist** are refunded to the contract's balance, but not re-added to the `treasury` counter.
* **The wallet library loads from esm.sh**, pinned to a version but without an integrity hash, so the site trusts that CDN. Bundling it into the repo is the fix before any mainnet use.
* **Unaudited by a third party.** Testnet only. An internal audit (November 2026) is in [docs/AUDIT.md](docs/AUDIT.md). The wasm is built in CI; the build is not yet reproducible (`cargo near build reproducible-wasm` is the next step).

## Repository layout

```
contract/            the coin (near-sdk 5.30)
  src/lib.rs         state, keeper, takeover, revival, views
  src/token.rs       NEP-141 / 145 / 148 and hold-time accounting
  src/tests.rs       lifecycle tests; src/tests/ attacks, timing, money, known limits, random histories (90 in all)
factory/             launches coins and connects tokens as <name>.<factory>; 30 tests
guard/               the guard for a connected token: lock, stake, takeover; 44 tests
samples/plain-token/ a plain NEP-141 standing in for a coin launched elsewhere
contract/examples/   simulate.rs: 60 days x 6 scenarios plus quorum and heartbeat sweeps, on the real contract
web/index.html       the app: Flatline Watch, Launch, Connect, coin Monitor / Activity / Verify / Lore, demo mode
docs/                AUDIT.md; simulation.json, simulation.png and plot_simulation.py (draws the chart from the json)
tests/browser/       site_test.py: the site in Chrome against a fake NEAR node serving a hostile coin
scripts/             demo-coins.mjs (deploy + five stories), sandbox-e2e.mjs, pulse.mjs (keep-alive), Sputnik DAO how-to
.github/workflows/   CI (tests, lints, wasm, sandbox e2e), Pages, testnet-demo, keep-alive (pulses $HERE twice a day)
```

## Hackathon notes

* All code was written during NEAR HACKATHON (1–14 November 2026); the first commit is from 8 November 2026.
* Libraries: near-sdk 5.30, near-api-js 7.4, near-sandbox 0.3, NEAR Connect 0.12 (loaded from esm.sh). No templates.

## License

MIT
