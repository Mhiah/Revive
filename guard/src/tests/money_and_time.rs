//! Deadlines, the spending cap, vesting and the sealed bag on a connected coin.
use super::*;

#[test]
fn a_pulse_exactly_at_the_deadline_is_free() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY;
    w.at("dev.testnet", 0);
    g.pulse();
    assert_eq!(g.get_status().treasury.0, 3 * NEAR, "no penalty charged");
}

#[test]
#[should_panic(expected = "the vote has closed")]
fn a_vote_at_the_closing_second_is_refused() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    dead_and_open(&mut g, &mut w);
    w.t += DAY;
    w.at("bob.testnet", 0);
    g.t_vote(&w, true);
}

#[test]
#[should_panic(expected = "the vote is still open")]
fn finalizing_early_is_refused() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    dead_and_open(&mut g, &mut w);
    g.t_vote(&w, true);
    w.t += DAY - 1;
    w.at("bob.testnet", 0);
    g.t_finalize(&w);
}

#[test]
fn a_tie_is_a_no() {
    let mut w = W { t: START };
    let mut g = connect(&w);
    deliver(&mut g, &w, "p.testnet", 100, "stake");
    deliver(&mut g, &w, "q.testnet", 100, "stake");
    w.t += 3 * DAY + 1;
    w.at("p.testnet", 0);
    g.t_start(&w, acc("x.sputnikv2.testnet"));
    g.t_vote(&w, true);
    w.at("q.testnet", 0);
    g.t_vote(&w, false);
    w.t += DAY;
    w.at("q.testnet", 0);
    assert!(!g.t_finalize(&w));
}

#[test]
fn the_cap_holds_and_resets() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.at("dev.testnet", 0);
    let _ = g.spend(acc("artist.testnet"), U128(NEAR), "week one".into());
    assert!(panics(|| { let _ = g.spend(acc("artist.testnet"), U128(1), "more".into()); }).unwrap().contains("over the spending cap"));
    for _ in 0..4 {
        w.t += 2 * DAY;
        w.at("dev.testnet", 0);
        g.pulse();
    }
    let _ = g.spend(acc("artist.testnet"), U128(NEAR), "week two".into());
    assert_eq!(g.get_status().treasury.0, NEAR);
}

#[test]
#[should_panic(expected = "the coin has flatlined")]
fn a_dead_coin_cannot_spend() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    w.at("dev.testnet", 0);
    let _ = g.spend(acc("dev.testnet"), U128(1), "grab".into());
}

#[test]
#[should_panic(expected = "nothing vested yet")]
fn nothing_vests_at_connect() {
    let w = W { t: START };
    let mut g = setup(&w);
    w.at("dev.testnet", 0);
    let _ = g.claim_vested();
}

#[test]
fn the_sealed_bag_is_exactly_what_was_not_claimed() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    for _ in 0..20 {
        w.t += 2 * DAY;
        w.at("dev.testnet", 0);
        g.pulse();
    }
    w.at("dev.testnet", 0);
    let _ = g.claim_vested(); // 40 of 100 days
    assert_eq!(g.get_status().dev_claimed.0, 40_000_000 * T);
    dead_and_open(&mut g, &mut w);
    g.t_vote(&w, true);
    w.at("bob.testnet", 0);
    g.t_vote(&w, true);
    w.t += DAY;
    w.at("bob.testnet", 0);
    assert!(g.t_finalize(&w));
    let s = g.get_status();
    assert_eq!(s.dev_burned.0, 60_000_000 * T, "the unclaimed 60M is sealed");
    assert_eq!(s.dev_locked.0, 0);
}

#[test]
fn unstaking_is_always_open_even_mid_vote() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    dead_and_open(&mut g, &mut w);
    w.at("carol.testnet", 0);
    let _ = g.unstake(U128(100_000_000 * T));
    assert_eq!(g.get_stake(acc("carol.testnet")).0, 0);
}

#[test]
#[should_panic(expected = "not enough staked")]
fn you_cannot_unstake_more_than_you_staked() {
    let w = W { t: START };
    let mut g = setup(&w);
    w.at("carol.testnet", 0);
    let _ = g.unstake(U128(100_000_001 * T));
}
