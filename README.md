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

**Try it now, no wallet:** [mhiah.github.io/Revive](https://mhiah.github.io/Revive/) opens in demo mode
(a simulator with the same rules). Open **Nine Lives** and click the story buttons in order.

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

## The app

| Page | What it shows |
| --- | --- |
| **Flatline Watch** | Every coin from the factory, open takeovers first, then dead coins, then whoever is closest to death. |
| **Launch** | A form that calls the factory: name, supply, locked share, pulse interval, vote length, quorum, penalty, treasury seed. |
| **Coin · Monitor** | A live heart monitor (green alive, red flatlined, amber voting), countdown, lives, treasury, the takeover vote and the keeper. |
| **Coin · Verify** | Six checks read live from NEAR RPC: no access keys on the coin, same code hash as the factory (with the hex to compare against CI), factory locked, treasury backed, supply adds up, keeper is a DAO. |
| **Coin · Lore** | Every birth, death, takeover, comeback and revival, from the contract. |

Every number is a direct RPC read. There is no indexer or backend.

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

**Unit tests (no network):**

```bash
(cd contract && cargo test)   # 21 tests
(cd factory && cargo test)    # 7 tests
```

The coin tests cover the full lifecycle: launch, pulses, flatline, takeover,
the whale-buys-after-death attack, double voting through a second wallet,
comeback during a vote, blocked comeback after the vote, failed votes, quorum,
the Sputnik-only steward rule, spend caps, two deaths and two revivals, DEX
refunds keeping hold time, the running total matching a per-holder sum, and
only the parent factory naming a creator.

**End-to-end on a real NEAR node:** CI starts a local NEAR sandbox and runs
[scripts/demo-coins.mjs](scripts/demo-coins.mjs): it deploys the factory, stores
the code, deletes the factory's key, launches two coins and plays both stories
with real wasm and a real clock, checking every step:

* **$NINE**: the dev goes silent, a whale buys 250M after the death and is refused a vote, holders vote a steward in, the coin revives, 100M creator tokens burn, the old dev can no longer pulse, the steward pulses.
* **$BACK**: the dev goes silent, a takeover opens, a late pulse without the penalty is refused, the dev pays the 0.2 NEAR penalty, the vote is cancelled and the penalty lands in the treasury.

```bash
cd scripts && npm install && cd ..
node scripts/sandbox-e2e.mjs      # needs the two wasm files from CI or cargo near build
```

**The same stories on testnet:** run the **testnet-demo** workflow (Actions tab)
after adding `NEAR_ACCOUNT_ID` and `NEAR_PRIVATE_KEY` secrets for a funded
testnet account (about 13 NEAR). It uploads `demo-coins.json` with the factory
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
| `set_links(icon, links)` | keeper | |
| `start_takeover(steward)` | holder with weight | After the flatline. |
| `vote(support)` | holder with weight | One ballot per takeover. |
| `finalize_takeover()` | anyone | After the vote ends. |
| `get_status`, `get_vote_weight`, `get_ballot`, `get_lore`, `get_lore_count` | view | |
| NEP-141 / NEP-145 methods | | `ft_balance_of(<contract>)` returns the creator's locked tokens. |

**Factory**

| Method | Who | Notes |
| --- | --- | --- |
| `new(owner)` | deployer | |
| `store_code()` | owner, once | The coin wasm as raw input. Can never change afterwards. |
| `launch(name, config)` | anyone | Attach ≥ `get_launch_cost()` (the coin's code size plus room for state, about 2.5 NEAR), with 300 TGas. Creates `<name>.<factory>`; the caller becomes creator and keeper; extra NEAR seeds the treasury. Refunds if the launch fails. |
| `get_coins`, `get_coin_count`, `get_code_hash`, `get_launch_cost`, `get_owner` | view | |

## Limits (honest list)

* **Off-chain assets don't move.** The contract can't take over the dev's X account, Telegram or website. Revive moves what lives on-chain: the keeper role, treasury, links, metadata icon and the dev's tokens. Takeover teams may also face IP questions about the original art ([Decrypt](https://decrypt.co/247781/meme-coin-takeover-teams-face-legal-entanglement-experts-say)).
* **Hold-time is not identity.** A patient attacker can buy early and wait. Revive stops *buying a vote after the death*; it does not stop someone who planned ahead.
* **Quorum is slightly lenient.** Accounts whose tokens are mostly post-flatline count as negative in the running total, which makes the eligible total a little smaller than the sum of positive weights.
* **Demo stewards.** Without a `steward` input, the demo workflow uses a stand-in steward account (its own namespace, nominated by suffix) instead of a Sputnik DAO, because creating a DAO costs several testnet NEAR. Real coins launched from the app require `.sputnikv2.testnet` / `.sputnik-dao.near`.
* **Storage.** Each coin carries its own copy of the code, so a launch costs about 2.5 NEAR of storage. NEAR global contracts would cut that; they were left out to keep testnet costs predictable.
* **Treasury transfers to accounts that don't exist** are refunded to the contract's balance, but not re-added to the `treasury` counter.
* **Unaudited.** Testnet only. The wasm is built in CI; the build is not yet reproducible (`cargo near build reproducible-wasm` is the next step).

## Repository layout

```
contract/            the coin (near-sdk 5.30)
  src/lib.rs         state, keeper, takeover, revival, views
  src/token.rs       NEP-141 / 145 / 148 and hold-time accounting
  src/tests.rs       21 unit tests
factory/             launches coins as <name>.<factory>; 7 unit tests
web/index.html       the app: Flatline Watch, Launch, coin Monitor / Verify / Lore, demo mode
scripts/             demo-coins.mjs (deploy + both stories), sandbox-e2e.mjs, Sputnik DAO how-to
.github/workflows/   CI (tests, lints, wasm, sandbox e2e), Pages, testnet-demo
```

## Hackathon notes

* All code was written during NEAR HACKATHON (1–14 November 2026); the first commit is from 8 November 2026.
* Libraries: near-sdk 5.30, near-api-js 7.4, near-sandbox 0.3, NEAR Connect 0.12 (loaded from esm.sh). No templates.

## License

MIT
