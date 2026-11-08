//! Revive factory: anyone launches a Revive coin as `<name>.<factory>`.
//!
//! The owner stores the Revive wasm once (`store_code`). After that the code
//! can never change, every coin gets the same code, and its code hash is
//! public through `get_code_hash`. The deploy script then deletes the
//! factory's keys, so nobody controls it.

use near_sdk::json_types::{Base58CryptoHash, U128};
use near_sdk::serde_json::{self, json, Value};
use near_sdk::store::Vector;
use near_sdk::{env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise, PromiseError};

const CODE_KEY: &[u8] = b"code";
const GUARD_KEY: &[u8] = b"guard";
/// NEP-145 registration the factory pays so the guard can hold the token.
const TOKEN_REGISTRATION: u128 = 1_250_000_000_000_000_000_000; // 0.00125 NEAR
const GAS_METADATA: Gas = Gas::from_tgas(5);
const GAS_CONNECT_STEP: Gas = Gas::from_tgas(230);
const GAS_GUARD_INIT: Gas = Gas::from_tgas(60);
const GAS_AFTER_CREATE: Gas = Gas::from_tgas(40);
const GAS_STORAGE_DEPOSIT: Gas = Gas::from_tgas(10);
/// Room for the coin's state on top of its code: account, config, first holders, lore.
const STATE_BYTES: u128 = 10_000;
const GAS_FOR_INIT: Gas = Gas::from_tgas(60);
const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(15);

#[near(serializers = [borsh])]
#[derive(BorshStorageKey)]
enum StorageKey {
    Coins,
}

#[allow(dead_code)]
#[ext_contract(ext_token)]
pub trait Token {
    fn ft_metadata(&self) -> TokenMetadata;
    fn storage_deposit(&mut self, account_id: Option<AccountId>, registration_only: Option<bool>);
}

#[near(serializers = [json])]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct CoinEntry {
    pub account_id: AccountId,
    pub name: String,
    pub symbol: String,
    pub creator: AccountId,
    pub launched_at: u64,
    /// "coin" for a Revive coin, "guard" for a connected outside token.
    pub kind: String,
    /// The outside token a guard protects.
    pub token_id: Option<AccountId>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Factory {
    owner: AccountId,
    code_hash: Option<Vec<u8>>,
    code_len: u64,
    guard_hash: Option<Vec<u8>>,
    guard_len: u64,
    coins: Vector<CoinEntry>,
}

#[near]
impl Factory {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        Self { owner, code_hash: None, code_len: 0, guard_hash: None, guard_len: 0, coins: Vector::new(StorageKey::Coins) }
    }

    /// Owner stores the Revive wasm, sent as the raw transaction input. Once only.
    pub fn store_code(&mut self) {
        let code = env::input().unwrap_or_else(|| env::panic_str("send the wasm as the input"));
        self.internal_store_code(code);
    }

    /// Owner stores the guard wasm (for connected outside tokens). Once only.
    pub fn store_guard_code(&mut self) {
        let code = env::input().unwrap_or_else(|| env::panic_str("send the wasm as the input"));
        self.internal_store_guard_code(code);
    }

    /// Connect an existing NEP-141 token as `<name>.<factory>`: a guard that
    /// holds the creator's locked bag, holders' stakes and a NEAR treasury under
    /// Revive's takeover rules. The token itself is not changed. The factory
    /// first reads the token's ft_metadata, so only real tokens get connected.
    #[payable]
    pub fn connect(&mut self, name: String, config: Value) -> Promise {
        require!(self.guard_hash.is_some(), "the factory has no guard code yet");
        Self::check_name(&name);
        let deposit = env::attached_deposit().as_yoctonear();
        let cost = self.get_connect_cost().0;
        require!(deposit >= cost, format!("attach at least {} yoctoNEAR (get_connect_cost)", cost));
        let token_id: AccountId = config
            .get("token_id")
            .and_then(Value::as_str)
            .and_then(|t| t.parse().ok())
            .unwrap_or_else(|| env::panic_str("config.token_id must be a token account"));
        ext_token::ext(token_id)
            .with_static_gas(GAS_METADATA)
            .ft_metadata()
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CONNECT_STEP).on_token_checked(
                name,
                config,
                env::predecessor_account_id(),
                U128(deposit),
            ))
    }

    #[private]
    pub fn on_token_checked(
        &mut self,
        name: String,
        config: Value,
        creator: AccountId,
        deposit: U128,
        #[callback_result] meta: Result<TokenMetadata, PromiseError>,
    ) -> Promise {
        let Ok(meta) = meta else {
            env::log_str("not a NEP-141 token: ft_metadata failed; refunding");
            return Promise::new(creator).transfer(NearToken::from_yoctonear(deposit.0));
        };
        let storage = (self.guard_len as u128 + STATE_BYTES) * env::storage_byte_cost().as_yoctonear();
        let treasury = deposit.0 - storage - TOKEN_REGISTRATION;
        let account: AccountId = format!("{}.{}", name, env::current_account_id()).parse().unwrap();
        let mut config = config;
        let obj = config.as_object_mut().unwrap_or_else(|| env::panic_str("config must be an object"));
        obj.insert("name".into(), json!(meta.name));
        obj.insert("symbol".into(), json!(meta.symbol));
        obj.insert("decimals".into(), json!(meta.decimals));
        obj.insert("creator".into(), json!(creator));
        let token_id = obj.get("token_id").cloned().unwrap();
        let args = serde_json::to_vec(&json!({ "config": config })).unwrap();
        let code = env::storage_read(GUARD_KEY).unwrap();
        Promise::new(account.clone())
            .create_account()
            .transfer(NearToken::from_yoctonear(storage))
            .deploy_contract(code)
            .function_call("new", args, NearToken::from_yoctonear(treasury), GAS_GUARD_INIT)
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_AFTER_CREATE).on_guard_created(
                account,
                meta.name,
                meta.symbol,
                creator,
                serde_json::from_value(token_id).unwrap(),
                deposit,
            ))
    }

    /// The guard exists: register it with the token so it can receive tokens.
    #[private]
    pub fn on_guard_created(
        &mut self,
        account_id: AccountId,
        name: String,
        symbol: String,
        creator: AccountId,
        token_id: AccountId,
        deposit: U128,
    ) -> Promise {
        if env::promise_result_checked(0, 1_000).is_err() {
            env::log_str("creating the guard failed; refunding");
            return Promise::new(creator).transfer(NearToken::from_yoctonear(deposit.0));
        }
        self.coins.push(CoinEntry {
            account_id: account_id.clone(),
            name,
            symbol,
            creator: creator.clone(),
            launched_at: env::block_timestamp() / 1_000_000_000,
            kind: "guard".into(),
            token_id: Some(token_id.clone()),
        });
        env::log_str(&format!("EVENT_JSON:{}", json!({
            "standard": "revive-factory", "version": "1.0.0", "event": "connected",
            "data": [{ "account_id": account_id, "token_id": token_id, "creator": creator }]
        })));
        ext_token::ext(token_id)
            .with_static_gas(GAS_STORAGE_DEPOSIT)
            .with_attached_deposit(NearToken::from_yoctonear(TOKEN_REGISTRATION))
            .storage_deposit(Some(account_id), Some(true))
    }

    /// Launch a coin as `<name>.<factory>`. The caller becomes the creator and
    /// first keeper. Attach at least `get_launch_cost()` for storage; anything
    /// above that seeds the coin's treasury.
    #[payable]
    pub fn launch(&mut self, name: String, config: Value) -> Promise {
        require!(self.code_hash.is_some(), "the factory has no code yet");
        Self::check_name(&name);
        let deposit = env::attached_deposit().as_yoctonear();
        let storage = self.get_launch_cost().0;
        require!(deposit >= storage, format!("attach at least {} yoctoNEAR (get_launch_cost)", storage));
        let treasury = deposit - storage;

        let creator = env::predecessor_account_id();
        let account: AccountId = format!("{}.{}", name, env::current_account_id()).parse().unwrap_or_else(|_| env::panic_str("invalid name"));
        let mut config = config;
        let obj = config.as_object_mut().unwrap_or_else(|| env::panic_str("config must be an object"));
        obj.insert("creator".into(), json!(creator));
        let coin_name = obj.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
        let symbol = obj.get("symbol").and_then(Value::as_str).unwrap_or_default().to_string();
        let args = serde_json::to_vec(&json!({ "config": config })).unwrap();

        let code = env::storage_read(CODE_KEY).unwrap();
        Promise::new(account.clone())
            .create_account()
            .transfer(NearToken::from_yoctonear(storage))
            .deploy_contract(code)
            .function_call("new", args, NearToken::from_yoctonear(treasury), GAS_FOR_INIT)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_CALLBACK)
                    .on_launch(account, coin_name, symbol, creator, U128(deposit)),
            )
    }

    /// Records the coin if it launched; refunds the creator if it didn't.
    #[private]
    pub fn on_launch(&mut self, account_id: AccountId, name: String, symbol: String, creator: AccountId, deposit: U128) -> bool {
        let ok = env::promise_result_checked(0, 1_000).is_ok();
        if ok {
            self.coins.push(CoinEntry {
                account_id: account_id.clone(),
                name,
                symbol,
                creator: creator.clone(),
                launched_at: env::block_timestamp() / 1_000_000_000,
                kind: "coin".into(),
                token_id: None,
            });
            env::log_str(&format!("EVENT_JSON:{}", json!({
                "standard": "revive-factory", "version": "1.0.0", "event": "launched",
                "data": [{ "account_id": account_id, "creator": creator }]
            })));
        } else {
            // A failed batch returns its NEAR to the factory, so pay the creator back.
            let _ = Promise::new(creator).transfer(NearToken::from_yoctonear(deposit.0));
        }
        ok
    }

    // ------------------------------------------------------------- views


    pub fn get_coins(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<CoinEntry> {
        let from = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(50).min(100);
        (from..self.coins.len().min(from.saturating_add(limit))).filter_map(|i| self.coins.get(i).cloned()).collect()
    }

    pub fn get_coin_count(&self) -> u32 {
        self.coins.len()
    }

    /// Base58 sha256 of the stored wasm. Every coin's account code hash equals this.
    pub fn get_code_hash(&self) -> Option<Base58CryptoHash> {
        self.code_hash.as_ref().map(|h| {
            let arr: [u8; 32] = h.as_slice().try_into().unwrap();
            Base58CryptoHash::from(arr)
        })
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    pub fn get_code_len(&self) -> u64 {
        self.code_len
    }

    /// Base58 sha256 of the stored guard wasm. Every guard's code hash equals this.
    pub fn get_guard_code_hash(&self) -> Option<Base58CryptoHash> {
        self.guard_hash.as_ref().map(|h| {
            let arr: [u8; 32] = h.as_slice().try_into().unwrap();
            Base58CryptoHash::from(arr)
        })
    }

    /// NEAR (yocto) a connect needs: the guard's code and state, plus its registration on the token.
    pub fn get_connect_cost(&self) -> U128 {
        U128((self.guard_len as u128 + STATE_BYTES) * env::storage_byte_cost().as_yoctonear() + TOKEN_REGISTRATION)
    }

    /// NEAR (yocto) a launch keeps for the new coin's storage: its code plus room for state.
    pub fn get_launch_cost(&self) -> U128 {
        U128((self.code_len as u128 + STATE_BYTES) * env::storage_byte_cost().as_yoctonear())
    }
}

impl Factory {
    fn check_name(name: &str) {
        require!(
            name.len() >= 2 && name.len() <= 32 && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'),
            "name must be 2-32 chars of a-z, 0-9, - or _"
        );
    }

    fn internal_store_guard_code(&mut self, code: Vec<u8>) {
        require!(env::predecessor_account_id() == self.owner, "only the owner stores the code");
        require!(self.guard_hash.is_none(), "the guard code is already stored and can never change");
        require!(code.len() > 1_000 && code.starts_with(b"\0asm"), "input is not a wasm file");
        let hash = env::sha256(&code);
        env::storage_write(GUARD_KEY, &code);
        self.guard_len = code.len() as u64;
        self.guard_hash = Some(hash);
        env::log_str(&format!("EVENT_JSON:{}", json!({
            "standard": "revive-factory", "version": "1.0.0", "event": "guard_code_stored",
            "data": [{ "code_hash": self.get_guard_code_hash(), "bytes": code.len() }]
        })));
    }

    fn internal_store_code(&mut self, code: Vec<u8>) {
        require!(env::predecessor_account_id() == self.owner, "only the owner stores the code");
        require!(self.code_hash.is_none(), "the code is already stored and can never change");
        require!(code.len() > 1_000 && code.starts_with(b"\0asm"), "input is not a wasm file");
        let hash = env::sha256(&code);
        env::storage_write(CODE_KEY, &code);
        self.code_len = code.len() as u64;
        self.code_hash = Some(hash);
        env::log_str(&format!("EVENT_JSON:{}", json!({
            "standard": "revive-factory", "version": "1.0.0", "event": "code_stored",
            "data": [{ "code_hash": self.get_code_hash(), "bytes": code.len() }]
        })));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    fn ctx(who: &str, deposit: u128, _input: Vec<u8>) {
        let mut b = VMContextBuilder::new();
        b.current_account_id("revive.testnet".parse().unwrap())
            .predecessor_account_id(who.parse().unwrap())
            .attached_deposit(NearToken::from_yoctonear(deposit));
        testing_env!(b.build());
    }

    fn fake_wasm() -> Vec<u8> {
        let mut w = b"\0asm".to_vec();
        w.extend(vec![7u8; 2_000]);
        w
    }

    #[test]
    fn stores_code_once_and_exposes_hash() {
        ctx("mia.testnet", 0, fake_wasm());
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_code(fake_wasm());
        assert!(f.get_code_hash().is_some());
        assert_eq!(f.get_code_len(), 2_004);
    }

    #[test]
    #[should_panic(expected = "the code is already stored and can never change")]
    fn code_cannot_change() {
        ctx("mia.testnet", 0, fake_wasm());
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_code(fake_wasm());
        f.internal_store_code(fake_wasm());
    }

    #[test]
    #[should_panic(expected = "only the owner stores the code")]
    fn only_owner_stores_code() {
        ctx("mallory.testnet", 0, fake_wasm());
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_code(fake_wasm());
    }

    #[test]
    #[should_panic(expected = "name must be 2-32 chars")]
    fn rejects_bad_names() {
        ctx("mia.testnet", 0, fake_wasm());
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_code(fake_wasm());
        ctx("bob.testnet", 2 * 10u128.pow(24), vec![]);
        let _ = f.launch("Bad.Name".into(), json!({ "name": "x", "symbol": "X" }));
    }

    #[test]
    #[should_panic(expected = "attach at least")]
    fn launch_needs_storage_deposit() {
        ctx("mia.testnet", 0, fake_wasm());
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_code(fake_wasm());
        ctx("bob.testnet", 1, vec![]);
        let _ = f.launch("nine".into(), json!({ "name": "Nine", "symbol": "NINE" }));
    }

    #[test]
    fn guard_code_once_and_connect_cost() {
        ctx("mia.testnet", 0, vec![]);
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_guard_code(fake_wasm());
        assert!(f.get_guard_code_hash().is_some());
        assert_eq!(f.get_connect_cost().0, (2_004 + 10_000) * env::storage_byte_cost().as_yoctonear() + TOKEN_REGISTRATION);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.internal_store_guard_code(fake_wasm())));
        assert!(r.is_err(), "the guard code can never change");
    }

    #[test]
    #[should_panic(expected = "config.token_id must be a token account")]
    fn connect_needs_a_token() {
        ctx("mia.testnet", 0, vec![]);
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_guard_code(fake_wasm());
        ctx("bob.testnet", 5 * 10u128.pow(24), vec![]);
        let _ = f.connect("meme".into(), json!({ "pulse_interval_secs": 60 }));
    }

    #[test]
    fn connect_builds_the_metadata_check() {
        ctx("mia.testnet", 0, vec![]);
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_guard_code(fake_wasm());
        ctx("bob.testnet", 5 * 10u128.pow(24), vec![]);
        let _ = f.connect("meme".into(), json!({ "token_id": "meme.cooking.testnet" }));
    }

    #[test]
    fn launch_cost_follows_code_size() {
        ctx("mia.testnet", 0, vec![]);
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_code(fake_wasm());
        assert_eq!(f.get_launch_cost().0, (2_004 + 10_000) * env::storage_byte_cost().as_yoctonear());
    }

    #[test]
    fn launch_builds_the_promise() {
        ctx("mia.testnet", 0, fake_wasm());
        let mut f = Factory::new("mia.testnet".parse().unwrap());
        f.internal_store_code(fake_wasm());
        ctx("bob.testnet", 2 * 10u128.pow(24), vec![]);
        let _ = f.launch("nine".into(), json!({ "name": "Nine", "symbol": "NINE" }));
    }
}
