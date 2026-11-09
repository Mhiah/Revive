//! Random histories on a connected coin: stakes, unstakes (some failing and rolled back),
//! pulses, takeovers and votes. After every step the guard's books must balance.
use super::*;

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

const PEOPLE: [&str; 5] = ["alice.testnet", "bob.testnet", "carol.testnet", "whale.testnet", "erin.testnet"];

#[test]
fn random_histories_keep_the_guards_books_balanced() {
    for seed in 1..=30u64 {
        let mut rng = Rng(seed * 7_919 + 3);
        let mut w = W { t: START };
        let mut g = setup(&w);
        let mut held: u128 = 700_000_000 * T; // what the token would say the guard holds
        let mut lives = 0;
        let mut daos = 0;
        let mut voted = std::collections::HashSet::new();
        for step in 0..120 {
            let s = g.get_status();
            let dead = w.t > s.flatline_at;
            match rng.below(8) {
                0 | 1 => {
                    let p = PEOPLE[rng.below(5) as usize];
                    let n = 1 + rng.below(50_000_000) as u128;
                    deliver(&mut g, &w, p, n, "stake");
                    held += n * T;
                }
                2 => {
                    let p = PEOPLE[rng.below(5) as usize];
                    let st = g.get_stake(acc(p)).0;
                    if st > 0 {
                        let amt = 1 + (rng.next() as u128) % st;
                        w.at(p, 1);
                        let since = g.stakers.get(&acc(p)).unwrap().since;
                        let _ = g.unstake(U128(amt));
                        if rng.below(4) == 0 {
                            // The token transfer failed: the callback puts the stake back.
                            w.at("meme-guard.revive.testnet", 0);
                            g.on_unstake(acc(p), U128(amt), since, Err(PromiseError::Failed));
                        } else {
                            held -= amt;
                        }
                    }
                }
                3 => w.t += rng.below(5 * DAY),
                4 => {
                    if !s.takeover.as_ref().is_some_and(|tk| w.t >= tk.ends_at) {
                        w.at(s.keeper.as_str(), if dead { s.config.comeback_penalty.0 } else { 0 });
                        g.pulse();
                    }
                }
                _ => {
                    if let Some(tk) = s.takeover.clone() {
                        if w.t >= tk.ends_at {
                            w.at("anyone.testnet", 0);
                            if g.finalize_takeover() {
                                lives += 1;
                            }
                        } else {
                            let p = PEOPLE[rng.below(5) as usize];
                            if !voted.contains(&(tk.id, p)) && g.weight_at(&acc(p), tk.flatline_at) > 0 {
                                w.at(p, 0);
                                g.vote(rng.below(3) > 0);
                                voted.insert((tk.id, p));
                            }
                        }
                    } else if dead {
                        let p = PEOPLE[rng.below(5) as usize];
                        if g.weight_at(&acc(p), s.flatline_at) > 0 {
                            daos += 1;
                            w.at(p, 0);
                            g.start_takeover(acc(&format!("d{daos}.sputnikv2.testnet")));
                        }
                    }
                }
            }
            let s = g.get_status();
            let ctx = format!("seed {seed}, step {step}");
            let stakes: u128 = PEOPLE.iter().map(|p| g.get_stake(acc(p)).0).sum();
            assert_eq!(stakes, s.staked.0, "{ctx}: stakes add up");
            assert_eq!(held, s.staked.0 + s.dev_locked.0 + s.dev_burned.0, "{ctx}: the guard holds exactly what it owes");
            let f = g.flatline_at();
            assert!(g.eligible_total(f).abs_diff(honest_total(&g, &PEOPLE, f)) <= 6, "{ctx}: eligible total");
            assert_eq!(s.lives, lives, "{ctx}: lives");
        }
    }
}
