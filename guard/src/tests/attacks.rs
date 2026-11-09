//! Attacks on a connected coin's vote: late stake, stake hopping, fake DAOs, and a DAO
//! that staked before it was elected.
use super::*;

const STAKERS: [&str; 5] = ["alice.testnet", "bob.testnet", "carol.testnet", "whale.testnet", "dao.sputnikv2.testnet"];

#[test]
fn late_stake_does_not_shrink_the_eligible_total() {
    // Regression: stake added after the death used to pull the total down, even to 0.
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    let f = g.flatline_at();
    let before = g.eligible_total(f);
    w.t += 20 * DAY;
    deliver(&mut g, &w, "whale.testnet", 900_000_000, "stake");
    assert_eq!(g.eligible_total(f), before);
}

#[test]
fn quorum_cannot_be_dodged_by_late_staking() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 13 * DAY;
    deliver(&mut g, &w, "whale.testnet", 900_000_000, "stake");
    w.at("carol.testnet", 0);
    g.start_takeover(acc("meme.sputnikv2.testnet"));
    g.vote(true);
    w.t += DAY;
    w.at("anyone.testnet", 0);
    assert!(!g.finalize_takeover(), "carol's ~17% must not pass a 20% quorum");
}

#[test]
fn eligible_total_is_exact_through_unstakes_and_restakes_after_the_death() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 4 * DAY;
    let f = g.flatline_at();
    w.at("alice.testnet", 0);
    let _ = g.unstake(U128(120_000_000 * T));
    deliver(&mut g, &w, "alice.testnet", 60_000_000, "stake");
    w.t += DAY;
    deliver(&mut g, &w, "whale.testnet", 500_000_000, "stake");
    w.at("bob.testnet", 0);
    let _ = g.unstake(U128(10_000_000 * T));
    assert!(g.eligible_total(f).abs_diff(honest_total(&g, &STAKERS, f)) <= 5);
}

#[test]
fn a_dao_that_staked_before_its_election_does_not_inflate_the_next_quorum() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    deliver(&mut g, &w, "dao.sputnikv2.testnet", 400_000_000, "stake");
    w.t += 3 * DAY + 1;
    w.at("alice.testnet", 0);
    g.start_takeover(acc("dao.sputnikv2.testnet"));
    g.vote(true);
    w.at("bob.testnet", 0);
    g.vote(true);
    w.t += DAY;
    w.at("bob.testnet", 0);
    assert!(g.finalize_takeover());
    // The DAO is keeper now and can't vote, so its stake must not count toward the next quorum.
    let f = g.flatline_at();
    assert_eq!(g.weight_at(&acc("dao.sputnikv2.testnet"), f), 0);
    assert!(g.eligible_total(f).abs_diff(honest_total(&g, &STAKERS, f)) <= 5,
        "eligible {} vs honest {}", g.eligible_total(f), honest_total(&g, &STAKERS, f));
}

#[test]
#[should_panic(expected = "already voted")]
fn nobody_votes_twice() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    dead_and_open(&mut g, &mut w);
    g.vote(true);
    g.vote(true);
}

#[test]
#[should_panic(expected = "the steward must be a Sputnik DAO")]
fn lookalike_dao_names_are_refused() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    w.at("alice.testnet", 0);
    g.start_takeover(acc("fakesputnikv2.testnet"));
}

#[test]
#[should_panic(expected = "a takeover vote is already open")]
fn one_takeover_at_a_time() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    dead_and_open(&mut g, &mut w);
    w.at("bob.testnet", 0);
    g.start_takeover(acc("other.sputnikv2.testnet"));
}

#[test]
fn hopping_stake_after_the_death_votes_nothing() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    w.t += 3 * DAY + 1;
    w.at("bob.testnet", 0);
    let _ = g.unstake(U128(200_000_000 * T));
    deliver(&mut g, &w, "bob2.testnet", 200_000_000, "stake");
    assert_eq!(g.get_vote_weight(acc("bob.testnet")).0, 0);
    assert_eq!(g.get_vote_weight(acc("bob2.testnet")).0, 0);
}

#[test]
#[should_panic(expected = "only the guarded token can be sent here")]
fn a_fake_token_contract_cannot_credit_stake() {
    let w = W { t: START };
    let mut g = setup(&w);
    w.at("evil-token.testnet", 0);
    let _ = g.ft_on_transfer(acc("mallory.testnet"), U128(1_000 * T), "stake".into());
}

#[test]
#[should_panic(expected = "the creator and keeper cannot stake for votes")]
fn the_elected_dao_cannot_stake_after_winning() {
    let mut w = W { t: START };
    let mut g = setup(&w);
    dead_and_open(&mut g, &mut w);
    g.vote(true);
    w.at("bob.testnet", 0);
    g.vote(true);
    w.t += DAY;
    w.at("bob.testnet", 0);
    assert!(g.finalize_takeover());
    deliver(&mut g, &w, "meme.sputnikv2.testnet", 1_000, "stake");
}
