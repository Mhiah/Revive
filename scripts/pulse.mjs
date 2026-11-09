// Keeps a coin alive: its keeper sends a pulse. Run by the keep-alive workflow.
//   COIN=here.revive-xxxxx.you.testnet KEEPER=dev-xxxxx.you.testnet \
//   NEAR_ACCOUNT_ID=you.testnet NEAR_PRIVATE_KEY=ed25519:... node scripts/pulse.mjs
// The demo accounts share the root account's key, so the same secret signs for the keeper.
// If the keeper runs low on NEAR for gas, the root account tops it up.
import { Account, JsonRpcProvider, nearToYocto } from "near-api-js";

const B58 = /[1-9A-HJ-NP-Za-km-z]/;
const raw = String(process.env.NEAR_PRIVATE_KEY || "");
const body = raw.includes(":") ? raw.slice(raw.lastIndexOf(":") + 1) : raw;
const key = "ed25519:" + [...body].filter((c) => B58.test(c)).join("");
const clean = (v) => String(v || "").replace(/["'\s]/g, "");
const rootId = clean(process.env.NEAR_ACCOUNT_ID), coin = clean(process.env.COIN), keeperId = clean(process.env.KEEPER);
if (!rootId || !coin || !keeperId || key.length < 20) {
  console.error("Set COIN, KEEPER, NEAR_ACCOUNT_ID and NEAR_PRIVATE_KEY.");
  process.exit(1);
}
const provider = new JsonRpcProvider({ url: process.env.NEAR_RPC || "https://test.rpc.fastnear.com" });
const view = (method) => provider.callFunction({ contractId: coin, method, args: {} });
const keeper = new Account(keeperId, provider, key);

const before = await view("get_status");
if (before.keeper !== keeperId) {
  console.log(`${coin} is kept by ${before.keeper} now, not ${keeperId}; nothing to do.`);
  process.exit(0);
}
const low = BigInt(nearToYocto("0.3"));
if (BigInt(await keeper.getBalance()) < low) {
  await new Account(rootId, provider, key).transfer({ receiverId: keeperId, amount: nearToYocto("1").toString() });
  console.log(`topped up ${keeperId} with 1 NEAR for gas`);
}
const late = before.now > before.flatline_at;
await keeper.callFunction({ contractId: coin, methodName: "pulse", args: {}, deposit: late ? before.config.comeback_penalty : "0" });
const after = await view("get_status");
console.log(`${coin}: ${before.status} -> ${after.status}; next deadline in ${Math.round((after.flatline_at - after.now) / 3600)}h${late ? " (late: paid the comeback penalty)" : ""}`);
if (after.status !== "alive") process.exit(1);
