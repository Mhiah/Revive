//! The edges of every deadline: the second the pulse is due, the last second of a vote,
//! and takeovers that follow each other.
use super::*;

#[test]
fn a_pulse_exactly_at_the_deadline_is_on_time() {
    let mut w = World::new();
    let mut c = launch(&w);
    w.wait(3 * DAY);
    w.at("dev.testnet", 0);
    assert_eq!(c.get_status().status, "alive", "the deadline second still counts as alive");
    c.pulse();
    assert!(!c.get_lore(None, None).iter().any(|l| l.kind == "comeback"), "no penalty, no comeback");
}

#[test]
#[should_panic(expected = "late pulse: attach the comeback penalty")]
fn one_second_late_needs_the_penalty() {
    let mut w = World::new();
    let mut c = launch(&w);
    w.wait(3 * DAY + 1);
    w.at("dev.testnet", 0);
    c.pulse();
}

#[test]
#[should_panic(expected = "the coin still has a pulse")]
fn no_takeover_at_the_deadline_second() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY);
    w.at("alice.testnet", 0);
    c.t_start(&w, acc("ninecat.sputnikv2.testnet"));
}

#[test]
fn a_vote_in_the_last_second_counts() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    w.wait(DAY - 1);
    w.at("bob.testnet", 0);
    let b = c.t_vote(&w, true);
    assert!(b.weight.0 > 0);
}

#[test]
#[should_panic(expected = "the vote has closed")]
fn a_vote_at_the_closing_second_is_refused() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    w.wait(DAY);
    w.at("bob.testnet", 0);
    c.t_vote(&w, true);
}

#[test]
#[should_panic(expected = "the vote is still open")]
fn finalizing_one_second_early_is_refused() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    c.t_vote(&w, true);
    w.wait(DAY - 1);
    w.at("bob.testnet", 0);
    c.t_finalize(&w);
}

#[test]
fn finalizing_at_the_closing_second_works() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    c.t_vote(&w, true);
    w.wait(DAY);
    w.at("bob.testnet", 0);
    assert!(c.t_finalize(&w));
}

#[test]
fn a_comeback_in_the_last_second_of_the_vote_cancels_it() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    c.t_vote(&w, true);
    w.wait(DAY - 1);
    w.at("dev.testnet", 2 * NEAR);
    c.pulse();
    let s = c.get_status();
    assert!(s.takeover.is_none());
    assert_eq!(s.keeper, acc("dev.testnet"));
    assert_eq!(s.lives, 0);
}

#[test]
fn a_tie_is_a_no() {
    let mut w = World::new();
    let mut c = launch(&w);
    for h in ["p.testnet", "q.testnet"] {
        register(&mut c, &w, h);
        send(&mut c, &w, "dev.testnet", h, 100_000_000);
    }
    w.wait(3 * DAY + 1);
    w.at("p.testnet", 0);
    c.t_start(&w, acc("ninecat.sputnikv2.testnet"));
    c.t_vote(&w, true);
    w.at("q.testnet", 0);
    c.t_vote(&w, false);
    w.wait(DAY);
    w.at("q.testnet", 0);
    assert!(!c.t_finalize(&w), "equal weights: yes does not beat no");
}

#[test]
fn a_failed_takeover_can_be_followed_by_another() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    c.t_vote(&w, false);
    w.wait(DAY);
    w.at("bob.testnet", 0);
    assert!(!c.t_finalize(&w));
    c.t_start(&w, acc("second.sputnikv2.testnet"));
    c.t_vote(&w, true);
    w.at("alice.testnet", 0);
    c.t_vote(&w, true);
    w.wait(DAY);
    w.at("carol.testnet", 0);
    assert!(c.t_finalize(&w));
    let s = c.get_status();
    assert_eq!(s.keeper, acc("second.sputnikv2.testnet"));
    assert_eq!(s.takeover_count, 2);
}

#[test]
fn a_revival_restarts_the_clock() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    c.t_vote(&w, true);
    w.wait(DAY);
    w.at("bob.testnet", 0);
    c.t_finalize(&w);
    let s = c.get_status();
    assert_eq!(s.status, "alive");
    assert_eq!(s.flatline_at, w.t + 3 * DAY, "the DAO gets a full interval");
    w.wait(3 * DAY + 1);
    w.at("bob.testnet", 0);
    assert_eq!(c.get_status().status, "flatlined", "the DAO must keep pulsing too");
}

#[test]
fn the_dao_keeps_it_alive_after_a_revival() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    c.t_vote(&w, true);
    w.wait(DAY);
    w.at("bob.testnet", 0);
    c.t_finalize(&w);
    for _ in 0..4 {
        w.wait(2 * DAY);
        w.at("ninecat.sputnikv2.testnet", 0);
        c.pulse();
    }
    assert_eq!(c.get_status().status, "alive");
    assert!(panics(|| {
        w.at("dev.testnet", 0);
        c.pulse();
    })
    .unwrap()
    .contains("only the keeper has a pulse"));
}
