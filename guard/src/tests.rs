use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;

const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const T: u128 = 1_000_000_000_000_000_000; // 1 token (18 decimals)
const DAY: u64 = 86_400;
const START: u64 = 1_800_000_000;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

struct W {
    t: u64,
}

impl W {
    fn at(&self, who: &str, deposit: u128) {
        let mut b = VMContextBuilder::new();
        b.current_account_id(acc("meme-guard.revive.testnet"))
            .predecessor_account_id(acc(who))
            .block_timestamp(self.t * 1_000_000_000)
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .account_balance(NearToken::from_yoctonear(50 * NEAR));
        testing_env!(b.build());
    }
}

fn connect(w: &W) -> Guard {
    // Fresh storage for every guard, so tests that build more than one don't share state.
    w.at("dev.testnet", 0);
    near_sdk::mock::with_mocked_blockchain(|b| {
        let _ = b.take_storage();
    });
    w.at("dev.testnet", 3 * NEAR);
    Guard::new(ConnectConfig {
        token_id: acc("meme.cooking.testnet"),
        name: "Meme".into(),
        symbol: "MEME".into(),
        decimals: 18,
        links: None,
        pulse_interval_secs: 3 * DAY,
        vote_duration_secs: DAY,
        quorum_bps: 2_000,
        vest_duration_secs: 100 * DAY,
        spend_cap: U128(NEAR),
        spend_period_secs: 7 * DAY,
        comeback_penalty: U128(2 * NEAR),
        dao_suffix: ".sputnikv2.testnet".into(),
        creator: None,
    })
}

/// The token contract calls ft_on_transfer on behalf of `sender`.
fn deliver(g: &mut Guard, w: &W, sender: &str, tokens: u128, msg: &str) {
    w.at("meme.cooking.testnet", 0);
    let _ = g.ft_on_transfer(acc(sender), U128(tokens * T), msg.into());
}

fn setup(w: &W) -> Guard {
    let mut g = connect(w);
    deliver(&mut g, w, "dev.testnet", 100_000_000, "lock");
    deliver(&mut g, w, "alice.testnet", 300_000_000, "stake");
    deliver(&mut g, w, "bob.testnet", 200_000_000, "stake");
    deliver(&mut g, w, "carol.testnet", 100_000_000, "stake");
    g
}

#[test]
fn connect_and_lock() {
    let w = W { t: START };
    let g = setup(&w);
    let s = g.get_status();
    assert_eq!(s.kind, "guard");
    assert_eq!(s.status, "alive");
    assert_eq!(s.token_id, acc("meme.cooking.testnet"));
    assert_eq!(s.dev_locked.0, 100_000_000 * T);
    assert_eq!(s.staked.0, 600_000_000 * T);
    assert_eq!(s.stakers, 3);
    assert_eq!(s.treasury.0, 3 * NEAR);
}

#[test]
#[should_panic(expected = "only the guarded token can be sent here")]
fn other_tokens_are_refused() {
    let w = W { t: START };
    let mut g = connect(&w);
    w.at("scam.testnet", 0);
    let _ = g.ft_on_transfer(acc("alice.testnet"), U128(T), "stake".into());
}

#[test]
#[should_panic(expected = "only the living creator locks a bag")]
fn only_creator_locks() {
    let w = W { t: START };
    let mut g = connect(&w);
    deliver(&mut g, &w, "alice.testnet", 1, "lock");
}

#[test]
#[should_panic(expected = "the creator and keeper cannot stake for votes")]
fn creator_cannot_stake() {
    let w = W { t: START };
    let mut g = connect(&w);
    deliver(&mut g, &w, "dev.testnet", 1, "stake");
}

#[test]
#[should_panic(expected = "msg must be")]
fn unknown_msg_refused() {
    let w = W { t: START };
    let mut g = connect(&w);
    deliver(&mut g, &w, "alice.testnet", 1, "gift");
}

#[test]
fn full_takeover_seals_the_bag() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 2 * DAY;
    w.at("dev.testnet", 0);
    g.pulse();
    w.t += 8 * DAY; // 10 days in, 3-day pulse missed
    w.at("dev.testnet", 0);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.claim_vested()));
    assert_eq!(g.get_status().status, "flatlined");

    w.at("alice.testnet", 0);
    g.start_takeover(acc("meme.sputnikv2.testnet"));
    g.vote(true);
    w.at("bob.testnet", 0);
    g.vote(true);
    w.at("carol.testnet", 0);
    g.vote(false);
    w.t += DAY;
    w.at("anyone.testnet", 0);
    assert!(g.finalize_takeover());
    let s = g.get_status();
    assert_eq!(s.keeper, acc("meme.sputnikv2.testnet"));
    assert_eq!(s.lives, 1);
    assert_eq!(s.dev_burned.0, 100_000_000 * T, "the whole unclaimed bag is sealed");
    assert_eq!(s.dev_locked.0, 0);
    // The DAO keeps it alive and controls the treasury.
    w.at("meme.sputnikv2.testnet", 0);
    g.pulse();
    let _ = g.spend(acc("artist.testnet"), U128(NEAR / 2), "memes".into());
    assert_eq!(g.get_status().treasury.0, 3 * NEAR - NEAR / 2);
}

#[test]
fn staking_after_the_death_buys_no_vote() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    deliver(&mut g, &w, "whale.testnet", 900_000_000, "stake");
    assert_eq!(g.get_vote_weight(acc("whale.testnet")).0, 0);
    w.at("whale.testnet", 0);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut g2 = std::mem::replace(&mut g, setup(&W { t: START }));
        g2.start_takeover(acc("x.sputnikv2.testnet"));
    }));
    assert!(r.is_err(), "whale cannot open a takeover");
}

#[test]
fn unstake_then_restake_after_death_has_no_weight() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    w.at("alice.testnet", 0);
    let before = g.get_vote_weight(acc("alice.testnet")).0;
    assert!(before > 0);
    let _ = g.unstake(U128(300_000_000 * T));
    assert_eq!(g.get_stake(acc("alice.testnet")).0, 0);
    deliver(&mut g, &w, "alice.testnet", 300_000_000, "stake");
    assert_eq!(g.get_vote_weight(acc("alice.testnet")).0, 0, "re-staked tokens are new tokens");
}

#[test]
fn failed_unstake_restores_the_stake() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += DAY;
    w.at("bob.testnet", 0);
    let since = g.stakers.get(&acc("bob.testnet")).unwrap().since;
    let _ = g.unstake(U128(50_000_000 * T));
    w.at("meme-guard.revive.testnet", 0);
    g.on_unstake(acc("bob.testnet"), U128(50_000_000 * T), since, Err(PromiseError::Failed));
    assert_eq!(g.get_stake(acc("bob.testnet")).0, 200_000_000 * T);
    assert_eq!(g.stakers.get(&acc("bob.testnet")).unwrap().since, since);
}

#[test]
fn comeback_mid_vote() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    w.at("alice.testnet", 0);
    g.start_takeover(acc("meme.sputnikv2.testnet"));
    w.at("dev.testnet", 2 * NEAR);
    g.pulse();
    let s = g.get_status();
    assert_eq!(s.status, "alive");
    assert!(s.takeover.is_none());
    assert_eq!(s.treasury.0, 5 * NEAR);
}

#[test]
fn vesting_claims_while_alive() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 10 * DAY;
    w.at("dev.testnet", 2 * NEAR);
    g.pulse(); // late, pays the comeback penalty
    let _ = g.claim_vested();
    assert_eq!(g.get_status().dev_claimed.0, 10_000_000 * T);
    w.at("meme-guard.revive.testnet", 0);
    g.on_claim(U128(10_000_000 * T), Err(PromiseError::Failed));
    assert_eq!(g.get_status().dev_claimed.0, 0, "a failed transfer is undone");
}

#[test]
fn quorum_and_no_votes() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    w.at("carol.testnet", 0);
    g.start_takeover(acc("meme.sputnikv2.testnet"));
    g.vote(true);
    w.t += DAY;
    w.at("anyone.testnet", 0);
    assert!(!g.finalize_takeover(), "carol alone is under the 20% quorum");
    assert_eq!(g.get_status().status, "flatlined");
}

#[test]
fn running_total_matches_stakers() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += DAY;
    deliver(&mut g, &w, "dave.testnet", 50_000_000, "stake");
    w.t += DAY;
    w.at("alice.testnet", 0);
    let _ = g.unstake(U128(100_000_000 * T));
    let f = g.flatline_at();
    let by_hand: u128 = ["alice.testnet", "bob.testnet", "carol.testnet", "dave.testnet"].iter().map(|a| g.weight_at(&acc(a), f)).sum();
    assert!(by_hand.abs_diff(g.eligible_total(f)) <= 4);
}

#[test]
#[should_panic(expected = "only the parent factory can name the creator")]
fn strangers_cannot_name_creator() {
    let w = W { t: START };
    w.at("mallory.testnet", 0);
    Guard::new(ConnectConfig {
        token_id: acc("meme.cooking.testnet"),
        name: "Meme".into(),
        symbol: "MEME".into(),
        decimals: 18,
        links: None,
        pulse_interval_secs: DAY,
        vote_duration_secs: DAY,
        quorum_bps: 2_000,
        vest_duration_secs: 0,
        spend_cap: U128(NEAR),
        spend_period_secs: DAY,
        comeback_penalty: U128(NEAR),
        dao_suffix: ".sputnikv2.testnet".into(),
        creator: Some(acc("vitalik.testnet")),
    });
}


// Grouped test files (see the table in the README).
mod attacks;
mod money_and_time;
mod invariants;

fn panics<F: FnOnce()>(f: F) -> Option<String> {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    r.err().map(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())
}

/// Everyone's real voting weight at `f`, staker by staker.
fn honest_total(g: &Guard, who: &[&str], f: u64) -> u128 {
    who.iter().map(|a| g.weight_at(&acc(a), f)).sum()
}

/// Dev goes silent; alice opens a takeover.
fn dead_and_open(g: &mut Guard, w: &mut W) {
    w.t += 3 * DAY + 1;
    w.at("alice.testnet", 0);
    g.start_takeover(acc("meme.sputnikv2.testnet"));
}
