//! The factory's refunds, its registry, and the rules on code and names.
use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{test_vm_config, testing_env, PromiseResult, RuntimeFeesConfig};

const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const FACTORY: &str = "revive.testnet";

fn at(who: &str, deposit: u128) {
    let mut b = VMContextBuilder::new();
    b.current_account_id(FACTORY.parse().unwrap())
        .predecessor_account_id(who.parse().unwrap())
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .block_timestamp(1_800_000_000 * 1_000_000_000);
    testing_env!(b.build());
}

/// The factory calling itself back, with the result of the promise it waited on.
fn callback_with(result: PromiseResult) {
    let mut b = VMContextBuilder::new();
    b.current_account_id(FACTORY.parse().unwrap())
        .predecessor_account_id(FACTORY.parse().unwrap())
        .block_timestamp(1_800_000_000 * 1_000_000_000);
    testing_env!(b.build(), test_vm_config(), RuntimeFeesConfig::test(), Default::default(), vec![result]);
}

fn wasm() -> Vec<u8> {
    let mut w = b"\0asm".to_vec();
    w.extend(vec![1u8; 3_000]);
    w
}

fn factory() -> Factory {
    at("mia.testnet", 0);
    near_sdk::mock::with_mocked_blockchain(|b| {
        let _ = b.take_storage();
    });
    let mut f = Factory::new("mia.testnet".parse().unwrap(), vec![".sputnikv2.testnet".to_string()]);
    f.internal_store_code(wasm());
    f.internal_store_guard_code(wasm());
    f
}

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

#[test]
fn a_successful_launch_is_recorded() {
    let mut f = factory();
    callback_with(PromiseResult::Successful(vec![]));
    assert!(f.on_launch(acc("nine.revive.testnet"), "Nine".into(), "NINE".into(), acc("bob.testnet"), U128(3 * NEAR)));
    let coins = f.get_coins(None, None);
    assert_eq!(coins.len(), 1);
    assert_eq!(coins[0].kind, "coin");
    assert_eq!(coins[0].creator, acc("bob.testnet"));
}

#[test]
fn a_failed_launch_records_nothing() {
    let mut f = factory();
    callback_with(PromiseResult::Failed);
    assert!(!f.on_launch(acc("nine.revive.testnet"), "Nine".into(), "NINE".into(), acc("bob.testnet"), U128(3 * NEAR)));
    assert_eq!(f.get_coin_count(), 0, "a failed launch is refunded, not listed");
}

#[test]
fn a_token_that_fails_its_metadata_check_is_refunded() {
    let mut f = factory();
    callback_with(PromiseResult::Failed);
    let _ = f.on_token_checked("meme".into(), json!({ "token_id": "notatoken.testnet" }), acc("bob.testnet"), U128(3 * NEAR));
    assert_eq!(f.get_coin_count(), 0);
}

/// A token whose ft_metadata answers with the wrong shape gets the same refund. Before,
/// the callback panicked while reading the answer and the deposit stayed in the factory.
#[test]
fn a_token_with_malformed_metadata_is_refunded_not_stuck() {
    for bad in [&b"{\"name\":\"x\"}"[..], b"not json", b"{\"name\":\"x\",\"symbol\":\"X\",\"decimals\":200}", b"{\"name\":\"x\",\"symbol\":\"\",\"decimals\":18}"] {
        let mut f = factory();
        callback_with(PromiseResult::Successful(bad.to_vec()));
        let _ = f.on_token_checked("meme".into(), json!({ "token_id": "odd.testnet" }), acc("bob.testnet"), U128(3 * NEAR));
        assert_eq!(f.get_coin_count(), 0);
    }
}

#[test]
fn a_real_token_goes_on_to_create_the_guard() {
    let mut f = factory();
    let meta = br#"{"spec":"ft-1.0.0","name":"Meme","symbol":"MEME","decimals":18,"icon":null}"#;
    callback_with(PromiseResult::Successful(meta.to_vec()));
    let _ = f.on_token_checked("meme".into(), json!({ "token_id": "meme.testnet", "dao_suffix": ".sputnikv2.testnet" }), acc("bob.testnet"), U128(3 * NEAR));
}

fn panics<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> Option<String> {
    std::panic::catch_unwind(f).err().map(|e| {
        e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
    })
}

#[test]
fn launch_only_accepts_the_factorys_dao_suffixes() {
    let mut f = factory();
    at("bob.testnet", 5 * NEAR);
    let cfg = json!({ "name": "Nine", "symbol": "NINE", "dao_suffix": ".testnet" });
    let err = panics(std::panic::AssertUnwindSafe(|| { let _ = f.launch("nine".into(), cfg.clone()); })).expect("a made-up suffix is refused");
    assert!(err.contains("dao_suffix must be one of"), "{err}");
}

#[test]
fn a_missing_dao_suffix_becomes_the_factorys_own() {
    let f = factory();
    let mut cfg = json!({ "name": "Nine" });
    f.apply_dao_suffix(&mut cfg);
    assert_eq!(cfg["dao_suffix"], ".sputnikv2.testnet");
    assert_eq!(f.get_dao_suffixes(), vec![".sputnikv2.testnet".to_string()]);
}

#[test]
fn a_factory_needs_at_least_one_valid_dao_suffix() {
    at("mia.testnet", 0);
    assert!(panics(|| { let _ = Factory::new(acc("mia.testnet"), vec![]); }).is_some());
    assert!(panics(|| { let _ = Factory::new(acc("mia.testnet"), vec!["sputnik".into()]); }).is_some());
}

#[test]
fn a_guard_the_token_refused_to_register_is_flagged() {
    let mut f = factory();
    callback_with(PromiseResult::Failed);
    assert!(!f.on_registered(acc("meme.revive.testnet"), acc("meme.testnet")));
    let logs = near_sdk::test_utils::get_logs();
    assert!(logs.iter().any(|l| l.contains("guard_not_registered")), "{logs:?}");
}

#[test]
fn a_failed_guard_deploy_records_nothing() {
    let mut f = factory();
    callback_with(PromiseResult::Failed);
    let _ = f.on_guard_created(acc("meme.revive.testnet"), "Meme".into(), "MEME".into(), acc("bob.testnet"), acc("meme.cooking.testnet"), U128(3 * NEAR));
    assert_eq!(f.get_coin_count(), 0);
}

#[test]
fn a_successful_connect_is_recorded_as_a_guard() {
    let mut f = factory();
    callback_with(PromiseResult::Successful(vec![]));
    let _ = f.on_guard_created(acc("meme.revive.testnet"), "Meme".into(), "MEME".into(), acc("bob.testnet"), acc("meme.cooking.testnet"), U128(3 * NEAR));
    let coins = f.get_coins(None, None);
    assert_eq!(coins.len(), 1);
    assert_eq!(coins[0].kind, "guard");
    assert_eq!(coins[0].token_id, Some(acc("meme.cooking.testnet")));
}

#[test]
#[should_panic(expected = "the guard code is already stored and can never change")]
fn the_guard_code_cannot_change() {
    let mut f = factory();
    at("mia.testnet", 0);
    f.internal_store_guard_code(wasm());
}

#[test]
#[should_panic(expected = "only the owner stores the code")]
fn only_the_owner_stores_guard_code() {
    at("mia.testnet", 0);
    let mut f = Factory::new(acc("mia.testnet"), vec![".sputnikv2.testnet".to_string()]);
    at("mallory.testnet", 0);
    f.internal_store_guard_code(wasm());
}

#[test]
#[should_panic(expected = "input is not a wasm file")]
fn non_wasm_code_is_refused() {
    at("mia.testnet", 0);
    let mut f = Factory::new(acc("mia.testnet"), vec![".sputnikv2.testnet".to_string()]);
    f.internal_store_code(vec![0u8; 5_000]);
}

#[test]
#[should_panic(expected = "the factory has no guard code yet")]
fn connect_needs_the_guard_code() {
    at("mia.testnet", 0);
    let mut f = Factory::new(acc("mia.testnet"), vec![".sputnikv2.testnet".to_string()]);
    at("bob.testnet", 5 * NEAR);
    let _ = f.connect("meme".into(), json!({ "token_id": "meme.cooking.testnet" }));
}

#[test]
#[should_panic(expected = "attach at least")]
fn connect_needs_its_deposit() {
    let mut f = factory();
    at("bob.testnet", 1);
    let _ = f.connect("meme".into(), json!({ "token_id": "meme.cooking.testnet" }));
}

#[test]
#[should_panic(expected = "name must be 2-32 chars")]
fn names_over_32_chars_are_refused() {
    let mut f = factory();
    at("bob.testnet", 5 * NEAR);
    let _ = f.launch("a".repeat(33), json!({ "name": "x", "symbol": "X" }));
}

#[test]
#[should_panic(expected = "name must be 2-32 chars")]
fn one_letter_names_are_refused() {
    let mut f = factory();
    at("bob.testnet", 5 * NEAR);
    let _ = f.launch("a".into(), json!({ "name": "x", "symbol": "X" }));
}

#[test]
fn the_registry_pages_and_caps_its_answers() {
    let mut f = factory();
    for i in 0..130 {
        callback_with(PromiseResult::Successful(vec![]));
        f.on_launch(acc(&format!("c{i}.revive.testnet")), format!("C{i}"), "C".into(), acc("bob.testnet"), U128(NEAR));
    }
    assert_eq!(f.get_coin_count(), 130);
    assert_eq!(f.get_coins(None, None).len(), 50, "default page");
    assert_eq!(f.get_coins(Some(0), Some(500)).len(), 100, "capped at 100");
    let last = f.get_coins(Some(120), Some(100));
    assert_eq!(last.len(), 10);
    assert_eq!(last[9].account_id, acc("c129.revive.testnet"));
}

#[test]
fn costs_cover_the_code_storage() {
    let f = factory();
    let per_byte = env::storage_byte_cost().as_yoctonear();
    assert!(f.get_launch_cost().0 >= (wasm().len() as u128) * per_byte);
    assert!(f.get_connect_cost().0 > (wasm().len() as u128) * per_byte, "connect also pays the token registration");
}
