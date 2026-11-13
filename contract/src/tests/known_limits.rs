//! Limits the contract cannot close on its own, written down as tests so they stay visible.
//! Each test passes when the limit behaves as documented in the README ("Known limits").
use super::*;

#[test]
fn a_creator_with_a_second_wallet_can_outvote_the_holders() {
    // Vote weight belongs to accounts, and the contract can't tell that two accounts are one
    // person. A creator who moves liquid tokens to another wallet long before the death gets
    // that wallet's full weight, and can vote a takeover down. Holders' defence is the
    // public record: the transfer is on-chain, and the app's Activity tab shows it.
    let mut w = World::new();
    let mut c = launch(&w);
    for h in ["alice.testnet", "bob.testnet", "carol.testnet", "devalt.testnet"] {
        register(&mut c, &w, h);
    }
    send(&mut c, &w, "dev.testnet", "devalt.testnet", 500_000_000);   // the creator's own second wallet
    send(&mut c, &w, "dev.testnet", "alice.testnet", 150_000_000);
    send(&mut c, &w, "dev.testnet", "bob.testnet", 100_000_000);
    send(&mut c, &w, "dev.testnet", "carol.testnet", 50_000_000);
    w.wait(3 * DAY + 1);                                                 // the creator stops pulsing

    w.at("alice.testnet", 0);
    c.t_start(&w, acc("ninecat.sputnikv2.testnet"));
    for h in ["alice.testnet", "bob.testnet", "carol.testnet"] {
        w.at(h, 0);
        c.t_vote(&w, true);
    }
    w.at("devalt.testnet", 0);
    let ballot = c.t_vote(&w, false);
    assert!(ballot.weight.0 > 0, "the second wallet held before the death, so it has weight");
    w.wait(DAY);
    w.at("alice.testnet", 0);
    assert!(!c.t_finalize(&w), "500M held by the creator's second wallet outweighs 300M of honest holders");
    assert!(c.get_lore(None, None).last().unwrap().note.contains("holders voted no"));
}

#[test]
fn the_creator_account_itself_never_has_weight() {
    // What the contract can enforce: the creator's own account and its locked allocation never vote.
    let mut w = World::new();
    let mut c = launch(&w);
    early_holders(&mut c, &w);
    w.wait(3 * DAY + 1);
    assert_eq!(c.get_vote_weight(acc("dev.testnet")).0, 0);
    w.at("alice.testnet", 0);
    c.t_start(&w, acc("ninecat.sputnikv2.testnet"));
    w.at("dev.testnet", 0);
    let r = panics(|| { c.t_vote(&w, false); });
    assert!(r.unwrap_or_default().contains("the keeper and creator cannot vote"));
}
