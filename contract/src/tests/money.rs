//! Treasury, spending cap, vesting and supply: where the money can go and when.
use super::*;

fn revived(c: &mut Revive, w: &mut World) {
    early_holders(c, w);
    dead_and_open(c, w);
    c.t_vote(w, true);
    w.wait(DAY);
    w.at("bob.testnet", 0);
    assert!(c.t_finalize(w));
}

#[test]
fn spending_exactly_the_cap_is_allowed() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    let _ = c.spend(acc("artist.testnet"), U128(NEAR), "full cap".into());
    assert_eq!(c.get_status().treasury.0, 4 * NEAR);
}

#[test]
#[should_panic(expected = "over the spending cap for this period")]
fn one_yocto_over_the_cap_is_refused() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    let _ = c.spend(acc("artist.testnet"), U128(NEAR), "full cap".into());
    let _ = c.spend(acc("artist.testnet"), U128(1), "one more yocto".into());
}

#[test]
fn the_cap_resets_each_period() {
    let mut w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    let _ = c.spend(acc("artist.testnet"), U128(NEAR), "week one".into());
    w.wait(2 * DAY);
    w.at("dev.testnet", 0);
    c.pulse();
    w.wait(2 * DAY);
    w.at("dev.testnet", 0);
    c.pulse();
    w.wait(3 * DAY);
    w.at("dev.testnet", 0);
    c.pulse();
    let _ = c.spend(acc("artist.testnet"), U128(NEAR), "week two".into());
    assert_eq!(c.get_status().treasury.0, 3 * NEAR);
}

#[test]
#[should_panic(expected = "not enough in the treasury")]
fn spending_more_than_the_treasury_is_refused() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    let _ = c.spend(acc("artist.testnet"), U128(6 * NEAR), "too much".into());
}

#[test]
#[should_panic(expected = "the coin has flatlined")]
fn a_dead_coin_cannot_spend() {
    let mut w = World::new();
    let mut c = launch(&w);
    w.wait(3 * DAY + 1);
    w.at("dev.testnet", 0);
    let _ = c.spend(acc("dev2.testnet"), U128(1), "grab it".into());
}

#[test]
#[should_panic(expected = "only the keeper can spend")]
fn a_holder_cannot_spend() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("alice.testnet", 0);
    let _ = c.spend(acc("alice.testnet"), U128(1), "mine".into());
}

#[test]
#[should_panic(expected = "memo too long")]
fn spend_memos_are_bounded() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    let _ = c.spend(acc("artist.testnet"), U128(1), "x".repeat(201));
}

#[test]
fn the_dao_can_spend_after_a_revival_and_the_old_dev_cannot() {
    let mut w = World::new();
    let mut c = launch(&w);
    revived(&mut c, &mut w);
    w.at("ninecat.sputnikv2.testnet", 0);
    let _ = c.spend(acc("artist.testnet"), U128(NEAR / 2), "community bounty".into());
    assert_eq!(c.get_status().treasury.0, 5 * NEAR - NEAR / 2);
    assert!(panics(|| {
        w.at("dev.testnet", 0);
        let _ = c.spend(acc("dev.testnet"), U128(1), "old habits".into());
    })
    .unwrap()
    .contains("only the keeper can spend"));
}

#[test]
fn anyone_can_fund_the_treasury() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("fan.testnet", 3 * NEAR);
    c.fund_treasury();
    assert_eq!(c.get_status().treasury.0, 8 * NEAR);
}

#[test]
#[should_panic(expected = "attach NEAR to fund the treasury")]
fn funding_with_nothing_is_refused() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("fan.testnet", 0);
    c.fund_treasury();
}

#[test]
fn the_comeback_penalty_lands_in_the_treasury() {
    let mut w = World::new();
    let mut c = launch(&w);
    w.wait(3 * DAY + 5);
    w.at("dev.testnet", 2 * NEAR);
    c.pulse();
    assert_eq!(c.get_status().treasury.0, 7 * NEAR);
}

#[test]
#[should_panic(expected = "nothing vested yet")]
fn nothing_vests_at_launch() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    c.claim_vested();
}

#[test]
fn vesting_is_linear_and_never_over_pays() {
    let mut w = World::new();
    let mut c = launch(&w);
    // 100M over 100 days, pulsing every 2 days.
    let mut claimed = 0u128;
    for day in (2..=110).step_by(2) {
        w.wait(2 * DAY);
        w.at("dev.testnet", 0);
        c.pulse();
        if day % 10 == 0 {
            if let Some(m) = panics(|| { c.claim_vested(); }) {
                assert!(day > 100 && m.contains("nothing vested yet"), "day {day}: {m}");
            }
            claimed = c.get_status().dev_claimed.0;
            let expect = 100_000_000 * TOKEN * (day.min(100) as u128) / 100;
            assert_eq!(claimed, expect, "day {day}");
        }
    }
    assert_eq!(claimed, 100_000_000 * TOKEN, "all of it, never more");
    assert_eq!(c.get_status().dev_locked.0, 0);
}

#[test]
#[should_panic(expected = "the coin has flatlined")]
fn a_dead_creator_cannot_claim() {
    let mut w = World::new();
    let mut c = launch(&w);
    w.wait(50 * DAY);
    w.at("dev.testnet", 0);
    c.claim_vested();
}

#[test]
fn a_revival_burns_only_what_was_not_claimed() {
    let mut w = World::new();
    let mut c = launch(&w);
    for _ in 0..20 {
        w.wait(2 * DAY);
        w.at("dev.testnet", 0);
        c.pulse();
    }
    w.at("dev.testnet", 0);
    c.claim_vested(); // 40 days of 100: 40M
    assert_eq!(c.get_status().dev_claimed.0, 40_000_000 * TOKEN);
    revived(&mut c, &mut w);
    let s = c.get_status();
    assert_eq!(s.dev_burned.0, 60_000_000 * TOKEN);
    assert_eq!(s.total_supply.0, 940_000_000 * TOKEN);
    assert_eq!(s.total_supply.0, s.circulating.0 + s.dev_locked.0, "supply adds up after the burn");
}

#[test]
#[should_panic(expected = "only the living creator can claim")]
fn the_old_creator_cannot_claim_after_a_revival() {
    let mut w = World::new();
    let mut c = launch(&w);
    revived(&mut c, &mut w);
    w.at("dev.testnet", 0);
    c.claim_vested();
}

#[test]
fn a_forced_unregister_burns_and_keeps_the_books_straight() {
    let w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.at("carol.testnet", 1);
    assert!(c.storage_unregister(Some(true)));
    let s = c.get_status();
    assert_eq!(s.total_supply.0, 900_000_000 * TOKEN);
    assert_eq!(s.total_supply.0, s.circulating.0 + s.dev_locked.0);
}

#[test]
#[should_panic(expected = "the keeper and creator stay registered")]
fn the_keeper_cannot_unregister() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 1);
    c.storage_unregister(Some(true));
}

#[test]
#[should_panic(expected = "receiver is not registered")]
fn sending_to_an_unregistered_account_is_refused() {
    let w = World::new();
    let mut c = launch(&w);
    send(&mut c, &w, "dev.testnet", "stranger.testnet", 1);
}

#[test]
#[should_panic(expected = "sender and receiver must differ")]
fn sending_to_yourself_is_refused() {
    let w = World::new();
    let mut c = launch(&w);
    send(&mut c, &w, "dev.testnet", "dev.testnet", 1);
}

#[test]
#[should_panic(expected = "only the keeper can change the links")]
fn only_the_keeper_changes_the_links() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("alice.testnet", 0);
    c.set_links(None, Some("scam.example".into()));
}
