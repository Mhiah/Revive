// Deploys the Revive factory and plays two live stories, checking every step:
//   $NINE  — the dev goes silent, a whale buys after the death and is refused a
//            vote, holders vote a steward in, the coin revives, the steward pulses.
//   $BACK  — the dev goes silent, a takeover opens, the dev comes back mid-vote
//            and pays the penalty into the treasury.
//
// Testnet (from the repo root, after building both wasm files):
//   NEAR_ACCOUNT_ID=you.testnet NEAR_PRIVATE_KEY=ed25519:... node scripts/demo-coins.mjs
// The sandbox run in CI calls run() from sandbox-e2e.mjs.

import { Account, JsonRpcProvider, KeyPair, nearToYocto, teraToGas } from "near-api-js";
import { readFile, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const TOKEN = 10n ** 18n;
const tokens = (n) => (BigInt(n) * TOKEN).toString();
const sleep = (s) => new Promise((r) => setTimeout(r, s * 1000));
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

function check(cond, msg) {
  if (!cond) throw new Error("CHECK FAILED: " + msg);
  log("  ✓", msg);
}

export async function run(opts) {
  const {
    rpc, rootId, rootKey,
    coinWasm = "contract/target/near/revive.wasm",
    factoryWasm = "factory/target/near/revive_factory.wasm",
    prefix = "revive",
    timer = 60,                       // pulse interval and vote length, seconds
    daoSuffix,                        // e.g. ".sputnikv2.testnet"; default: stand-in stewards under the root
    steward: givenSteward,            // an existing Sputnik DAO that will pulse, if you have one
    stewardKey,                       // key of a stand-in steward account (sandbox)
    lockFactory = true,
    out = "demo-coins.json",
  } = opts;

  const provider = new JsonRpcProvider({ url: rpc });
  const root = new Account(rootId, provider, rootKey);
  const key = KeyPair.fromRandom("ed25519");
  const pub = key.getPublicKey().toString();
  const tag = Date.now().toString(36).slice(-5);
  const sub = (p) => `${p}-${tag}.${rootId}`;

  const view = (contractId, method, args = {}) => provider.callFunction({ contractId, method, args });
  const as = (id) => new Account(id, provider, key.toString());
  const call = (who, contractId, methodName, args = {}, deposit = "0", gas = teraToGas("30")) =>
    who.callFunction({ contractId, methodName, args, deposit, gas });
  const fails = async (p, expect) => {
    try { await p; } catch (e) { return String(e.message || e).includes(expect); }
    return false;
  };

  // ------------------------------------------------------------ accounts
  // Fund each account with what it needs: storage is 1 NEAR per 100 KB.
  const coinBytes = (await readFile(coinWasm)).length;
  const factoryBytes = (await readFile(factoryWasm)).length;
  const perByte = 1e-5;                                         // NEAR per byte
  const launchCost = (coinBytes + 10_000) * perByte;            // matches get_launch_cost
  const factoryFund = (factoryBytes + coinBytes + 4_000) * perByte + 0.3;
  const devFund = 2 * (launchCost + 0.1) + 0.2 + 0.3;          // two launches, treasury seeds, penalty, gas
  const r2 = (x) => (Math.ceil(x * 100) / 100).toFixed(2);
  const names = { dev: r2(devFund), alice: "0.15", bob: "0.15", carol: "0.15", whale: "0.15" };
  const need = factoryFund + devFund + 0.6 + (givenSteward ? 0 : 0.25) + 0.2;
  log(`Coin wasm ${coinBytes} bytes, factory wasm ${factoryBytes} bytes`);
  const have = Number(BigInt(await root.getBalance()) / 10n ** 21n) / 1000;
  log(`Root ${rootId} has ${have} NEAR; this run needs about ${r2(need)} NEAR`);
  if (have < need) throw new Error(`Not enough NEAR on ${rootId}: has ${have}, needs about ${need}. Top it up from the testnet faucet.`);
  log(`Creating demo accounts with tag ${tag}`);
  const factoryId = `${prefix}-${tag}.${rootId}`;
  await root.createSubAccount({ accountOrPrefix: `${prefix}-${tag}`, publicKey: pub, nearToTransfer: nearToYocto(r2(factoryFund)) });
  const acc = {};
  for (const [n, amt] of Object.entries(names)) {
    await root.createSubAccount({ accountOrPrefix: `${n}-${tag}`, publicKey: pub, nearToTransfer: nearToYocto(amt) });
    acc[n] = as(sub(n));
  }

  let steward = givenSteward;
  let suffix = daoSuffix;
  let stewardAcc = null;
  if (!steward) {
    // Stand-in steward: a plain account in its own namespace, nominated by suffix.
    await root.createSubAccount({ accountOrPrefix: `stewards-${tag}`, publicKey: pub, nearToTransfer: nearToYocto("0.25") });
    const parent = as(`stewards-${tag}.${rootId}`);
    await parent.createSubAccount({ accountOrPrefix: "ninelives", publicKey: stewardKey || pub, nearToTransfer: nearToYocto("0.1") });
    steward = `ninelives.stewards-${tag}.${rootId}`;
    suffix = `.stewards-${tag}.${rootId}`;
    stewardAcc = new Account(steward, provider, stewardKey || key.toString());
    log(`Stand-in steward ${steward} (no Sputnik DAO given)`);
  }

  // ------------------------------------------------------------- factory
  log(`Deploying factory ${factoryId}`);
  const factory = as(factoryId);
  await factory.deployContract(await readFile(factoryWasm));
  await call(factory, factoryId, "new", { owner: rootId });
  const wasm = await readFile(coinWasm);
  await root.callFunction({ contractId: factoryId, methodName: "store_code", args: new Uint8Array(wasm), gas: teraToGas("300") });
  const codeHash = await view(factoryId, "get_code_hash");
  check(!!codeHash, `factory stored the coin code (hash ${codeHash})`);
  if (lockFactory) {
    await factory.deleteKey(pub);
    const keys = await new Account(factoryId, provider).getAccessKeyList();
    check((keys.keys || []).length === 0, "factory has no keys left: nobody controls it");
  }

  const config = (name, symbol) => ({
    name, symbol, icon: null, links: "https://github.com/Mhiah/Revive",
    total_supply: tokens(1_000_000_000), dev_allocation: tokens(100_000_000),
    distributor: "", // filled below: the coin itself, meaning the creator
    pulse_interval_secs: timer, vote_duration_secs: timer, quorum_bps: 2000, vest_duration_secs: 86400,
    spend_cap: nearToYocto("0.1").toString(), spend_period_secs: 3600,
    comeback_penalty: nearToYocto("0.2").toString(), dao_suffix: suffix,
  });

  async function launch(name, symbol) {
    const coinId = `${name}.${factoryId}`;
    const cfg = config(`${symbol === "NINE" ? "Nine Lives" : "Back From The Dead"}`, symbol);
    cfg.distributor = coinId;
    const cost = BigInt(await view(factoryId, "get_launch_cost"));
    log(`  launch cost ${Number(cost / 10n ** 21n) / 1000} NEAR + 0.1 NEAR treasury`);
    const raw = await acc.dev.callFunctionRaw({ contractId: factoryId, methodName: "launch", args: { name, config: cfg },
      deposit: (cost + BigInt(nearToYocto("0.1"))).toString(), gas: teraToGas("300") });
    const failures = (raw.receipts_outcome || []).filter((r) => r.outcome.status && r.outcome.status.Failure);
    for (const f of failures) console.error("  receipt failed:", f.outcome.executor_id, JSON.stringify(f.outcome.status.Failure));
    for (const r of raw.receipts_outcome || []) for (const l of r.outcome.logs || []) if (!l.startsWith("EVENT_JSON")) console.log("  log:", l);
    const sv = raw.status && raw.status.SuccessValue;
    const ok = sv !== undefined && Buffer.from(sv, "base64").toString() === "true";
    check(ok, `${symbol} launched at ${coinId}`);
    const s = await view(coinId, "get_status");
    check(s.creator === acc.dev.accountId && s.keeper === acc.dev.accountId, `${symbol} creator and keeper is ${acc.dev.accountId}`);
    for (const h of ["alice", "bob", "carol", "whale"]) await call(acc[h], coinId, "storage_deposit", {}, nearToYocto("0.00125").toString());
    const give = { alice: 300_000_000, bob: 200_000_000, carol: 100_000_000 };
    for (const [h, n] of Object.entries(give)) await call(acc.dev, coinId, "ft_transfer", { receiver_id: acc[h].accountId, amount: tokens(n) }, "1");
    await call(acc.dev, coinId, "pulse");
    return coinId;
  }

  async function waitPast(coinId, field) {
    const s = await view(coinId, "get_status");
    const target = field === "flatline" ? s.flatline_at : s.takeover.ends_at;
    const wait = Math.max(0, target - s.now) + 4;
    log(`  … waiting ${wait}s`);
    await sleep(wait);
  }

  // ---------------------------------------------------- story 1: revival
  log("Story 1: $NINE dies and revives");
  const nine = await launch("nine", "NINE");
  log("  dev goes silent");
  await waitPast(nine, "flatline");
  check((await view(nine, "get_status")).status === "flatlined", "NINE flatlined");
  await call(acc.dev, nine, "ft_transfer", { receiver_id: acc.whale.accountId, amount: tokens(250_000_000) }, "1");
  check((await view(nine, "get_vote_weight", { account_id: acc.whale.accountId })) === "0", "whale bought 250M after the death: weight 0");
  await call(acc.alice, nine, "start_takeover", { steward });
  check((await view(nine, "get_status")).status === "voting", `takeover opened for ${steward}`);
  await call(acc.alice, nine, "vote", { support: true });
  check(await fails(call(acc.whale, nine, "vote", { support: false }), "no voting weight"), "whale's vote refused");
  await call(acc.bob, nine, "vote", { support: true });
  await call(acc.carol, nine, "vote", { support: false });
  await waitPast(nine, "vote");
  const revived = await call(acc.bob, nine, "finalize_takeover");
  const s1 = await view(nine, "get_status");
  check(revived === true && s1.keeper === steward && s1.lives === 1, "NINE revived: steward is keeper, lives = 1");
  check(BigInt(s1.dev_burned) === 100_000_000n * TOKEN, "creator's 100M locked tokens burned");
  check(await fails(call(acc.dev, nine, "pulse", {}, nearToYocto("0.2").toString()), "only the keeper"), "old dev can no longer pulse");
  if (stewardAcc) {
    await call(stewardAcc, nine, "pulse");
    check((await view(nine, "get_status")).status === "alive", "steward pulsed: NINE alive under its new keeper");
  } else {
    log(`  steward ${steward} is a DAO: pulse it with a FunctionCall proposal (scripts/dao-pulse.md)`);
  }

  // --------------------------------------------------- story 2: comeback
  log("Story 2: $BACK's dev comes back mid-vote");
  const back = await launch("back", "BACK");
  await waitPast(back, "flatline");
  await call(acc.alice, back, "start_takeover", { steward });
  await call(acc.alice, back, "vote", { support: true });
  const before = BigInt((await view(back, "get_status")).treasury);
  check(await fails(call(acc.dev, back, "pulse"), "comeback penalty"), "late pulse without the penalty is refused");
  await call(acc.dev, back, "pulse", {}, nearToYocto("0.2").toString());
  const s2 = await view(back, "get_status");
  check(s2.status === "alive" && !s2.takeover, "BACK alive again, takeover cancelled");
  check(BigInt(s2.treasury) - before === BigInt(nearToYocto("0.2")), "0.2 NEAR penalty went to the treasury");

  const lore = await view(nine, "get_lore", {});
  check(lore.some((l) => l.kind === "revived") && lore.some((l) => l.kind === "flatline"), "NINE's lore records the flatline and the revival");
  const coins = await view(factoryId, "get_coins", {});
  check(coins.length === 2, "factory registry lists both coins");

  const result = { factory: factoryId, code_hash: codeHash, coins: [nine, back], steward, accounts: Object.fromEntries(Object.entries(acc).map(([k, v]) => [k, v.accountId])) };
  await writeFile(out, JSON.stringify(result, null, 2));
  log("All checks passed.", JSON.stringify(result));
  return result;
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  // Describe the pasted key's shape without revealing it, to debug paste problems.
  const raw = String(process.env.NEAR_PRIVATE_KEY || "");
  const B58 = /[1-9A-HJ-NP-Za-km-z]/;
  const body = raw.includes(":") ? raw.slice(raw.lastIndexOf(":") + 1) : raw;
  console.log("key shape:", JSON.stringify({
    total_chars: raw.length,
    prefix: raw.includes(":") ? raw.slice(0, raw.indexOf(":")).replace(/[^a-z0-9_-]/gi, "?") : "(none)",
    body_chars: body.trim().length,
    non_base58_in_body: [...body.trim()].filter((c) => !B58.test(c)).map((c) => (c === " " ? "space" : c === "\n" ? "newline" : /[a-z0-9]/i.test(c) ? "char:" + c : "symbol:" + c.charCodeAt(0))),
    lines: raw.split("\n").length,
  }));
  // Accept the key however it was pasted (quotes, spaces, a JSON blob): take the ed25519:... part.
  const NEAR_PRIVATE_KEY = (String(process.env.NEAR_PRIVATE_KEY || "").match(/ed25519:[1-9A-HJ-NP-Za-km-z]+/) || [])[0];
  const NEAR_ACCOUNT_ID = String(process.env.NEAR_ACCOUNT_ID || "").replace(/["'\s]/g, "");
  if (!NEAR_ACCOUNT_ID || !NEAR_PRIVATE_KEY) {
    console.error("Set NEAR_ACCOUNT_ID and NEAR_PRIVATE_KEY (a funded testnet account, about 10 NEAR).");
    process.exit(1);
  }
  run({
    rpc: process.env.NEAR_RPC || "https://test.rpc.fastnear.com",
    rootId: NEAR_ACCOUNT_ID,
    rootKey: NEAR_PRIVATE_KEY,
    timer: Number(process.env.TIMER || 60),
    steward: process.env.STEWARD,
    daoSuffix: process.env.DAO_SUFFIX,
    out: process.env.OUT || "demo-coins.json",
  }).catch((e) => { console.error(e); process.exit(1); });
}
