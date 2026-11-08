# Running the coin from a Sputnik DAO

After a revival the keeper is a Sputnik DAO. The DAO keeps the coin alive by
passing a FunctionCall proposal that calls `pulse` on the coin.

## Create the steward DAO (testnet)

```bash
ARGS=$(echo -n '{"config":{"name":"ninelives","purpose":"Keeper of $NINE","metadata":""},"policy":["alice.testnet","bob.testnet"]}' | base64 -w0)
near contract call-function as-transaction sputnikv2.testnet create \
  json-args "{\"name\":\"ninelives\",\"args\":\"$ARGS\"}" \
  prepaid-gas '150.0 Tgas' attached-deposit '6 NEAR' \
  sign-as alice.testnet network-config testnet sign-with-keychain send
```

The DAO's account is `ninelives.sputnikv2.testnet`, which is what holders
nominate in `start_takeover`. Check the factory's current deposit before
running this; it may change.

## Pulse from the DAO

```bash
near contract call-function as-transaction ninelives.sputnikv2.testnet add_proposal json-args '{
  "proposal": {
    "description": "Keep $NINE alive",
    "kind": { "FunctionCall": {
      "receiver_id": "nine.you.testnet",
      "actions": [{ "method_name": "pulse", "args": "e30=", "deposit": "0", "gas": "30000000000000" }]
    }}
  }
}' prepaid-gas '100.0 Tgas' attached-deposit '0.1 NEAR' \
  sign-as alice.testnet network-config testnet sign-with-keychain send
```

Council members then approve it with `act_proposal` (`"action": "VoteApprove"`).
`e30=` is base64 for `{}`. The same pattern works for `spend` and `set_links`.
