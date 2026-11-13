//! 60-day simulation that drives the real Revive contract (not a model) through
//! six community scenarios and writes docs/simulation.json (the data behind the README's simulation section).
//!
//!     cd contract && cargo run --example simulate
//!
//! Every action below is a real call into the contract under a mocked NEAR
//! runtime with a moving clock, so the numbers are what the deployed code does.

use near_sdk::json_types::U128;
use near_sdk::serde_json::{json, Value};
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{testing_env, AccountId, NearToken};
use revive::{LaunchConfig, Revive, TAKEOVER_BOND};

const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const TOKEN: u128 = 1_000_000_000_000_000_000;
const DAY: u64 = 86_400;
const HOUR: u64 = 3_600;
const START: u64 = 1_800_000_000;
const DAYS: u64 = 60;

const HOLDERS: [(&str, u128); 12] = [
    ("ada", 60), ("bayo", 45), ("chi", 40), ("dami", 35), ("efe", 30), ("femi", 25),
    ("gbenga", 20), ("halima", 15), ("ife", 12), ("jide", 10), ("kemi", 8), ("lola", 5),
];

fn acc(s: &str) -> AccountId {
    if s.contains('.') { s.parse().unwrap() } else { format!("{s}.testnet").parse().unwrap() }
}

struct Sim {
    c: Revive,
    t: u64,
    days: Vec<Value>,
    events: Vec<Value>,
    rng: u64,
}

fn ctx(t: u64, who: &str, deposit: u128) {
    let mut b = VMContextBuilder::new();
    b.current_account_id(acc("nine.revive"))
        .predecessor_account_id(acc(who))
        .signer_account_id(acc(who))
        .block_timestamp(t * 1_000_000_000)
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_yoctonear(1_000 * NEAR));
    testing_env!(b.build());
}

impl Sim {
    fn at(&self, who: &str, deposit: u128) {
        ctx(self.t, who, deposit);
    }

    fn new() -> Self {
        Self::with_params(2_000, 3 * DAY)
    }

    fn with_params(quorum_bps: u16, pulse_interval_secs: u64) -> Self {
        ctx(START, "dev", 5 * NEAR);
        // Every simulated coin starts from empty storage, like a freshly deployed contract.
        near_sdk::mock::with_mocked_blockchain(|b| {
            let _ = b.take_storage();
        });
        let c = Revive::new(LaunchConfig {
            name: "Nine Lives".into(),
            symbol: "NINE".into(),
            icon: None,
            links: None,
            total_supply: U128(1_000_000_000 * TOKEN),
            dev_allocation: U128(100_000_000 * TOKEN),
            distributor: acc("pool"),
            pulse_interval_secs,
            vote_duration_secs: DAY,
            quorum_bps,
            vest_duration_secs: 90 * DAY,
            spend_cap: U128(NEAR),
            spend_period_secs: 7 * DAY,
            comeback_penalty: U128(2 * NEAR),
            dao_suffix: ".sputnikv2.testnet".into(),
            creator: None,
        });
        let mut s = Sim { c, t: START, days: vec![], events: vec![], rng: 42 };
        for (h, _) in HOLDERS.iter().chain([("whale", 0)].iter()) {
            s.at(h, 2_000_000_000_000_000_000_000);
            s.c.storage_deposit(None, None);
        }
        // Launch week: early holders buy from the pool.
        for (i, (h, m)) in HOLDERS.iter().enumerate() {
            s.t = START + (i as u64 % 4) * DAY / 2;
            s.send("pool", h, m * 1_000_000);
        }
        s.t = START;
        s.event(0, "launch", "1B $NINE: 900M in the pool, 100M creator tokens locked, vesting over 90 days");
        s
    }

    fn send(&mut self, from: &str, to: &str, tokens: u128) {
        self.at(from, 1);
        self.c.ft_transfer(acc(to), U128(tokens * TOKEN), None);
    }

    fn try_call<F: FnOnce(&mut Revive)>(&mut self, who: &str, deposit: u128, f: F) -> Result<(), String> {
        self.at(who, deposit);
        let c = &mut self.c;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(c))).map_err(|e| {
            e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
        })
    }

    fn rand(&mut self, n: u64) -> u64 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.rng >> 33) % n
    }

    fn event(&mut self, day: u64, kind: &str, text: &str) {
        self.events.push(json!({ "day": day, "kind": kind, "text": text }));
    }

    fn record(&mut self, day: u64) {
        self.at("observer", 0);
        let s = self.c.get_status();
        let tk = s.takeover.as_ref();
        self.days.push(json!({
            "day": day,
            "status": s.status,
            "supply": (s.total_supply.0 / TOKEN) as u64,
            "dev_locked": (s.dev_locked.0 / TOKEN) as u64,
            "dev_burned": (s.dev_burned.0 / TOKEN) as u64,
            "treasury": s.treasury.0 as f64 / NEAR as f64,
            "lives": s.lives,
            "keeper_is_dao": s.keeper.as_str().ends_with(".sputnikv2.testnet"),
            "yes_pct": tk.map(|t| t.yes.0 as f64 * 100.0 / (t.total_eligible.0.max(1)) as f64),
            "no_pct": tk.map(|t| t.no.0 as f64 * 100.0 / (t.total_eligible.0.max(1)) as f64),
        }));
    }

    /// Everyday life: tips into the treasury and a couple of small buys.
    fn daily_noise(&mut self) {
        self.at("tipjar", NEAR / 5);
        self.c.fund_treasury();
        for _ in 0..2 {
            let i = self.rand(HOLDERS.len() as u64) as usize;
            let amount = 100_000 + self.rand(400_000) as u128;
            self.send("pool", HOLDERS[i].0, amount);
        }
    }

    fn open_takeover(&mut self, day: u64, dao: &str) {
        let proposer = HOLDERS[1].0;
        let r = self.try_call(proposer, TAKEOVER_BOND + NEAR / 100, |c| {
            c.start_takeover(dao.parse().unwrap());
        });
        match r {
            Ok(()) => self.event(day, "takeover", &format!("{proposer} opens a takeover, nominating {dao}")),
            Err(e) => self.event(day, "rejected", &format!("takeover refused: {e}")),
        }
    }

    fn vote_all(&mut self, day: u64, yes_share: f64) {
        let mut yes = 0;
        let mut no = 0;
        for (i, (h, _)) in HOLDERS.iter().enumerate() {
            let support = (i as f64) < (HOLDERS.len() as f64 * yes_share);
            if self.try_call(h, NEAR / 100, |c| { c.vote(support); }).is_ok() {
                if support { yes += 1 } else { no += 1 }
            }
        }
        self.event(day, "vote", &format!("{yes} holders vote yes, {no} vote no"));
    }

    fn finalize(&mut self, day: u64) -> bool {
        self.at("anyone", NEAR / 100);
        let burned_before = self.c.get_status().dev_burned.0;
        let ok = self.c.finalize_takeover();
        let s = self.c.get_status();
        if ok {
            let burned = (s.dev_burned.0 - burned_before) as f64 / TOKEN as f64 / 1e6;
            let burn_text = if burned > 0.0 { format!("{burned:.1}M unvested creator tokens burned") } else { "nothing left to burn".to_string() };
            self.event(day, "revived", &format!("revived: {} is keeper, {burn_text}, life #{}", s.keeper, s.lives));
        } else {
            self.event(day, "failed", "takeover failed: the coin stays flatlined under its old keeper");
        }
        ok
    }

    fn pulse(&mut self, who: &str, deposit: u128) -> Result<(), String> {
        self.try_call(who, deposit, |c| c.pulse())
    }
}

/// What the keeper(s) and holders do on each day of one scenario.
#[derive(Clone)]
struct Plan {
    id: &'static str,
    name: &'static str,
    blurb: &'static str,
    dev_quits: Option<u64>,
    whale_buys_after_death: bool,
    comeback: bool,
    yes_share: f64,
    dao_quits: Option<u64>,
}

fn run(p: &Plan) -> Value {
    let mut s = Sim::new();
    let mut keeper = "dev".to_string();
    let mut dao_round = 0;
    let mut vote_opened: Option<u64> = None;
    let mut failed = false;
    let mut came_back = false;
    for day in 0..=DAYS {
        s.t = START + day * DAY + 12 * HOUR;
        if day > 0 {
            s.daily_noise();
        }
        let dev_active = keeper == "dev" && p.dev_quits.is_none_or(|q| day < q);
        let dao_active = keeper != "dev" && p.dao_quits.is_none_or(|q| day < q || dao_round > 1);
        // Keepers pulse every other day while they are around.
        if (dev_active || dao_active) && day % 2 == 0 {
            let _ = s.pulse(&keeper.clone(), 0);
            if dev_active && day % 6 == 0 && day > 0 {
                let _ = s.try_call("dev", 0, |c| { c.claim_vested(); });
            }
            if day % 6 == 0 && day > 0 {
                let _ = s.try_call(&keeper.clone(), 0, |c| { let _ = c.spend(acc("artist"), U128(NEAR / 2), "memes".into()); });
            }
        }
        if p.dev_quits == Some(day) && keeper == "dev" {
            s.event(day, "silent", "the dev stops pulsing");
        }
        if keeper != "dev" && p.dao_quits == Some(day) && dao_round == 1 {
            s.event(day, "silent", "the steward DAO stops pulsing too");
        }

        s.at("observer", 0);
        let status = s.c.get_status().status;
        if status == "flatlined" && vote_opened.is_none() && !failed {
            if p.whale_buys_after_death && keeper == "dev" {
                s.send("pool", "whale", 300_000_000);
                s.event(day, "whale", "a whale buys 300M after the death to swing the vote");
            }
            dao_round += 1;
            let dao = format!("ninelives-{dao_round}.sputnikv2.testnet");
            s.open_takeover(day, &dao);
            vote_opened = Some(day);
            if p.whale_buys_after_death && keeper == "dev" {
                let r = s.try_call("whale", NEAR / 100, |c| { c.vote(false); });
                if let Err(e) = r {
                    s.event(day, "rejected", &format!("whale's 300M vote refused: {}", e.split(':').next().unwrap_or(&e)));
                }
            }
            s.vote_all(day, p.yes_share);
            if p.comeback && keeper == "dev" {
                s.t += 6 * HOUR;
                match s.pulse("dev", 2 * NEAR) {
                    Ok(()) => {
                        came_back = true;
                        s.event(day, "comeback", "the dev comes back mid-vote, pays 2 NEAR into the treasury; vote cancelled")
                    }
                    Err(e) => s.event(day, "rejected", &e),
                }
                s.t -= 6 * HOUR;
                vote_opened = None;
                dao_round -= 1;
            }
        } else if let Some(opened) = vote_opened {
            if day == opened + 1 {
                if s.finalize(day) {
                    keeper = format!("ninelives-{dao_round}.sputnikv2.testnet");
                    let _ = s.pulse(&keeper.clone(), 0);
                } else {
                    failed = true;
                }
                vote_opened = None;
            }
        }
        if came_back && day % 2 == 0 {
            // After the comeback the dev keeps pulsing.
            let _ = s.pulse("dev", 0);
        }
        s.record(day);
    }
    json!({ "id": p.id, "name": p.name, "blurb": p.blurb, "days": s.days, "events": s.events })
}

/// Quorum test: the dev launches and vanishes; only some holders show up to vote yes.
/// Does the takeover pass at this quorum?
fn quorum_case(quorum_bps: u16, voters: &'static [usize], label: &'static str) -> Value {
    let mut s = Sim::with_params(quorum_bps, 3 * DAY);
    s.t = START + 4 * DAY;
    let proposer = HOLDERS[voters[0]].0;
    let _ = s.try_call(proposer, TAKEOVER_BOND + NEAR / 100, |c| { c.start_takeover("ninelives-1.sputnikv2.testnet".parse().unwrap()); });
    for &i in voters {
        let _ = s.try_call(HOLDERS[i].0, NEAR / 100, |c| { c.vote(true); });
    }
    s.at("observer", 0);
    let tk = s.c.get_status().takeover.unwrap();
    let yes_pct = tk.yes.0 as f64 * 100.0 / tk.total_eligible.0 as f64;
    s.t += DAY;
    s.at("anyone", NEAR / 100);
    let passed = s.c.finalize_takeover();
    json!({ "quorum_pct": quorum_bps / 100, "voters": label, "yes_pct": (yes_pct * 10.0).round() / 10.0, "passed": passed })
}

/// Heartbeat test: a loyal but irregular dev pulses on about half of all days, for 60 days.
/// How often does the coin flatline by accident, and how long would holders wait after a real quit?
fn heartbeat_case(interval_days: u64) -> Value {
    let mut deaths = 0u32;
    let mut dead_days = 0u32;
    let runs = 20u64;
    for seed in 0..runs {
        let mut s = Sim::with_params(2_000, interval_days * DAY);
        s.rng = 1_000 + seed;
        let mut was_dead = false;
        for day in 1..=DAYS {
            s.t = START + day * DAY + 12 * HOUR;
            s.at("observer", 0);
            let dead = s.c.get_status().status != "alive";
            if dead {
                dead_days += 1;
                if !was_dead {
                    deaths += 1;
                }
            }
            was_dead = dead;
            if s.rand(2) == 0 {
                let _ = s.pulse("dev", if dead { 2 * NEAR } else { 0 });
                was_dead = false;
            }
        }
    }
    json!({
        "interval_days": interval_days,
        "accidental_deaths_per_60_days": deaths as f64 / runs as f64,
        "accidental_dead_days_per_60_days": dead_days as f64 / runs as f64,
        "days_holders_wait_after_a_real_quit": interval_days,
    })
}

fn main() {
    // The simulation checks refusals by catching panics, which a panic = "abort" build can't do
    // (the release profile is set up for the wasm). Say so instead of dying without output.
    if cfg!(panic = "abort") {
        eprintln!("run the simulation without --release: cd contract && cargo run --example simulate");
        std::process::exit(2);
    }
    std::panic::set_hook(Box::new(|_| {}));
    let plans = [
        Plan { id: "steady", name: "Dev stays", blurb: "The dev pulses every other day for 60 days, claims vesting weekly and spends on memes.",
            dev_quits: None, whale_buys_after_death: false, comeback: false, yes_share: 0.8, dao_quits: None },
        Plan { id: "revival", name: "Dev quits, holders revive", blurb: "The dev goes silent on day 12. Holders take over and a DAO keeps the coin alive.",
            dev_quits: Some(12), whale_buys_after_death: false, comeback: false, yes_share: 0.8, dao_quits: None },
        Plan { id: "whale", name: "Whale buys after the death", blurb: "Same death, but a whale buys 300M after the flatline and tries to vote no.",
            dev_quits: Some(12), whale_buys_after_death: true, comeback: false, yes_share: 0.8, dao_quits: None },
        Plan { id: "comeback", name: "Dev comes back mid-vote", blurb: "The dev goes silent on day 12, then returns while the takeover vote is open and pays the penalty.",
            dev_quits: Some(12), whale_buys_after_death: false, comeback: true, yes_share: 0.8, dao_quits: None },
        Plan { id: "nine-lives", name: "The DAO dies too", blurb: "Revived on day 15; the first steward DAO goes quiet on day 35 and holders revive the coin a second time.",
            dev_quits: Some(12), whale_buys_after_death: false, comeback: false, yes_share: 0.8, dao_quits: Some(35) },
        Plan { id: "rejected", name: "Holders vote no", blurb: "The dev goes silent, a takeover opens, but most of the weight votes no. The coin stays dead.",
            dev_quits: Some(12), whale_buys_after_death: false, comeback: false, yes_share: 0.2, dao_quits: None },
    ];
    // Picking the parameters: the same contract under different quorums and heartbeats.
    let groups: [(&'static [usize], &'static str); 3] = [
        (&[10, 11], "the 2 smallest holders"),
        (&[3, 4, 5], "3 mid-size holders (a quarter of holders)"),
        (&[0, 2, 4, 6, 8, 10], "half of holders"),
    ];
    let mut quorum_sweep = vec![];
    for q in [1_000u16, 2_000, 3_500] {
        for (v, l) in groups {
            quorum_sweep.push(std::thread::spawn(move || quorum_case(q, v, l)).join().unwrap());
        }
    }
    let heartbeat_sweep: Vec<Value> = [1u64, 3, 7].into_iter().map(|d| std::thread::spawn(move || heartbeat_case(d)).join().unwrap()).collect();
    let out = json!({
        "generated_by": "contract/examples/simulate.rs (the real Revive contract, mocked clock)",
        "params": {
            "supply": "1,000,000,000", "creator_locked": "100,000,000 (10%), vesting over 90 days",
            "pulse_interval": "3 days", "vote": "1 day", "quorum": "20% of eligible weight",
            "comeback_penalty": "2 NEAR", "spend_cap": "1 NEAR per week", "treasury": "5 NEAR seed + 0.2 NEAR tips a day"
        },
        // Each scenario runs on its own thread: the mocked chain storage is per thread.
        "scenarios": plans.into_iter().map(|p| std::thread::spawn(move || run(&p)).join().unwrap()).collect::<Vec<_>>(),
        // Picking the parameters: the same contract under different quorums and heartbeats.
        "sweeps": { "quorum": quorum_sweep, "heartbeat": heartbeat_sweep },
    });
    let path = std::env::args().nth(1).unwrap_or_else(|| "../docs/simulation.json".into());
    std::fs::write(&path, near_sdk::serde_json::to_string_pretty(&out).unwrap()).unwrap();
    for sc in out["scenarios"].as_array().unwrap() {
        let last = sc["days"].as_array().unwrap().last().unwrap();
        println!("{:<28} supply {:>13}  treasury {:>6.2}  lives {}  status {}", sc["name"].as_str().unwrap(),
            last["supply"], last["treasury"].as_f64().unwrap(), last["lives"], last["status"]);
    }
    for q in out["sweeps"]["quorum"].as_array().unwrap() {
        println!("quorum {:>2}%  {:<44} yes {:>5}%  passed {}", q["quorum_pct"], q["voters"].as_str().unwrap(), q["yes_pct"], q["passed"]);
    }
    for h in out["sweeps"]["heartbeat"].as_array().unwrap() {
        println!("heartbeat {} days: {} accidental deaths, {} dead days per 60 days", h["interval_days"], h["accidental_deaths_per_60_days"], h["accidental_dead_days_per_60_days"]);
    }
    println!("wrote {path}");
}
