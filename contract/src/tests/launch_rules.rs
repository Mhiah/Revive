//! Settings a creator could pick to switch the takeover off are refused at launch,
//! and every takeover call pays its own way (bond and storage).
use super::*;

fn config() -> LaunchConfig {
    LaunchConfig {
        name: "Revive Cat".into(),
        symbol: "NINE".into(),
        icon: None,
        links: None,
        total_supply: U128(1_000_000_000 * TOKEN),
        dev_allocation: U128(100_000_000 * TOKEN),
        distributor: acc("dev.testnet"),
        pulse_interval_secs: 3 * DAY,
        vote_duration_secs: DAY,
        quorum_bps: 1_000,
        vest_duration_secs: 100 * DAY,
        spend_cap: U128(NEAR),
        spend_period_secs: 7 * DAY,
        comeback_penalty: U128(2 * NEAR),
        dao_suffix: ".sputnikv2.testnet".into(),
        creator: None,
    }
}

fn refused(cfg: LaunchConfig) -> String {
    let w = World::new();
    w.at("dev.testnet", 5 * NEAR);
    panics(|| {
        Revive::new(cfg);
    })
    .expect("this launch must be refused")
}

#[test]
fn vesting_shorter_than_four_heartbeats_is_refused() {
    // With no vesting the creator could claim the whole allocation at launch and walk away.
    let mut c = config();
    c.vest_duration_secs = 0;
    assert!(refused(c).contains("vest over at least 4 heartbeats"));
    let mut c = config();
    c.vest_duration_secs = 4 * 3 * DAY - 1;
    assert!(refused(c).contains("vest over at least 4 heartbeats"));
}

#[test]
fn a_free_comeback_is_refused() {
    // With no penalty a late dev could cancel every takeover vote for nothing.
    let mut c = config();
    c.comeback_penalty = U128(0);
    assert!(refused(c).contains("comeback penalty must be at least 0.01 NEAR"));
}

#[test]
fn a_heartbeat_or_vote_longer_than_30_days_is_refused() {
    // A heartbeat of years would mean the coin can never die.
    let mut c = config();
    c.pulse_interval_secs = MAX_PERIOD_SECS + 1;
    c.vest_duration_secs = 200 * DAY;
    assert!(refused(c).contains("pulse interval must be 60s to 30 days"));
    let mut c = config();
    c.vote_duration_secs = MAX_PERIOD_SECS + 1;
    assert!(refused(c).contains("vote must last 60s to 30 days"));
}

#[test]
fn oversized_names_icons_and_links_are_refused() {
    let mut c = config();
    c.name = "x".repeat(65);
    assert!(refused(c).contains("name must be"));
    let mut c = config();
    c.icon = Some("x".repeat(MAX_ICON_LEN + 1));
    assert!(refused(c).contains("icon too large"));
    let mut c = config();
    c.links = Some("x".repeat(MAX_LINKS_LEN + 1));
    assert!(refused(c).contains("links too long"));
}

#[test]
fn the_longest_allowed_settings_launch() {
    let w = World::new();
    w.at("dev.testnet", 5 * NEAR);
    let mut c = config();
    c.pulse_interval_secs = MAX_PERIOD_SECS;
    c.vote_duration_secs = MAX_PERIOD_SECS;
    c.vest_duration_secs = 4 * MAX_PERIOD_SECS;
    c.comeback_penalty = U128(MIN_PENALTY);
    c.icon = Some("x".repeat(MAX_ICON_LEN));
    let coin = Revive::new(c);
    assert_eq!(coin.get_status().status, "alive");
}

#[test]
fn opening_a_takeover_needs_the_bond() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    w.at("alice.testnet", TAKEOVER_BOND - 1);
    let err = panics(|| {
        c.start_takeover(acc("ninecat.sputnikv2.testnet"));
    })
    .expect("no bond, no takeover");
    assert!(err.contains("attach at least"), "{err}");
}

#[test]
fn voting_and_closing_pay_for_their_own_storage() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    dead_and_open(&mut c, &mut w);
    w.at("bob.testnet", 0);
    assert!(panics(|| {
        c.vote(true);
    })
    .unwrap()
    .contains("for storage"));
    // (A failed call is rolled back on-chain; the unit-test harness keeps its writes, so a
    // different holder casts the real vote.)
    w.at("alice.testnet", 0);
    c.t_vote(&w, true);
    w.wait(DAY);
    w.at("carol.testnet", 0);
    assert!(panics(|| {
        c.finalize_takeover();
    })
    .unwrap()
    .contains("for storage"));
}

#[test]
fn a_failed_takeover_leaves_its_bond_in_the_treasury() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    let before = c.get_status().treasury.0;
    dead_and_open(&mut c, &mut w);
    w.at("carol.testnet", 0);
    c.t_vote(&w, false);
    w.wait(DAY);
    w.at("carol.testnet", 0);
    assert!(!c.t_finalize(&w));
    assert_eq!(c.get_status().treasury.0, before + TAKEOVER_BOND);
}

#[test]
fn a_passed_takeover_returns_the_bond_not_the_treasury() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    let before = c.get_status().treasury.0;
    dead_and_open(&mut c, &mut w);
    for v in ["alice.testnet", "bob.testnet"] {
        w.at(v, 0);
        c.t_vote(&w, true);
    }
    w.wait(DAY);
    w.at("bob.testnet", 0);
    assert!(c.t_finalize(&w));
    assert_eq!(c.get_status().treasury.0, before, "the bond goes back to the proposer");
}

#[test]
fn a_comeback_gives_the_proposer_their_bond_back() {
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    let before = c.get_status().treasury.0;
    dead_and_open(&mut c, &mut w);
    w.at("dev.testnet", 2 * NEAR);
    c.pulse();
    assert_eq!(c.get_status().treasury.0, before + 2 * NEAR, "only the penalty goes to the treasury");
}

#[test]
fn the_keeper_pays_for_a_bigger_icon() {
    let w = World::new();
    let mut c = launch(&w);
    w.at("dev.testnet", 0);
    assert!(panics(|| c.set_links(Some("x".repeat(4_000)), None)).unwrap().contains("for storage"));
    w.at("dev.testnet", NEAR / 10);
    c.set_links(Some("x".repeat(4_000)), None);
    assert_eq!(c.get_status().icon.unwrap().len(), 4_000);
}

#[test]
fn spending_cannot_dip_into_the_nears_that_pay_for_storage() {
    let w = World::new();
    let mut c = launch(&w);
    // The coin's account holds less than its treasury counter claims (as if storage ate it).
    let mut b = VMContextBuilder::new();
    b.current_account_id(acc("revive.testnet"))
        .predecessor_account_id(acc("dev.testnet"))
        .block_timestamp(w.t * 1_000_000_000)
        .account_balance(NearToken::from_yoctonear(NEAR / 2));
    testing_env!(b.build());
    let err = panics(|| {
        let _ = c.spend(acc("alice.testnet"), U128(NEAR / 2), "x".into());
    })
    .expect("the spend must be refused");
    assert!(err.contains("needed to pay for the coin's storage"), "{err}");
}
