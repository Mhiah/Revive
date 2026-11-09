//! Attacks on the vote: buying in late, splitting wallets, hopping wallets, fake DAOs,
//! and the creator or keeper trying to steer their own takeover.
use super::*;

const HOLDERS: [&str; 5] = ["alice.testnet", "bob.testnet", "carol.testnet", "whale.testnet", "dev.testnet"];

#[test]
fn eligible_total_ignores_tokens_bought_after_the_death() {
    // Regression: before the fix, this buy pushed the eligible total to 0, so one tiny
    // "yes" met any quorum.
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "whale.testnet");
    w.wait(3 * DAY + 1);
    let f = c.flatline_at();
    let before = c.eligible_total(f);
    w.wait(20 * DAY);
    send(&mut c, &w, "dev.testnet", "whale.testnet", 290_000_000);
    assert_eq!(c.eligible_total(f), before, "late tokens neither add nor subtract");
    assert!(c.eligible_total(f).abs_diff(honest_total(&c, &HOLDERS, f)) <= 4);
}

#[test]
fn quorum_cannot_be_dodged_by_late_buying() {
    // Carol alone holds about 17% of the pre-death weight, under the 20% quorum.
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "whale.testnet");
    w.wait(3 * DAY + 1);
    w.wait(10 * DAY);
    send(&mut c, &w, "dev.testnet", "whale.testnet", 290_000_000);
    w.at("carol.testnet", 0);
    c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    c.vote(true);
    w.wait(DAY);
    w.at("carol.testnet", 0);
    assert!(!c.finalize_takeover(), "a 17% yes must not pass a 20% quorum");
    assert!(c.get_lore(None, None).last().unwrap().note.contains("quorum not reached"));
}

#[test]
fn eligible_total_is_exact_when_old_holders_trade_after_the_death() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "whale.testnet");
    w.wait(4 * DAY);
    let f = c.flatline_at();
    send(&mut c, &w, "alice.testnet", "bob.testnet", 120_000_000);
    w.wait(DAY);
    send(&mut c, &w, "bob.testnet", "whale.testnet", 50_000_000);
    send(&mut c, &w, "carol.testnet", "alice.testnet", 30_000_000);
    let honest = honest_total(&c, &HOLDERS, f);
    assert!(c.eligible_total(f).abs_diff(honest) <= 6, "{} vs {}", c.eligible_total(f), honest);
}

#[test]
fn splitting_a_wallet_before_the_death_adds_no_weight() {
    let run = |split: bool| {
        let mut w = World::new();
        let mut c = launch(&w);
        early_holders(&mut c, &w);
        for s in ["a1.testnet", "a2.testnet", "a3.testnet"] {
            register(&mut c, &w, s);
        }
        w.wait(DAY);
        if split {
            for s in ["a1.testnet", "a2.testnet", "a3.testnet"] {
                send(&mut c, &w, "alice.testnet", s, 100_000_000);
            }
        }
        w.wait(2 * DAY + 1);
        let f = c.flatline_at();
        ["alice.testnet", "a1.testnet", "a2.testnet", "a3.testnet"].iter().map(|a| c.weight_at(&acc(a), f)).sum::<u128>()
    };
    let whole = run(false);
    let split = run(true);
    assert!(split < whole, "split {split} must weigh less than whole {whole}: moved tokens restart their clock");
}

#[test]
fn hopping_to_a_new_wallet_after_the_death_loses_the_vote() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "fresh.testnet");
    w.wait(3 * DAY + 1);
    let f = c.flatline_at();
    send(&mut c, &w, "bob.testnet", "fresh.testnet", 200_000_000);
    assert_eq!(c.weight_at(&acc("bob.testnet"), f), 0, "bob holds nothing now");
    assert_eq!(c.weight_at(&acc("fresh.testnet"), f), 0, "fresh tokens arrived after the death");
}

#[test]
fn weight_is_frozen_at_the_flatline() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    let first = c.get_vote_weight(acc("alice.testnet")).0;
    w.wait(30 * DAY);
    assert_eq!(c.get_vote_weight(acc("alice.testnet")).0, first, "waiting after the death earns nothing");
}

#[test]
#[should_panic(expected = "already voted")]
fn nobody_votes_twice() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    c.vote(true);
    c.vote(true);
}

#[test]
#[should_panic(expected = "the keeper and creator cannot vote")]
fn the_creator_cannot_vote_on_their_own_takeover() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    w.at("dev.testnet", 0);
    c.vote(false);
}

#[test]
#[should_panic(expected = "the keeper and creator cannot start a takeover")]
fn the_creator_cannot_start_a_takeover() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("dev.testnet", 0);
    c.start_takeover(acc("friendly.sputnikv2.testnet"));
}

#[test]
#[should_panic(expected = "only holders from before the flatline can start a takeover")]
fn a_late_buyer_cannot_start_a_takeover() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "whale.testnet");
    w.wait(3 * DAY + 1);
    send(&mut c, &w, "dev.testnet", "whale.testnet", 100_000_000);
    w.at("whale.testnet", 0);
    c.start_takeover(acc("whales.sputnikv2.testnet"));
}

#[test]
#[should_panic(expected = "a takeover vote is already open")]
fn only_one_takeover_at_a_time() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    w.at("bob.testnet", 0);
    c.start_takeover(acc("other.sputnikv2.testnet"));
}

fn nominate(steward: &str) {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("alice.testnet", 0);
    c.start_takeover(acc(steward));
}

#[test]
#[should_panic(expected = "the steward must be a Sputnik DAO")]
fn lookalike_dao_name_without_the_dot_is_refused() {
    nominate("fakesputnikv2.testnet");
}

#[test]
#[should_panic(expected = "the steward must be a Sputnik DAO")]
fn the_dao_factory_itself_is_refused() {
    nominate("sputnikv2.testnet");
}

#[test]
#[should_panic(expected = "the steward must be a Sputnik DAO")]
fn a_dao_name_buried_in_another_account_is_refused() {
    nominate("x.sputnikv2.testnet.attacker.testnet");
}

#[test]
fn a_real_dao_name_is_accepted() {
    nominate("community.sputnikv2.testnet");
}

#[test]
#[should_panic(expected = "not enough balance")]
fn the_creator_cannot_move_locked_tokens() {
    let w = World::new();
    let mut c = launch(&w);
    register(&mut c, &w, "alice.testnet");
    // The dev holds 900M liquid; the 100M locked share is not theirs to send.
    send(&mut c, &w, "dev.testnet", "alice.testnet", 900_000_001);
}

#[test]
fn the_creators_own_tokens_are_left_out_of_the_eligible_total() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    let f = c.flatline_at();
    let holders_only = honest_total(&c, &["alice.testnet", "bob.testnet", "carol.testnet"], f);
    assert!(c.eligible_total(f).abs_diff(holders_only) <= 4, "the dev's 300M liquid tokens must not count");
}

#[test]
fn selling_before_voting_lowers_your_weight() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "buyer.testnet");
    dead_and_open(&mut c, &mut w);
    let full = c.get_vote_weight(acc("bob.testnet")).0;
    send(&mut c, &w, "bob.testnet", "buyer.testnet", 100_000_000);
    let half = c.get_vote_weight(acc("bob.testnet")).0;
    assert!(half * 2 <= full + 1, "selling half halves the vote");
}

#[test]
fn a_late_whale_cannot_flip_a_passing_vote() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    register(&mut c, &w, "whale.testnet");
    dead_and_open(&mut c, &mut w);
    c.vote(true);
    send(&mut c, &w, "dev.testnet", "whale.testnet", 290_000_000);
    assert!(panics(|| {
        w.at("whale.testnet", 0);
        c.vote(false);
    })
    .unwrap()
    .contains("no voting weight"));
    w.wait(DAY);
    w.at("bob.testnet", 0);
    assert!(c.finalize_takeover());
}

