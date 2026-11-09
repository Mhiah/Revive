// Deploys the Revive factory and plays two live stories, checking every step:
//   $NINE  — the dev goes silent, a whale buys after the death and is refused a
//            vote, holders vote a steward in, the coin revives, the steward pulses.
//   $BACK  — the dev goes silent, a takeover opens, the dev comes back mid-vote
//            and pays the penalty into the treasury.
//   $MEME  — a plain NEP-141 launched outside Revive is connected: the dev locks
//            their bag, holders stake, the dev goes silent, a whale stakes after
//            the death and gets no vote, holders take over, the bag is sealed.
// STORIES=coins,guard picks which run (default both).
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
    guardWasm = "guard/target/near/revive_guard.wasm",
    tokenWasm = "samples/plain-token/target/near/plain_token.wasm",
    stories = ["coins", "guard"],
    holderNear = "0.4",              // sandbox gas is pricier than testnet's
    prefix = "revive",
    timer = 60,                       // pulse interval and vote length, seconds
    daoSuffix,                        // e.g. ".sputnikv2.testnet"; default: stand-in stewards under the root
    steward: givenSteward,            // an existing Sputnik DAO that will pulse, if you have one
    stewardKey,                       // key of a stand-in steward account (sandbox)
    lockFactory = true,
    existingFactory,                  // reuse a deployed, locked factory instead of making one
    resumeTag,                        // continue a stopped run: skip accounts and token that already exist
    sputnikFactory,                   // "sputnikv2.testnet" to create a real DAO; unset = deploy the DAO code ourselves (sandbox)
    daoCodeFrom = { rpc: "https://test.rpc.fastnear.com", account: "clausehold-council.sputnikv2.testnet" },
    daoDeposit = "5.6",               // a Sputnik v2 DAO stores ~541 KB
    out = "demo-coins.json",
  } = opts;

  const provider = new JsonRpcProvider({ url: rpc });
  const root = new Account(rootId, provider, rootKey);
  const rootPub = KeyPair.fromString(rootKey).getPublicKey().toString();
  const rootKeys = (await root.getAccessKeyList()).keys.map((k) => k.public_key);
  if (!rootKeys.includes(rootPub)) throw new Error(`The private key does not match any key on ${rootId}; nothing was spent. Re-copy the key from the wallet.`);
  log(`Key matches an access key on ${rootId}`);
  // Demo accounts share the root's key, so a run that stops halfway can be picked up again.
  const key = KeyPair.fromString(rootKey);
  const pub = key.getPublicKey().toString();
  const tag = resumeTag || Date.now().toString(36).slice(-5);
  const exists = async (id) => { try { await new Account(id, provider).getState(); return true; } catch { return false; } };
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
  const want = (x) => stories.includes(x);
  const needCoin = want("coins") || want("dao");
  const size = async (f) => (await readFile(f)).length;
  const coinBytes = needCoin ? await size(coinWasm) : 0;
  const guardBytes = want("guard") ? await size(guardWasm) : 0;
  const tokenBytes = want("guard") ? await size(tokenWasm) : 0;
  const factoryBytes = await size(factoryWasm);
  const perByte = 1e-5;                                         // NEAR per byte
  const launchCost = (coinBytes + 10_000) * perByte;            // matches get_launch_cost
  const connectCost = (guardBytes + 10_000) * perByte + 0.00125; // matches get_connect_cost
  const factoryFund = existingFactory ? 0 : (factoryBytes + coinBytes + guardBytes + 4_000) * perByte + 0.3;
  const devFund = (want("coins") ? 2 * (launchCost + 0.1) + 0.2 : 0) + (want("guard") ? connectCost + 0.1 + 0.2 : 0)
    + (want("dao") ? launchCost + 0.2 + 0.2 : 0) + 0.3;
  const daoFund = want("dao") ? Number(daoDeposit) + (sputnikFactory ? 0 : 0.3) : 0;
  const tokenFund = want("guard") ? (tokenBytes + 4_000) * perByte + 0.3 : 0;
  const r2 = (x) => (Math.ceil(x * 100) / 100).toFixed(2);
  // In the DAO story alice proposes, which needs the DAO's 1 NEAR proposal bond (refunded when it passes).
  const aliceNear = want("dao") ? r2(Number(holderNear) + 1.1) : holderNear;
  const names = { dev: r2(devFund), alice: aliceNear, bob: holderNear, carol: holderNear, whale: holderNear };
  const standIn = !givenSteward && (want("coins") || want("guard"));
  const need = factoryFund + devFund + tokenFund + daoFund + Number(aliceNear) + 3 * Number(holderNear) + (standIn ? 0.35 : 0) + 0.3;
  log(`Stories: ${stories.join(", ")}. Wasm bytes: coin ${coinBytes}, guard ${guardBytes}, token ${tokenBytes}, factory ${factoryBytes}`);
  const have = Number(BigInt(await root.getBalance()) / 10n ** 21n) / 1000;
  log(`Root ${rootId} has ${have} NEAR; this run needs about ${r2(need)} NEAR`);
  if (resumeTag) log(`Resuming run ${tag}: accounts that already exist are reused`);
  else if (have < need) throw new Error(`Not enough NEAR on ${rootId}: has ${have}, needs about ${r2(need)}. Top it up from the testnet faucet.`);
  log(`Creating demo accounts with tag ${tag}`);
  const factoryId = existingFactory || `${prefix}-${tag}.${rootId}`;
  if (!existingFactory) await root.createSubAccount({ accountOrPrefix: `${prefix}-${tag}`, publicKey: pub, nearToTransfer: nearToYocto(r2(factoryFund)) });
  const acc = {};
  for (const [n, amt] of Object.entries(names)) {
    if (!(resumeTag && await exists(sub(n)))) await root.createSubAccount({ accountOrPrefix: `${n}-${tag}`, publicKey: pub, nearToTransfer: nearToYocto(amt) });
    acc[n] = as(sub(n));
  }

  let steward = givenSteward;
  let suffix = daoSuffix;
  let stewardAcc = null;
  if (standIn) {
    // Stand-in steward: a plain account in its own namespace, nominated by suffix.
    const parent = as(`stewards-${tag}.${rootId}`);
    if (!(resumeTag && await exists(parent.accountId))) {
      await root.createSubAccount({ accountOrPrefix: `stewards-${tag}`, publicKey: pub, nearToTransfer: nearToYocto("0.25") });
      await parent.createSubAccount({ accountOrPrefix: "ninelives", publicKey: stewardKey || pub, nearToTransfer: nearToYocto("0.1") });
    }
    steward = `ninelives.stewards-${tag}.${rootId}`;
    suffix = `.stewards-${tag}.${rootId}`;
    stewardAcc = new Account(steward, provider, stewardKey || key.toString());
    log(`Stand-in steward ${steward} (no Sputnik DAO given)`);
  }

  // ------------------------------------------------------------- factory
  let codeHash = null, guardHash = null;
  if (existingFactory) {
    log(`Reusing factory ${factoryId}`);
    if (needCoin) codeHash = await view(factoryId, "get_code_hash");
    if (want("guard")) guardHash = await view(factoryId, "get_guard_code_hash");
    check((!needCoin || codeHash) && (!want("guard") || guardHash), "the existing factory has the code these stories need");
  } else {
  log(`Deploying factory ${factoryId}`);
  const factory = as(factoryId);
  await factory.deployContract(await readFile(factoryWasm));
  await call(factory, factoryId, "new", { owner: rootId });
  if (needCoin) {
    await root.callFunction({ contractId: factoryId, methodName: "store_code", args: new Uint8Array(await readFile(coinWasm)), gas: teraToGas("300") });
    codeHash = await view(factoryId, "get_code_hash");
    check(!!codeHash, `factory stored the coin code (hash ${codeHash})`);
  }
  if (want("guard")) {
    await root.callFunction({ contractId: factoryId, methodName: "store_guard_code", args: new Uint8Array(await readFile(guardWasm)), gas: teraToGas("300") });
    guardHash = await view(factoryId, "get_guard_code_hash");
    check(!!guardHash, `factory stored the guard code (hash ${guardHash})`);
  }
  if (lockFactory) {
    await factory.deleteKey(pub);
    const keys = await new Account(factoryId, provider).getAccessKeyList();
    check((keys.keys || []).length === 0, "factory has no keys left: nobody controls it");
  }
  }

  const config = (name, symbol) => ({
    name, symbol, icon: null, links: "https://github.com/Mhiah/Revive",
    total_supply: tokens(1_000_000_000), dev_allocation: tokens(100_000_000),
    distributor: "", // filled below: the coin itself, meaning the creator
    pulse_interval_secs: timer, vote_duration_secs: timer, quorum_bps: 2000, vest_duration_secs: 86400,
    spend_cap: nearToYocto("0.1").toString(), spend_period_secs: 3600,
    comeback_penalty: nearToYocto("0.2").toString(), dao_suffix: suffix,
  });

  async function launch(name, symbol, display, sfx = suffix) {
    const coinId = `${name}.${factoryId}`;
    const cfg = config(display || (symbol === "NINE" ? "Nine Lives" : "Back From The Dead"), symbol);
    cfg.dao_suffix = sfx;
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
    // A view can lag the latest block (and miss a pulse that just landed), so keep
    // checking until the chain itself shows the deadline has passed.
    for (let i = 0; i < 40; i++) {
      const n = await view(coinId, "get_status");
      const t = field === "flatline" ? n.flatline_at : n.takeover?.ends_at;
      if (t !== undefined && n.now > t + 1) return;
      await sleep(3);
    }
  }

  const result = { factory: factoryId, code_hash: codeHash, guard_code_hash: guardHash, coins: [], steward,
    accounts: Object.fromEntries(Object.entries(acc).map(([k, v]) => [k, v.accountId])) };
  if (want("coins")) {
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
  result.coins.push(nine, back);
  }

  if (want("guard")) {
  // ------------------------------------- story 3: an outside token, connected
  log("Story 3: $MEME, a plain NEP-141 launched outside Revive, connected and taken over");
  const tokenId = `meme-${tag}.${rootId}`;
  const tokenReady = resumeTag && await exists(tokenId) && (await view(tokenId, "ft_balance_of", { account_id: acc.carol.accountId })) !== "0";
  if (!tokenReady) {
  await root.createSubAccount({ accountOrPrefix: `meme-${tag}`, publicKey: pub, nearToTransfer: nearToYocto(r2(tokenFund)) });
  const tokenAcc = as(tokenId);
  await tokenAcc.deployContract(await readFile(tokenWasm));
  await call(tokenAcc, tokenId, "new", { owner_id: acc.dev.accountId, total_supply: tokens(1_000_000_000), name: "Meme Cooked", symbol: "MEME" });
  }
  check((await view(tokenId, "ft_metadata")).symbol === "MEME", `outside token ${tokenId} exists, 1B $MEME owned by the dev`);
  if (!tokenReady) {
  const reg = (await view(tokenId, "storage_balance_bounds")).min;
  for (const h of ["alice", "bob", "carol", "whale"]) await call(acc[h], tokenId, "storage_deposit", {}, reg);
  for (const [h, n] of Object.entries({ alice: 300_000_000, bob: 200_000_000, carol: 100_000_000 })) {
    await call(acc.dev, tokenId, "ft_transfer", { receiver_id: acc[h].accountId, amount: tokens(n) }, "1");
  }
  }
  const ftCall = (who, receiver, n, msg) =>
    call(who, tokenId, "ft_transfer_call", { receiver_id: receiver, amount: tokens(n), msg }, "1", teraToGas("100"));

  const guardName = existingFactory ? `meme-${tag}` : "meme";      // a reused factory may already have a "meme"
  const gcost = BigInt(await view(factoryId, "get_connect_cost"));
  log(`  connect cost ${Number(gcost / 10n ** 21n) / 1000} NEAR + 0.1 NEAR treasury`);
  const raw = await acc.dev.callFunctionRaw({
    contractId: factoryId, methodName: "connect", gas: teraToGas("300"),
    deposit: (gcost + BigInt(nearToYocto("0.1"))).toString(),
    args: { name: guardName, config: {
      token_id: tokenId, links: "https://github.com/Mhiah/Revive",
      pulse_interval_secs: timer, vote_duration_secs: timer, quorum_bps: 2000, vest_duration_secs: 86400,
      spend_cap: nearToYocto("0.1").toString(), spend_period_secs: 3600,
      comeback_penalty: nearToYocto("0.2").toString(), dao_suffix: suffix,
    } },
  });
  for (const r of raw.receipts_outcome || []) {
    if (r.outcome.status && r.outcome.status.Failure) console.error("  receipt failed:", r.outcome.executor_id, JSON.stringify(r.outcome.status.Failure));
    for (const l of r.outcome.logs || []) if (!l.startsWith("EVENT_JSON")) console.log("  log:", l);
  }
  const guardId = `${guardName}.${factoryId}`;
  const g0 = await view(guardId, "get_status");
  check(g0.kind === "guard" && g0.token_id === tokenId && g0.creator === acc.dev.accountId && g0.config.symbol === "MEME",
    `connected: guard ${guardId} protects ${tokenId}; name and symbol read from the token`);
  check((await view(tokenId, "storage_balance_of", { account_id: guardId })) !== null, "the factory registered the guard on the token");

  await ftCall(acc.dev, guardId, 100_000_000, "lock");
  check(BigInt((await view(guardId, "get_status")).dev_locked) === 100_000_000n * TOKEN, "dev locked a 100M $MEME bag in the guard");
  for (const [h, n] of Object.entries({ alice: 300_000_000, bob: 200_000_000, carol: 100_000_000 })) await ftCall(acc[h], guardId, n, "stake");
  check(BigInt((await view(guardId, "get_status")).staked) === 600_000_000n * TOKEN, "holders staked 600M $MEME");
  await call(acc.dev, guardId, "pulse");
  log("  dev goes silent");
  await waitPast(guardId, "flatline");
  check((await view(guardId, "get_status")).status === "flatlined", "MEME's guard flatlined");
  await call(acc.dev, tokenId, "ft_transfer", { receiver_id: acc.whale.accountId, amount: tokens(250_000_000) }, "1");
  await ftCall(acc.whale, guardId, 250_000_000, "stake");
  check((await view(guardId, "get_vote_weight", { account_id: acc.whale.accountId })) === "0", "whale staked 250M after the death: weight 0");
  await call(acc.alice, guardId, "start_takeover", { steward });
  await call(acc.alice, guardId, "vote", { support: true });
  check(await fails(call(acc.whale, guardId, "vote", { support: false }), "no voting weight"), "whale's vote refused");
  await call(acc.bob, guardId, "vote", { support: true });
  await call(acc.carol, guardId, "vote", { support: false });
  await waitPast(guardId, "vote");
  const done = await call(acc.bob, guardId, "finalize_takeover");
  const g1 = await view(guardId, "get_status");
  check(done === true && g1.keeper === steward && g1.lives === 1, "MEME revived: steward is keeper, lives = 1");
  check(BigInt(g1.dev_burned) === 100_000_000n * TOKEN && BigInt(g1.dev_locked) === 0n, "dev's 100M bag sealed in the guard forever");
  const held = BigInt(await view(tokenId, "ft_balance_of", { account_id: guardId }));
  check(held === BigInt(g1.staked) + BigInt(g1.dev_burned), `guard holds exactly staked + sealed (${held / TOKEN} $MEME)`);
  const bobBefore = BigInt(await view(tokenId, "ft_balance_of", { account_id: acc.bob.accountId }));
  await call(acc.bob, guardId, "unstake", { amount: tokens(50_000_000) }, "0", teraToGas("100"));
  const bobAfter = BigInt(await view(tokenId, "ft_balance_of", { account_id: acc.bob.accountId }));
  check(bobAfter - bobBefore === 50_000_000n * TOKEN && (await view(guardId, "get_stake", { account_id: acc.bob.accountId })) === tokens(150_000_000),
    "bob unstaked 50M $MEME back to his wallet");
  if (stewardAcc) {
    await call(stewardAcc, guardId, "pulse");
    check((await view(guardId, "get_status")).status === "alive", "steward pulsed: MEME's guard alive under its new keeper");
  }
  result.coins.push(guardId);
  result.token = tokenId;
  }

  if (want("dao")) {
  // ------------------------- story 4: a real Sputnik DAO takes over and runs the coin
  log("Story 4: $CIVIC is taken over by a real Sputnik DAO, which then pulses and spends by proposal");
  const daoName = `civic-${tag}`;
  let daoId, daoSuffix4;
  const council = [acc.alice.accountId, acc.bob.accountId];
  const daoArgs = { config: { name: daoName, purpose: "Keeper of $CIVIC after the community takeover", metadata: "" }, policy: council };
  if (sputnikFactory) {
    daoId = `${daoName}.${sputnikFactory}`;
    daoSuffix4 = `.${sputnikFactory}`;
    if (!(resumeTag && await exists(daoId))) {
      await call(root, sputnikFactory, "create", { name: daoName, args: Buffer.from(JSON.stringify(daoArgs)).toString("base64") },
        nearToYocto(daoDeposit).toString(), teraToGas("300"));
    }
  } else {
    // Sandbox: no Sputnik factory here, so deploy the exact code testnet DAOs run, fetched from testnet.
    const r = await fetch(daoCodeFrom.rpc, { method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "query", params: { request_type: "view_code", account_id: daoCodeFrom.account, finality: "final" } }) });
    const code = Buffer.from((await r.json()).result.code_base64, "base64");
    log(`  fetched Sputnik DAO code from ${daoCodeFrom.account} (${code.length} bytes)`);
    await root.createSubAccount({ accountOrPrefix: "sputnikv2", publicKey: pub, nearToTransfer: nearToYocto(r2(Number(daoDeposit) + 0.2)) });
    const parent = as(`sputnikv2.${rootId}`);
    await parent.createSubAccount({ accountOrPrefix: daoName, publicKey: pub, nearToTransfer: nearToYocto(daoDeposit) });
    daoId = `${daoName}.sputnikv2.${rootId}`;
    daoSuffix4 = `.sputnikv2.${rootId}`;
    await as(daoId).deployContract(code);
    await call(as(daoId), daoId, "new", daoArgs, "0", teraToGas("100"));
  }
  const policy = await view(daoId, "get_policy");
  check(policy.roles.some((r) => JSON.stringify(r.kind).includes(acc.alice.accountId)), `real Sputnik DAO ${daoId} exists, council: alice and bob`);

  const civicName = existingFactory ? `civic-${tag}` : "civic";
  const civicId = `${civicName}.${factoryId}`;
  const takenOver = resumeTag && await exists(civicId) && (await view(civicId, "get_status")).keeper === daoId;
  let civic = civicId;
  if (takenOver) {
    log(`  resuming: ${civicId} is already kept by ${daoId}`);
  } else {
  civic = await launch(civicName, "CIVIC", "Civic Heart", daoSuffix4);
  check((await view(civic, "get_status")).config.dao_suffix === daoSuffix4, `$CIVIC only accepts DAOs ending in ${daoSuffix4}`);
  log("  dev goes silent");
  await waitPast(civic, "flatline");
  await call(acc.alice, civic, "start_takeover", { steward: daoId });
  await call(acc.alice, civic, "vote", { support: true });
  await call(acc.bob, civic, "vote", { support: true });
  await waitPast(civic, "vote");
  const ok4 = await call(acc.bob, civic, "finalize_takeover");
  check(ok4 === true, "takeover finalized");
  }
  const c1 = await view(civic, "get_status");
  check(c1.keeper === daoId && c1.lives === 1, `$CIVIC revived: keeper is the Sputnik DAO ${daoId}`);
  // The council's votes carry the gas for running each proposal, so make sure they can pay it.
  const topUp = async (a, min) => {
    const bal = BigInt(await new Account(a.accountId, provider).getBalance());
    const want = BigInt(nearToYocto(min));
    if (bal < want) await root.transfer({ receiverId: a.accountId, amount: (want - bal).toString() });
  };
  await topUp(acc.alice, "1.4");
  await topUp(acc.bob, "0.5");
  check(await fails(call(acc.dev, civic, "pulse"), "keeper"), "the old dev can no longer pulse");

  // The DAO runs the coin: every action is a proposal its council votes on.
  const bond = policy.proposal_bond;
  async function daoDoes(description, method, args, gas, deposit = "0") {
    const kind = { FunctionCall: { receiver_id: civic, actions: [{
      method_name: method, args: Buffer.from(JSON.stringify(args)).toString("base64"), deposit, gas: teraToGas(gas).toString() }] } };
    await call(acc.alice, daoId, "add_proposal", { proposal: { description, kind } }, bond, teraToGas("50"));
    const id = (await view(daoId, "get_last_proposal_id")) - 1;
    await call(acc.alice, daoId, "act_proposal", { id, action: "VoteApprove", proposal: kind }, "0", teraToGas("100"));
    await call(acc.bob, daoId, "act_proposal", { id, action: "VoteApprove", proposal: kind }, "0", teraToGas("150"));
    return (await view(daoId, "get_proposal", { id })).status;
  }
  const s4 = await view(civic, "get_status");
  const before = s4.last_pulse;
  // If the coin has gone flat again by the time the DAO votes, its pulse is a comeback and must pay
  // the penalty into the treasury, so the proposal attaches it. The DAO is sent the NEAR to pay it.
  const late = s4.now > s4.flatline_at - 40;
  const penalty = late ? s4.config.comeback_penalty : "0";
  if (late) await root.transfer({ receiverId: daoId, amount: (BigInt(penalty) + BigInt(nearToYocto("0.1"))).toString() });
  await sleep(2);
  const st1 = await daoDoes(late ? "Bring $CIVIC back: pay the comeback penalty and pulse" : "Pulse $CIVIC: the community keeps it alive",
    "pulse", {}, "30", penalty);
  const c2 = await view(civic, "get_status");
  check(st1 === "Approved" && c2.last_pulse > before && c2.status === "alive", "DAO proposal approved by alice and bob: the DAO pulsed $CIVIC");
  const carolBefore = BigInt((await new Account(acc.carol.accountId, provider).getBalance()));
  const st2 = await daoDoes("Pay carol 0.05 NEAR from the $CIVIC treasury for community work", "spend",
    { receiver_id: acc.carol.accountId, amount: nearToYocto("0.05").toString(), memo: "community bounty, approved by DAO vote" }, "50");
  const carolAfter = BigInt((await new Account(acc.carol.accountId, provider).getBalance()));
  check(st2 === "Approved" && carolAfter - carolBefore === BigInt(nearToYocto("0.05")), "DAO proposal approved: carol was paid 0.05 NEAR from the coin's treasury");
  check(await fails(call(acc.alice, civic, "spend", { receiver_id: acc.alice.accountId, amount: "1", memo: "x" }), "keeper"),
    "a council member alone cannot spend: only the DAO, by vote");
  result.coins.push(civic);
  result.dao = daoId;
  }

  const registry = await view(factoryId, "get_coins", {});
  check((existingFactory ? registry.length >= result.coins.length : registry.length === result.coins.length) && registry.every((c) => c.kind === (c.token_id ? "guard" : "coin")), `factory registry lists ${result.coins.length} entries with their kinds`);
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
  // Drop anything that can't be part of a base58 key (line breaks, quotes, UI text like a stray "0").
  const cleanBody = [...body].filter((c) => B58.test(c)).join("");
  const NEAR_PRIVATE_KEY = cleanBody ? "ed25519:" + cleanBody : undefined;
  const NEAR_ACCOUNT_ID = String(process.env.NEAR_ACCOUNT_ID || "").replace(/["'\s]/g, "");
  if (!NEAR_ACCOUNT_ID || !NEAR_PRIVATE_KEY) {
    console.error("Set NEAR_ACCOUNT_ID and NEAR_PRIVATE_KEY (ed25519:...) for a funded testnet account.");
    process.exit(1);
  }
  run({
    rpc: process.env.NEAR_RPC || "https://test.rpc.fastnear.com",
    rootId: NEAR_ACCOUNT_ID,
    rootKey: NEAR_PRIVATE_KEY,
    timer: Number(process.env.TIMER || 60),
    steward: process.env.STEWARD,
    daoSuffix: process.env.DAO_SUFFIX,
    stories: (process.env.STORIES || "coins,guard").split(",").map((x) => x.trim()).filter(Boolean),
    holderNear: process.env.HOLDER_NEAR || "0.15",
    existingFactory: process.env.FACTORY || undefined,
    resumeTag: process.env.RESUME_TAG || undefined,
    sputnikFactory: process.env.SPUTNIK_FACTORY || undefined,
    out: process.env.OUT || "demo-coins.json",
  }).catch((e) => { console.error(e); process.exit(1); });
}
