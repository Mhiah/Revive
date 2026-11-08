// End-to-end test: runs the demo-coins script against a local NEAR sandbox.
// Real wasm, real cross-contract calls, real clock. Used by CI.
import { Sandbox, DEFAULT_ACCOUNT_ID, DEFAULT_PRIVATE_KEY } from "near-sandbox";
import { run } from "./demo-coins.mjs";

const sandbox = await Sandbox.start({});
let code = 0;
try {
  await run({
    rpc: sandbox.rpcUrl,
    rootId: DEFAULT_ACCOUNT_ID,
    rootKey: DEFAULT_PRIVATE_KEY,
    timer: 60,
    out: "sandbox-result.json",
  });
} catch (e) {
  console.error(e);
  code = 1;
} finally {
  await sandbox.tearDown();
}
process.exit(code);
