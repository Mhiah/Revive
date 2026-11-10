//! The attacks a reviewer found against the first guard, replayed against the fixed one.
use super::*;

/// The keeper pulses every 3 days for `days` days, so the guard stays alive.
fn live_for(g: &mut Guard, w: &mut W, days: u64) {
    for _ in 0..days / 3 {
        w.t += 3 * DAY;
        w.at("dev.testnet", 0);
        g.pulse();
    }
}

fn connect_with(w: &W, decimals: u8) -> Guard {
    w.at("dev.testnet", 0);
    near_sdk::mock::with_mocked_blockchain(|b| {
        let _ = b.take_storage();
    });
    w.at("dev.testnet", 3 * NEAR);
    Guard::new(ConnectConfig {
        token_id: acc("meme.cooking.testnet"),
        name: "Meme".into(),
        symbol: "MEME".into(),
        decimals,
        links: None,
        pulse_interval_secs: 3 * DAY,
        vote_duration_secs: DAY,
        quorum_bps: 1_000,
        vest_duration_secs: 100 * DAY,
        spend_cap: U128(NEAR),
        spend_period_secs: 7 * DAY,
        comeback_penalty: U128(2 * NEAR),
        dao_suffix: ".sputnikv2.testnet".into(),
        creator: None,
    })
}

#[test]
fn a_late_lock_vests_from_when_it_arrives() {
    // Before: lock 1 token, keep pulsing past the vesting period, lock 100M, claim it all at once.
    let mut w = W { t: START };
    let mut g = connect(&w);
    deliver(&mut g, &w, "dev.testnet", 1, "lock");
    for _ in 0..34 {
        w.t += 3 * DAY;
        w.at("dev.testnet", 0);
        g.pulse();
    }
    deliver(&mut g, &w, "dev.testnet", 100_000_000, "lock");
    w.at("dev.testnet", 0);
    let _ = g.claim_vested();
    let s = g.get_status();
    assert!(s.dev_claimed.0 <= T, "only the 1 token locked 100+ days ago had vested, got {}", s.dev_claimed.0);
    assert!(s.dev_locked.0 >= 100_000_000 * T - T);
    // Half way through a fresh period, about half of the new bag has vested.
    live_for(&mut g, &mut w, 51);
    let _ = g.claim_vested();
    let claimed = g.get_status().dev_claimed.0;
    assert!(claimed > 50_000_000 * T && claimed < 52_000_000 * T, "claimed {}", claimed / T);
}

#[test]
fn whats_vested_stays_vested_when_more_is_locked() {
    let mut w = W { t: START };
    let mut g = connect(&w);
    deliver(&mut g, &w, "dev.testnet", 100_000_000, "lock");
    live_for(&mut g, &mut w, 51); // 51 of 100 days
    deliver(&mut g, &w, "dev.testnet", 100_000_000, "lock"); // top-up halfway through
    w.at("dev.testnet", 0);
    let _ = g.claim_vested();
    let claimed = g.get_status().dev_claimed.0;
    assert_eq!(claimed, 51_000_000 * T, "the 51M already vested can still be claimed");
}

#[test]
fn a_24_decimal_token_with_a_huge_supply_fits() {
    // Before: a 2e29 stake (200k tokens at 24 decimals) overflowed. Now a trillion tokens fit.
    let mut w = W { t: START };
    let mut g = connect_with(&w, 24);
    let big = 1_000_000_000_000u128 * 10u128.pow(24); // 1e36 base units
    for who in ["alice.testnet", "bob.testnet"] {
        register(&mut g, &w, who);
        w.at("meme.cooking.testnet", 0);
        let _ = g.ft_on_transfer(acc(who), U128(big / 4), "stake".into());
    }
    w.t += 3 * DAY + 1;
    w.at("alice.testnet", 0);
    let tk = g.t_start(&w, acc("x.sputnikv2.testnet"));
    assert!(tk.total_eligible.0 > 0);
    let alice = g.get_vote_weight(acc("alice.testnet")).0;
    assert!(tk.total_eligible.0.abs_diff(2 * alice) <= 2, "both stakes count equally");
}

#[test]
fn stakes_past_the_cap_are_refused_cleanly() {
    let w = W { t: START };
    let mut g = connect_with(&w, 12); // one weight unit = one base unit
    register(&mut g, &w, "alice.testnet");
    w.at("meme.cooking.testnet", 0);
    let err = panics(|| {
        let _ = g.ft_on_transfer(acc("alice.testnet"), U128(MAX_WEIGHT_UNITS + 1), "stake".into());
    })
    .expect("refused");
    assert!(err.contains("amount too large"), "{err}");
}

#[test]
fn a_failed_unstake_is_always_put_back() {
    // Before: if others staked while the unstake was in flight, putting it back overflowed
    // and the tokens sat in the guard credited to nobody.
    let w = W { t: START };
    let mut g = connect_with(&w, 12);
    let half = MAX_WEIGHT_UNITS / 2;
    for who in ["alice.testnet", "bob.testnet", "carol.testnet"] {
        register(&mut g, &w, who);
    }
    w.at("meme.cooking.testnet", 0);
    let _ = g.ft_on_transfer(acc("alice.testnet"), U128(half), "stake".into());
    let _ = g.ft_on_transfer(acc("bob.testnet"), U128(half), "stake".into());
    w.at("alice.testnet", 0);
    let since = g.stakers.get(&acc("alice.testnet")).unwrap().since;
    let _ = g.unstake(U128(half));
    w.at("meme.cooking.testnet", 0);
    let _ = g.ft_on_transfer(acc("carol.testnet"), U128(half), "stake".into()); // fills the room
    w.at("meme-guard.revive.testnet", 0);
    assert!(!g.on_unstake(acc("alice.testnet"), U128(half), since, Err(PromiseError::Failed)));
    assert_eq!(g.get_stake(acc("alice.testnet")).0, half, "alice's stake is back");
    assert_eq!(g.get_status().staked.0, 3 * half);
}

#[test]
fn staking_needs_a_paid_registration() {
    // Before: anyone could create staker entries for free and eat the guard's storage.
    let w = W { t: START };
    let mut g = connect(&w);
    w.at("meme.cooking.testnet", 0);
    let err = panics(|| {
        let _ = g.ft_on_transfer(acc("alice.testnet"), U128(T), "stake".into());
    })
    .expect("refused");
    assert!(err.contains("register first"), "{err}");
    w.at("alice.testnet", STAKER_STORAGE - 1);
    assert!(panics(|| {
        let _ = g.storage_deposit(None, None);
    })
    .unwrap()
    .contains("at least 0.002 NEAR"));
    w.at("alice.testnet", STAKER_STORAGE);
    let _ = g.storage_deposit(None, None);
    assert!(g.storage_balance_of(acc("alice.testnet")).is_some());
    w.at("meme.cooking.testnet", 0);
    let _ = g.ft_on_transfer(acc("alice.testnet"), U128(T), "stake".into());
    assert_eq!(g.get_stake(acc("alice.testnet")).0, T);
}

#[test]
fn unsafe_connect_settings_are_refused() {
    let w = W { t: START };
    w.at("dev.testnet", 0);
    let base = || ConnectConfig {
        token_id: acc("meme.cooking.testnet"),
        name: "Meme".into(),
        symbol: "MEME".into(),
        decimals: 18,
        links: None,
        pulse_interval_secs: 3 * DAY,
        vote_duration_secs: DAY,
        quorum_bps: 1_000,
        vest_duration_secs: 100 * DAY,
        spend_cap: U128(NEAR),
        spend_period_secs: 7 * DAY,
        comeback_penalty: U128(2 * NEAR),
        dao_suffix: ".sputnikv2.testnet".into(),
        creator: None,
    };
    let mut c = base();
    c.vest_duration_secs = 0;
    assert!(panics(|| {
        Guard::new(c);
    })
    .unwrap()
    .contains("vest over at least 4 heartbeats"));
    let mut c = base();
    c.comeback_penalty = U128(0);
    assert!(panics(|| {
        Guard::new(c);
    })
    .unwrap()
    .contains("comeback penalty"));
    let mut c = base();
    c.pulse_interval_secs = MAX_PERIOD_SECS + 1;
    c.vest_duration_secs = 200 * DAY;
    assert!(panics(|| {
        Guard::new(c);
    })
    .unwrap()
    .contains("30 days"));
}

#[test]
fn opening_a_takeover_needs_the_bond() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    w.at("alice.testnet", TAKEOVER_BOND - 1);
    assert!(panics(|| {
        g.start_takeover(acc("x.sputnikv2.testnet"));
    })
    .unwrap()
    .contains("attach at least"));
}

#[test]
fn a_failed_takeovers_bond_stays_with_the_coin() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    let before = g.get_status().treasury.0;
    w.t += 3 * DAY + 1;
    w.at("carol.testnet", 0);
    g.t_start(&w, acc("x.sputnikv2.testnet"));
    g.t_vote(&w, false);
    w.t += DAY;
    w.at("carol.testnet", 0);
    assert!(!g.t_finalize(&w));
    assert_eq!(g.get_status().treasury.0, before + TAKEOVER_BOND);
}
