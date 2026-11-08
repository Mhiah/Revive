#!/usr/bin/env bash
# Deploy a Revive coin to NEAR testnet with near-cli-rs (https://near.cli.rs).
#
#   CREATOR=you.testnet COIN=nine.you.testnet ./scripts/deploy-testnet.sh
#
# Short timers (2-minute pulse, 2-minute vote) so the whole story can be
# played live in a demo. Use days for a real coin.
set -euo pipefail

: "${CREATOR:?set CREATOR to your testnet account, e.g. you.testnet}"
: "${COIN:?set COIN to a new sub-account, e.g. nine.you.testnet}"
WASM="${WASM:-contract/target/near/revive.wasm}"
NET="network-config testnet"

if [ ! -f "$WASM" ]; then
  echo "Building $WASM (needs cargo-near)…"
  (cd contract && cargo near build non-reproducible-wasm)
fi

echo "1/4 Create $COIN (funded by $CREATOR)"
near account create-account fund-myself "$COIN" '3 NEAR' autogenerate-new-keypair save-to-keychain \
  sign-as "$CREATOR" $NET sign-with-keychain send

echo "2/4 Deploy code (no init yet)"
near contract deploy "$COIN" use-file "$WASM" without-init-call $NET sign-with-keychain send

echo "3/4 Launch from $CREATOR, who becomes creator and first keeper; 1 NEAR seeds the treasury"
near contract call-function as-transaction "$COIN" new json-args "$(cat <<JSON
{"config":{
  "name":"Nine Lives","symbol":"NINE","icon":null,"links":"https://github.com/Mhiah/Revive",
  "total_supply":"1000000000000000000000000000",
  "dev_allocation":"100000000000000000000000000",
  "distributor":"$CREATOR",
  "pulse_interval_secs":120,
  "vote_duration_secs":120,
  "quorum_bps":2000,
  "vest_duration_secs":86400,
  "spend_cap":"500000000000000000000000",
  "spend_period_secs":3600,
  "comeback_penalty":"200000000000000000000000",
  "dao_suffix":".sputnikv2.testnet"
}}
JSON
)" prepaid-gas '100.0 Tgas' attached-deposit '1 NEAR' sign-as "$CREATOR" $NET sign-with-keychain send

echo "4/4 Delete the coin account's full-access key: nobody can change the code after this"
KEY=$(near account list-keys "$COIN" $NET now 2>/dev/null | grep -oE 'ed25519:[1-9A-HJ-NP-Za-km-z]+' | head -1 || true)
if [ -n "$KEY" ]; then
  near account delete-keys "$COIN" public-keys "$KEY" $NET sign-with-keychain send
else
  echo "Could not read the key; delete it yourself with: near account delete-keys $COIN"
fi

echo
echo "Done. Open the page with:  web/index.html?contract=$COIN"
