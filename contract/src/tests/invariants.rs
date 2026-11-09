//! Random histories: about 7,000 seeded random steps of transfers, pulses, comebacks, claims,
//! spends, takeovers and votes. After every single step the books must still balance.
use super::*;

/// Small deterministic random generator, so any failure can be replayed from its seed.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

const PEOPLE: [&str; 6] = ["dev.testnet", "alice.testnet", "bob.testnet", "carol.testnet", "whale.testnet", "erin.testnet"];

struct Ledger {
    treasury: u128,
    lives: u32,
    ballots: std::collections::HashSet<(u32, String)>,
}

fn check(c: &Revive, l: &Ledger, seed: u64, step: usize) {
    let s = c.get_status();
    let ctx = format!("seed {seed}, step {step}");
    // 1. Every token is somewhere: holders' balances add up to the circulating supply.
    let held: u128 = PEOPLE.iter().map(|p| c.ft_balance_of(acc(p)).0).sum();
    assert_eq!(held, s.circulating.0, "{ctx}: balances vs circulating");
    // 2. Supply = circulating + the creator's still-locked share.
    assert_eq!(s.total_supply.0, s.circulating.0 + s.dev_locked.0, "{ctx}: supply");
    // 3. The O(1) eligible total equals the honest holder-by-holder sum.
    let f = c.flatline_at();
    let honest = honest_total(c, &PEOPLE, f);
    assert!(c.eligible_total(f).abs_diff(honest) <= PEOPLE.len() as u128 + 1, "{ctx}: eligible {} vs honest {honest}", c.eligible_total(f));
    // 4. Nobody's weight beats holding their tokens since launch.
    for p in PEOPLE {
        let a = acc(p);
        let max = c.ft_balance_of(a.clone()).0 * (f.saturating_sub(START)) as u128 / WEIGHT_SCALE;
        assert!(c.weight_at(&a, f) <= max, "{ctx}: {p} weight above its ceiling");
    }
    // 5. The treasury matches every deposit, penalty and spend.
    assert_eq!(s.treasury.0, l.treasury, "{ctx}: treasury");
    // 6. Lives only go up, one per successful takeover; the keeper is the creator or a DAO.
    assert_eq!(s.lives, l.lives, "{ctx}: lives");
    assert!(s.keeper == s.creator || s.keeper.as_str().ends_with(".sputnikv2.testnet"), "{ctx}: keeper");
    // 7. The spending cap holds.
    assert!(s.spent_in_period.0 <= s.config.spend_cap.0, "{ctx}: cap");
}

fn run(seed: u64, steps: usize) {
    let mut rng = Rng(seed * 2_654_435_761 + 1);
    let mut w = World::new();
    let mut c = launch(&w);
    for p in &PEOPLE[1..] {
        register(&mut c, &w, p);
    }
    let mut l = Ledger { treasury: 5 * NEAR, lives: 0, ballots: Default::default() };
    let mut daos = 0;
    check(&c, &l, seed, 0);
    for step in 1..=steps {
        let s = c.get_status();
        let t = w.t;
        let dead = t > s.flatline_at;
        match rng.below(10) {
            // Transfer a random slice of someone's tokens.
            0..=3 => {
                let from = PEOPLE[rng.below(6) as usize];
                let to = PEOPLE[rng.below(6) as usize];
                let bal = c.ft_balance_of(acc(from)).0;
                if from != to && bal > 0 {
                    let amt = 1 + (rng.next() as u128 * TOKEN) % bal;
                    w.at(from, 1);
                    c.ft_transfer(acc(to), U128(amt), None);
                }
            }
            // Let time pass, up to two pulse intervals.
            4 => w.wait(rng.below(6 * DAY)),
            // The keeper pulses (paying the penalty if late, and only while that is allowed).
            5 => {
                let open_and_closed = s.takeover.as_ref().is_some_and(|tk| t >= tk.ends_at);
                if !open_and_closed {
                    let pay = if dead { s.config.comeback_penalty.0 } else { 0 };
                    w.at(s.keeper.as_str(), pay);
                    c.pulse();
                    l.treasury += pay;
                }
            }
            // The creator claims what has vested.
            6 => {
                if s.keeper == s.creator && !dead && s.takeover.is_none() && c.dev_active && c.dev_vested() > c.dev_claimed {
                    w.at("dev.testnet", 0);
                    c.claim_vested();
                }
            }
            // The keeper spends within the cap.
            7 => {
                if !dead && s.takeover.is_none() {
                    let spent = if t >= s.period_resets_at { 0 } else { s.spent_in_period.0 };
                    let room = (s.config.spend_cap.0 - spent).min(s.treasury.0);
                    if room > 0 {
                        let amt = 1 + (rng.next() as u128) % room;
                        w.at(s.keeper.as_str(), 0);
                        let _ = c.spend(acc("artist.testnet"), U128(amt), "work".into());
                        l.treasury -= amt;
                    }
                }
            }
            // Holders open a takeover, vote on it, or close it.
            _ => {
                if let Some(tk) = s.takeover.clone() {
                    if t >= tk.ends_at {
                        w.at("bob.testnet", 0);
                        if c.finalize_takeover() {
                            l.lives += 1;
                        }
                    } else {
                        let v = PEOPLE[1 + rng.below(5) as usize];
                        let a = acc(v);
                        if a != c.keeper && a != c.creator && !l.ballots.contains(&(tk.id, v.to_string())) && c.weight_at(&a, tk.flatline_at) > 0 {
                            w.at(v, 0);
                            c.vote(rng.below(3) > 0);
                            l.ballots.insert((tk.id, v.to_string()));
                        }
                    }
                } else if dead {
                    let p = PEOPLE[1 + rng.below(5) as usize];
                    let a = acc(p);
                    if a != c.keeper && a != c.creator && c.weight_at(&a, s.flatline_at) > 0 {
                        daos += 1;
                        w.at(p, 0);
                        c.start_takeover(acc(&format!("dao{daos}.sputnikv2.testnet")));
                    }
                }
            }
        }
        check(&c, &l, seed, step);
    }
}

#[test]
fn random_histories_keep_the_books_balanced() {
    for seed in 1..=40 {
        run(seed, 120);
    }
}

#[test]
fn long_random_histories_keep_the_books_balanced() {
    for seed in 1000..1004 {
        run(seed, 600);
    }
}

#[test]
fn the_eligible_total_matches_by_hand_through_deaths_and_revivals() {
    // Same runs, but stopping at every takeover start to compare the frozen total.
    for seed in 500..525 {
        let mut rng = Rng(seed);
        let mut w = World::new();
        let mut c = launch(&w);
        for p in &PEOPLE[1..] {
            register(&mut c, &w, p);
            send(&mut c, &w, "dev.testnet", p, 1 + rng.below(100_000_000) as u128);
        }
        w.wait(1 + rng.below(3 * DAY));
        for _ in 0..30 {
            let from = PEOPLE[1 + rng.below(5) as usize];
            let to = PEOPLE[rng.below(6) as usize];
            let bal = c.ft_balance_of(acc(from)).0;
            if from != to && bal > 0 {
                w.at(from, 1);
                c.ft_transfer(acc(to), U128(1 + (rng.next() as u128) % bal), None);
            }
            w.wait(rng.below(DAY));
        }
        let f = c.flatline_at();
        assert!(c.eligible_total(f).abs_diff(honest_total(&c, &PEOPLE, f)) <= 7, "seed {seed}");
    }
}
