use super::*;
use crate::token::STORAGE_COST;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;

const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const TOKEN: u128 = 1_000_000_000_000_000_000;
const DAY: u64 = 86_400;
const START: u64 = 1_800_000_000;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

struct World {
    t: u64,
    who: std::cell::RefCell<String>,
}

impl World {
    fn new() -> Self {
        World { t: START, who: std::cell::RefCell::new("dev.testnet".into()) }
    }
    fn at(&self, who: &str, deposit: u128) {
        *self.who.borrow_mut() = who.to_string();
        let mut b = VMContextBuilder::new();
        b.current_account_id(acc("revive.testnet"))
            .predecessor_account_id(acc(who))
            .signer_account_id(acc(who))
            .block_timestamp(self.t * 1_000_000_000)
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .account_balance(NearToken::from_yoctonear(100 * NEAR));
        testing_env!(b.build());
    }
    /// Move the clock; the last caller stays the caller.
    fn wait(&mut self, secs: u64) {
        self.t += secs;
        let who = self.who.borrow().clone();
        self.at(&who, 0);
    }
}

fn launch(w: &World) -> Revive {
    w.at("dev.testnet", 5 * NEAR);
    Revive::new(LaunchConfig {
        name: "Revive Cat".into(),
        symbol: "NINE".into(),
        icon: None,
        links: Some("x.com/ninecat".into()),
        total_supply: U128(1_000_000_000 * TOKEN),
        dev_allocation: U128(100_000_000 * TOKEN),
        distributor: acc("dev.testnet"),
        pulse_interval_secs: 3 * DAY,
        vote_duration_secs: DAY,
        quorum_bps: 2_000,
        vest_duration_secs: 100 * DAY,
        spend_cap: U128(NEAR),
        spend_period_secs: 7 * DAY,
        comeback_penalty: U128(2 * NEAR),
        dao_suffix: ".sputnikv2.testnet".into(),
    })
}

fn register(c: &mut Revive, w: &World, who: &str) {
    w.at(who, STORAGE_COST);
    c.storage_deposit(None, None);
}

fn send(c: &mut Revive, w: &World, from: &str, to: &str, tokens: u128) {
    w.at(from, 1);
    c.ft_transfer(acc(to), U128(tokens * TOKEN), None);
}

/// Dev hands out tokens to three early holders, then goes quiet.
fn early_holders(c: &mut Revive, w: &World) {
    for h in ["alice.testnet", "bob.testnet", "carol.testnet"] {
        register(c, w, h);
    }
    send(c, w, "dev.testnet", "alice.testnet", 300_000_000);
    send(c, w, "dev.testnet", "bob.testnet", 200_000_000);
    send(c, w, "dev.testnet", "carol.testnet", 100_000_000);
}

#[test]
fn launch_state() {
    let w = World::new();
    let c = launch(&w);
    let s = c.get_status();
    assert_eq!(s.status, "alive");
    assert_eq!(s.treasury.0, 5 * NEAR);
    assert_eq!(c.ft_total_supply().0, 1_000_000_000 * TOKEN);
    assert_eq!(c.ft_balance_of(acc("dev.testnet")).0, 900_000_000 * TOKEN);
    assert_eq!(c.ft_balance_of(acc("revive.testnet")).0, 100_000_000 * TOKEN, "locked creator tokens show as the contract's balance");
    assert_eq!(c.ft_metadata().symbol, "NINE");
    assert_eq!(c.get_lore_count(), 1);
}

#[test]
#[should_panic(expected = "creator can keep at most 10% of supply")]
fn creator_allocation_capped() {
    let w = World::new();
    w.at("dev.testnet", 0);
    Revive::new(LaunchConfig {
        name: "Greedy".into(),
        symbol: "GRDY".into(),
        icon: None,
        links: None,
        total_supply: U128(1_000 * TOKEN),
        dev_allocation: U128(101 * TOKEN),
        distributor: acc("dev.testnet"),
        pulse_interval_secs: DAY,
        vote_duration_secs: DAY,
        quorum_bps: 2_000,
        vest_duration_secs: 0,
        spend_cap: U128(NEAR),
        spend_period_secs: DAY,
        comeback_penalty: U128(NEAR),
        dao_suffix: ".sputnikv2.testnet".into(),
    });
}

#[test]
fn pulse_keeps_it_alive() {
    let mut w = World::new();
    let mut c = launch(&w);
    for _ in 0..5 {
        w.wait(2 * DAY);
        w.at("dev.testnet", 0);
        c.pulse();
        assert_eq!(c.get_status().status, "alive");
    }
    w.wait(3 * DAY + 1);
    w.at("dev.testnet", 0);
    assert_eq!(c.get_status().status, "flatlined");
}

#[test]
#[should_panic(expected = "only the keeper has a pulse")]
fn only_keeper_pulses() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("alice.testnet", 0);
    c.pulse();
}

#[test]
#[should_panic(expected = "the coin still has a pulse")]
fn no_takeover_while_alive() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(DAY);
    w.at("alice.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
}

#[test]
fn full_revival() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);

    // Dev pulses every 2 days for 10 days, claims vesting, then disappears.
    for _ in 0..5 {
        w.wait(2 * DAY);
        w.at("dev.testnet", 0);
        c.pulse();
    }
    let claimed = c.claim_vested().0;
    assert_eq!(claimed, 10_000_000 * TOKEN);

    w.wait(3 * DAY + 1);
    w.at("alice.testnet", 0);
    assert_eq!(c.get_status().status, "flatlined");
    let tk = c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    assert!(tk.total_eligible.0 > 0);

    for v in ["alice.testnet", "bob.testnet"] {
        w.at(v, 0);
        c.vote(true);
    }
    w.at("carol.testnet", 0);
    c.vote(false);

    w.wait(DAY);
    w.at("anyone.testnet", 0);
    assert_eq!(c.get_status().status, "vote_closed");
    assert!(c.finalize_takeover());

    let s = c.get_status();
    assert_eq!(s.status, "alive");
    assert_eq!(s.keeper, acc("ninecat.sputnikv2.testnet"));
    assert_eq!(s.lives, 1);
    assert_eq!(s.dev_burned.0, 90_000_000 * TOKEN, "unvested creator tokens are burned");
    assert_eq!(c.ft_total_supply().0, 910_000_000 * TOKEN);
    assert_eq!(c.ft_balance_of(acc("revive.testnet")).0, 0);

    // The DAO is the keeper now: it pulses and spends.
    w.wait(DAY);
    w.at("ninecat.sputnikv2.testnet", 0);
    c.pulse();
    let _ = c.spend(acc("artist.testnet"), U128(NEAR / 2), "new banner".into());
    assert_eq!(c.get_status().treasury.0, 5 * NEAR - NEAR / 2);
    assert!(c.get_lore(None, None).iter().any(|l| l.kind == "revived"));
}

#[test]
#[should_panic(expected = "only the living creator can claim")]
fn creator_cannot_claim_after_revival() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("alice.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    c.vote(true);
    w.wait(DAY);
    c.finalize_takeover();
    w.at("dev.testnet", 0);
    c.claim_vested();
}

/// The BonkDAO-style attack: a whale buys after the flatline to swing the vote.
#[test]
fn tokens_bought_after_flatline_have_no_weight() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "whale.testnet");

    w.wait(3 * DAY + 1);
    // Whale buys 250M from the pool after the coin is already dead.
    send(&mut c, &w, "dev.testnet", "whale.testnet", 250_000_000);
    assert_eq!(c.get_vote_weight(acc("whale.testnet")).0, 0);
    assert!(c.get_vote_weight(acc("carol.testnet")).0 > 0);

    w.at("carol.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        w.at("whale.testnet", 0);
        c.vote(false);
    }));
    assert!(result.is_err(), "the whale cannot vote");
}

#[test]
fn moving_tokens_after_voting_does_not_double_count() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "alice2.testnet");
    w.wait(3 * DAY + 1);
    w.at("alice.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    c.vote(true);
    send(&mut c, &w, "alice.testnet", "alice2.testnet", 300_000_000);
    assert_eq!(c.get_vote_weight(acc("alice2.testnet")).0, 0);

    // Moving tokens into an older wallet that already holds dilutes it, not boosts it.
    let before = c.get_vote_weight(acc("bob.testnet")).0;
    w.at("alice2.testnet", 1);
    c.ft_transfer(acc("bob.testnet"), U128(300_000_000 * TOKEN), None);
    assert!(c.get_vote_weight(acc("bob.testnet")).0 < before);
}

#[test]
fn comeback_cancels_the_vote_and_pays_holders_treasury() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("alice.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    c.vote(true);

    w.wait(DAY / 2);
    w.at("dev.testnet", 2 * NEAR);
    c.pulse();
    let s = c.get_status();
    assert_eq!(s.status, "alive");
    assert!(s.takeover.is_none());
    assert_eq!(s.treasury.0, 7 * NEAR);
    assert!(c.get_lore(None, None).iter().any(|l| l.kind == "comeback"));
}

#[test]
#[should_panic(expected = "late pulse: attach the comeback penalty")]
fn late_pulse_needs_penalty() {
    let mut w = World::new();
    let mut c = launch(&w);
    w.wait(3 * DAY + 1);
    w.at("dev.testnet", NEAR);
    c.pulse();
}

#[test]
#[should_panic(expected = "the vote has closed; call finalize_takeover first")]
fn no_comeback_after_the_vote_closes() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("alice.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    c.vote(true);
    w.wait(DAY);
    w.at("dev.testnet", 2 * NEAR);
    c.pulse();
}

#[test]
fn no_vote_wins_and_coin_stays_flatlined() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("carol.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    c.vote(true);
    w.at("alice.testnet", 0);
    c.vote(false);
    w.wait(DAY);
    assert!(!c.finalize_takeover());
    assert_eq!(c.get_status().status, "flatlined");
    assert_eq!(c.get_status().keeper, acc("dev.testnet"));
    // A new takeover can be tried.
    w.at("bob.testnet", 0);
    assert_eq!(c.start_takeover(acc("other.sputnikv2.testnet")).id, 2);
}

#[test]
fn quorum_is_required() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    // Carol alone holds 100M of 600M eligible (~16.7%), under the 20% quorum.
    w.at("carol.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    c.vote(true);
    w.wait(DAY);
    assert!(!c.finalize_takeover());
    assert!(c.get_lore(None, None).iter().any(|l| l.note.contains("quorum not reached")));
}

#[test]
#[should_panic(expected = "the steward must be a Sputnik DAO")]
fn steward_must_be_a_dao() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("alice.testnet", 0);
    c.start_takeover(acc("alice.testnet"));
}

#[test]
#[should_panic(expected = "over the spending cap for this period")]
fn spending_is_capped() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    let _ = c.spend(acc("me.testnet"), U128(NEAR / 2), "marketing".into());
    let _ = c.spend(acc("me.testnet"), U128(NEAR), "more marketing".into());
}

#[test]
fn nine_lives_second_death_and_revival() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    for (round, dao) in ["first.sputnikv2.testnet", "second.sputnikv2.testnet"].iter().enumerate() {
        w.wait(3 * DAY + 1);
        w.at("alice.testnet", 0);
        c.start_takeover(acc(dao));
        c.vote(true);
        w.at("bob.testnet", 0);
        c.vote(true);
        w.wait(DAY);
        assert!(c.finalize_takeover());
        assert_eq!(c.get_status().lives, round as u32 + 1);
        assert_eq!(c.get_status().keeper, acc(dao));
    }
}

#[test]
fn dex_refund_keeps_hold_time() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "pool.testnet");
    w.wait(2 * DAY);
    let since_before = c.holders.get(&acc("alice.testnet")).unwrap().since;
    w.at("alice.testnet", 1);
    c.internal_transfer(&acc("alice.testnet"), &acc("pool.testnet"), 1_000 * TOKEN, false);
    c.internal_transfer(&acc("pool.testnet"), &acc("alice.testnet"), 1_000 * TOKEN, true);
    assert_eq!(c.holders.get(&acc("alice.testnet")).unwrap().since, since_before);
}

#[test]
fn running_total_matches_sum_of_holders() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "dave.testnet");
    w.wait(DAY);
    send(&mut c, &w, "alice.testnet", "dave.testnet", 50_000_000);
    w.wait(DAY);
    send(&mut c, &w, "bob.testnet", "alice.testnet", 20_000_000);
    let f = c.flatline_at();
    let by_hand: u128 = ["alice.testnet", "bob.testnet", "carol.testnet", "dave.testnet"]
        .iter()
        .map(|a| c.weight_at(&acc(a), f))
        .sum();
    let fast = c.eligible_total(f);
    let diff = by_hand.abs_diff(fast);
    assert!(diff <= 4, "running total {fast} vs sum {by_hand}");
}
