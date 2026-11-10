//! Revive: a NEAR memecoin with a community takeover built into the contract.
//!
//! The keeper (first the creator, later a holder-run Sputnik DAO) must `pulse`
//! on-chain before every deadline. If the pulse stops, the coin flatlines and
//! holders who held *before* the flatline can vote a Sputnik DAO in as the new
//! keeper. A passed vote burns the creator's unvested tokens, hands the
//! treasury and the coin's links to the DAO, and adds a life to the coin.

use near_sdk::json_types::U128;
use near_sdk::serde_json::{self, json};
use near_sdk::store::{LookupMap, Vector};
use near_sdk::{env, log, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault, Promise};

mod token;
#[cfg(test)]
mod tests;

/// Largest supply allowed, so every weight product fits in u128.
pub const MAX_SUPPLY: u128 = 1_000_000_000 * 1_000_000_000_000_000_000;
/// Vote weights are kept in micro-tokens times seconds (yocto / 1e12).
const WEIGHT_SCALE: u128 = 1_000_000_000_000;
/// The creator may keep at most 10% of supply, locked and vesting.
const MAX_DEV_BPS: u128 = 1_000;
/// Longest heartbeat or vote: a coin must be able to die within a month.
pub const MAX_PERIOD_SECS: u64 = 30 * 86_400;
/// Creator tokens must vest over at least this many heartbeats.
pub const MIN_VEST_PULSES: u64 = 4;
/// Smallest comeback penalty, so a late keeper can't cancel a vote for free.
pub const MIN_PENALTY: u128 = 10_000_000_000_000_000_000_000; // 0.01 NEAR
/// Bond for opening a takeover: back to the proposer if it passes or the keeper comes
/// back, into the treasury if it fails. Stops a dust holder from blocking the vote slot.
pub const TAKEOVER_BOND: u128 = 100_000_000_000_000_000_000_000; // 0.1 NEAR
/// Bytes each call pays for (generous; anything unused stays as storage headroom).
/// Writes only land after a call returns, so these are fixed rather than measured.
pub const VOTE_BYTES: u128 = 250;
pub const TAKEOVER_BYTES: u128 = 900;
pub const FINALIZE_BYTES: u128 = 400;
pub const LINKS_BASE_BYTES: u128 = 300;
pub const MAX_ICON_LEN: usize = 8_000;
pub const MAX_LINKS_LEN: usize = 1_000;

pub fn now() -> u64 {
    env::block_timestamp() / 1_000_000_000
}

#[near(serializers = [borsh])]
#[derive(BorshStorageKey)]
enum StorageKey {
    Holders,
    Ballots,
    Lore,
}

/// A registered account. `since` is the balance-weighted average time the
/// account's tokens arrived, so `balance * (t - since)` is its token-seconds.
#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Holder {
    pub balance: u128,
    pub since: u64,
}

#[near(serializers = [json])]
pub struct LaunchConfig {
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub links: Option<String>,
    pub total_supply: U128,
    /// Creator tokens, locked in the contract and vesting while the coin is alive.
    pub dev_allocation: U128,
    /// Who receives the liquid supply (for example the account that seeds the pool).
    pub distributor: AccountId,
    pub pulse_interval_secs: u64,
    pub vote_duration_secs: u64,
    /// Yes weight needed, in basis points of all eligible weight.
    pub quorum_bps: u16,
    pub vest_duration_secs: u64,
    /// Most NEAR (yocto) the keeper can spend from the treasury per period.
    pub spend_cap: U128,
    pub spend_period_secs: u64,
    /// NEAR (yocto) a late keeper must pay into the treasury to come back.
    pub comeback_penalty: U128,
    /// Account suffix every steward must have, e.g. ".sputnik-dao.near".
    pub dao_suffix: String,
    /// Set only by the Revive factory that created this account, to name the
    /// real creator (otherwise the caller of `new` is the creator).
    pub creator: Option<AccountId>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Config {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub dev_allocation: U128,
    pub pulse_interval_secs: u64,
    pub vote_duration_secs: u64,
    pub quorum_bps: u16,
    pub vest_duration_secs: u64,
    pub spend_cap: U128,
    pub spend_period_secs: u64,
    pub comeback_penalty: U128,
    pub dao_suffix: String,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Takeover {
    pub id: u32,
    pub steward: AccountId,
    pub proposer: AccountId,
    pub started_at: u64,
    pub ends_at: u64,
    pub flatline_at: u64,
    pub yes: U128,
    pub no: U128,
    pub total_eligible: U128,
    pub voters: u32,
    /// NEAR held while the vote is open (see TAKEOVER_BOND).
    pub bond: U128,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Ballot {
    pub support: bool,
    pub weight: U128,
}

/// One line of the coin's on-chain history.
#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Lore {
    pub at: u64,
    pub kind: String,
    pub actor: AccountId,
    pub note: String,
}

#[near(serializers = [json])]
pub struct StatusView {
    pub status: String,
    pub now: u64,
    pub keeper: AccountId,
    pub creator: AccountId,
    pub lives: u32,
    pub launched_at: u64,
    pub last_pulse: u64,
    pub flatline_at: u64,
    pub total_supply: U128,
    pub circulating: U128,
    pub treasury: U128,
    pub spent_in_period: U128,
    pub period_resets_at: u64,
    pub dev_locked: U128,
    pub dev_claimed: U128,
    pub dev_burned: U128,
    pub takeover: Option<Takeover>,
    pub takeover_count: u32,
    pub config: Config,
    pub icon: Option<String>,
    pub links: Option<String>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Revive {
    cfg: Config,
    icon: Option<String>,
    links: Option<String>,
    total_supply: u128,
    circulating: u128,
    holders: LookupMap<AccountId, Holder>,
    /// Sum over holders of balance * since. Lets the contract total everyone's
    /// token-seconds without looping: sum(balance * (t - since)) = circulating * t - this.
    sum_bal_since: u128,
    /// Sum over holders whose tokens arrived after the flatline of balance * (since - flatline).
    /// The running sum above counts those as negative weight; adding this back makes the
    /// eligible total exact. It can only be non-zero while the coin is dead, and resets to 0
    /// whenever the coin gets a new heartbeat (every since is then before the new deadline).
    late_excess: u128,
    creator: AccountId,
    keeper: AccountId,
    launched_at: u64,
    last_pulse: u64,
    dev_claimed: u128,
    dev_burned: u128,
    dev_active: bool,
    treasury: u128,
    period_start: u64,
    spent_in_period: u128,
    takeover: Option<Takeover>,
    takeover_count: u32,
    ballots: LookupMap<(u32, AccountId), Ballot>,
    lives: u32,
    lore: Vector<Lore>,
}

pub(crate) fn emit(standard: &str, version: &str, event: &str, data: serde_json::Value) {
    log!(
        "EVENT_JSON:{}",
        json!({ "standard": standard, "version": version, "event": event, "data": [data] })
    );
}

fn revive_event(event: &str, data: serde_json::Value) {
    emit("revive", "1.0.0", event, data);
}

#[near]
impl Revive {
    #[init]
    #[payable]
    pub fn new(config: LaunchConfig) -> Self {
        let supply = config.total_supply.0;
        let dev = config.dev_allocation.0;
        require!(supply > 0 && supply <= MAX_SUPPLY, "supply must be 1..=1e9 tokens");
        require!(dev * 10_000 <= supply * MAX_DEV_BPS, "creator can keep at most 10% of supply");
        require!(config.pulse_interval_secs >= 60 && config.pulse_interval_secs <= MAX_PERIOD_SECS, "pulse interval must be 60s to 30 days");
        require!(config.vote_duration_secs >= 60 && config.vote_duration_secs <= MAX_PERIOD_SECS, "vote must last 60s to 30 days");
        require!(config.quorum_bps >= 1 && config.quorum_bps <= 10_000, "quorum must be 1..=10000 bps");
        require!(config.spend_period_secs >= 60, "spend period must be at least 60s");
        require!(
            config.vest_duration_secs >= MIN_VEST_PULSES * config.pulse_interval_secs,
            "creator tokens must vest over at least 4 heartbeats"
        );
        require!(config.comeback_penalty.0 >= MIN_PENALTY, "comeback penalty must be at least 0.01 NEAR");
        require!(config.dao_suffix.starts_with('.') && config.dao_suffix.len() > 1, "dao_suffix must start with '.'");
        require!(config.symbol.len() <= 16 && !config.symbol.is_empty(), "symbol must be 1..=16 chars");
        require!(!config.name.is_empty() && config.name.len() <= 64, "name must be 1..=64 chars");
        require!(config.icon.as_ref().is_none_or(|i| i.len() <= MAX_ICON_LEN), "icon too large (8000 chars max)");
        require!(config.links.as_ref().is_none_or(|l| l.len() <= MAX_LINKS_LEN), "links too long");

        let t = now();
        let caller = env::predecessor_account_id();
        let creator = match config.creator {
            Some(c) => {
                let parent_suffix = format!(".{}", caller);
                require!(env::current_account_id().as_str().ends_with(&parent_suffix), "only the parent factory can name the creator");
                c
            }
            None => caller,
        };
        let distributor = if config.distributor == env::current_account_id() { creator.clone() } else { config.distributor };
        let liquid = supply - dev;
        let mut holders = LookupMap::new(StorageKey::Holders);
        holders.insert(distributor.clone(), Holder { balance: liquid, since: t });
        if creator != distributor {
            holders.insert(creator.clone(), Holder { balance: 0, since: t });
        }

        let mut this = Self {
            cfg: Config {
                name: config.name,
                symbol: config.symbol,
                decimals: 18,
                dev_allocation: config.dev_allocation,
                pulse_interval_secs: config.pulse_interval_secs,
                vote_duration_secs: config.vote_duration_secs,
                quorum_bps: config.quorum_bps,
                vest_duration_secs: config.vest_duration_secs,
                spend_cap: config.spend_cap,
                spend_period_secs: config.spend_period_secs,
                comeback_penalty: config.comeback_penalty,
                dao_suffix: config.dao_suffix,
            },
            icon: config.icon,
            links: config.links,
            total_supply: supply,
            circulating: liquid,
            holders,
            sum_bal_since: liquid * t as u128,
            late_excess: 0,
            creator: creator.clone(),
            keeper: creator.clone(),
            launched_at: t,
            last_pulse: t,
            dev_claimed: 0,
            dev_burned: 0,
            dev_active: true,
            treasury: env::attached_deposit().as_yoctonear(),
            period_start: t,
            spent_in_period: 0,
            takeover: None,
            takeover_count: 0,
            ballots: LookupMap::new(StorageKey::Ballots),
            lives: 0,
            lore: Vector::new(StorageKey::Lore),
        };

        emit("nep141", "1.0.0", "ft_mint", json!({ "owner_id": distributor, "amount": liquid.to_string(), "memo": "launch" }));
        if dev > 0 {
            emit("nep141", "1.0.0", "ft_mint", json!({ "owner_id": env::current_account_id(), "amount": dev.to_string(), "memo": "creator allocation, locked" }));
        }
        this.push_lore("born", &creator, format!("{} launched by {}", this.cfg.symbol, creator));
        revive_event("born", json!({ "creator": creator, "pulse_interval_secs": this.cfg.pulse_interval_secs }));
        this
    }

    // ---------------------------------------------------------------- keeper

    /// The keeper's heartbeat. On time it is free. Late (after the flatline)
    /// it is a comeback: the keeper must attach `comeback_penalty`, which goes
    /// to the treasury, and any open takeover vote is cancelled.
    #[payable]
    pub fn pulse(&mut self) {
        require!(env::predecessor_account_id() == self.keeper, "only the keeper has a pulse");
        let t = now();
        let deposit = env::attached_deposit().as_yoctonear();
        if t > self.flatline_at() {
            if let Some(tk) = &self.takeover {
                require!(t < tk.ends_at, "the vote has closed; call finalize_takeover first");
            }
            require!(deposit >= self.cfg.comeback_penalty.0, "late pulse: attach the comeback penalty");
            let cancelled = self.takeover.take();
            let note = match &cancelled {
                Some(tk) => format!("came back {}s late and paid {} yocto; takeover #{} cancelled", t - self.flatline_at(), deposit, tk.id),
                None => format!("came back {}s late and paid {} yocto", t - self.flatline_at(), deposit),
            };
            if let Some(tk) = &cancelled {
                // The proposer acted in good faith: their bond goes back.
                let _ = Promise::new(tk.proposer.clone()).transfer(NearToken::from_yoctonear(tk.bond.0));
            }
            let keeper = self.keeper.clone();
            self.push_lore("comeback", &keeper, note);
            revive_event("comeback", json!({ "keeper": keeper, "penalty": deposit.to_string(), "cancelled_takeover": cancelled.map(|tk| tk.id) }));
        }
        self.treasury += deposit;
        self.last_pulse = t;
        self.late_excess = 0;
        revive_event("pulse", json!({ "keeper": self.keeper, "next_deadline": self.flatline_at() }));
    }

    /// Creator only, while the creator is still keeper and the coin is alive.
    pub fn claim_vested(&mut self) -> U128 {
        let caller = env::predecessor_account_id();
        require!(caller == self.creator && self.keeper == self.creator && self.dev_active, "only the living creator can claim");
        self.assert_alive();
        let claimable = self.dev_vested() - self.dev_claimed;
        require!(claimable > 0, "nothing vested yet");
        self.dev_claimed += claimable;
        self.internal_credit(&caller, claimable, now());
        emit("nep141", "1.0.0", "ft_transfer", json!({ "old_owner_id": env::current_account_id(), "new_owner_id": caller, "amount": claimable.to_string(), "memo": "vested" }));
        U128(claimable)
    }

    /// Anyone can add NEAR to the treasury.
    #[payable]
    pub fn fund_treasury(&mut self) {
        let amount = env::attached_deposit().as_yoctonear();
        require!(amount > 0, "attach NEAR to fund the treasury");
        self.treasury += amount;
        revive_event("treasury_funded", json!({ "by": env::predecessor_account_id(), "amount": amount.to_string() }));
    }

    /// Keeper spends treasury NEAR, within the per-period cap, while alive.
    pub fn spend(&mut self, receiver_id: AccountId, amount: U128, memo: String) -> Promise {
        require!(env::predecessor_account_id() == self.keeper, "only the keeper can spend");
        self.assert_alive();
        require!(memo.len() <= 200, "memo too long");
        let t = now();
        if t >= self.period_start + self.cfg.spend_period_secs {
            self.period_start = t;
            self.spent_in_period = 0;
        }
        require!(amount.0 > 0 && amount.0 <= self.treasury, "not enough in the treasury");
        require!(self.spent_in_period + amount.0 <= self.cfg.spend_cap.0, "over the spending cap for this period");
        self.treasury -= amount.0;
        self.spent_in_period += amount.0;
        let keeper = self.keeper.clone();
        self.push_lore("spend", &keeper, format!("{} yocto to {}: {}", amount.0, receiver_id, memo));
        self.assert_storage_covered(amount.0);
        revive_event("spend", json!({ "keeper": keeper, "receiver_id": receiver_id, "amount": amount, "memo": memo }));
        Promise::new(receiver_id).transfer(NearToken::from_yoctonear(amount.0))
    }

    /// Keeper updates the coin's icon and links (website, X, Telegram).
    /// Attach NEAR for any extra storage; the rest is refunded.
    #[payable]
    pub fn set_links(&mut self, icon: Option<String>, links: Option<String>) {
        require!(env::predecessor_account_id() == self.keeper, "only the keeper can change the links");
        self.assert_alive();
        require!(links.as_ref().is_none_or(|l| l.len() <= MAX_LINKS_LEN), "links too long");
        require!(icon.as_ref().is_none_or(|i| i.len() <= MAX_ICON_LEN), "icon too large (8000 chars max)");
                self.icon = icon;
        self.links = links;
        let keeper = self.keeper.clone();
        self.push_lore("links", &keeper, "updated icon and links".to_string());
        self.charge_storage(LINKS_BASE_BYTES + self.icon.as_ref().map_or(0, |i| i.len() as u128) + self.links.as_ref().map_or(0, |l| l.len() as u128), 0);
    }

    // -------------------------------------------------------------- takeover

    /// After a flatline, a holder from before the flatline nominates a
    /// Sputnik DAO to become the keeper, which opens a vote. Attach the 0.1 NEAR
    /// bond plus a little for storage; the rest is refunded.
    #[payable]
    pub fn start_takeover(&mut self, steward: AccountId) -> Takeover {
                let t = now();
        let caller = env::predecessor_account_id();
        require!(self.takeover.is_none(), "a takeover vote is already open");
        require!(t > self.flatline_at(), "the coin still has a pulse");
        require!(caller != self.keeper && caller != self.creator, "the keeper and creator cannot start a takeover");
        let f = self.flatline_at();
        require!(self.weight_at(&caller, f) > 0, "only holders from before the flatline can start a takeover");
        require!(steward.as_str().ends_with(&self.cfg.dao_suffix), "the steward must be a Sputnik DAO");
        require!(steward != self.creator && steward != self.keeper, "the steward must be a new account");

        self.takeover_count += 1;
        let tk = Takeover {
            id: self.takeover_count,
            steward: steward.clone(),
            proposer: caller.clone(),
            started_at: t,
            ends_at: t + self.cfg.vote_duration_secs,
            flatline_at: f,
            yes: U128(0),
            no: U128(0),
            total_eligible: U128(self.eligible_total(f)),
            voters: 0,
            bond: U128(TAKEOVER_BOND),
        };
        self.takeover = Some(tk.clone());
        self.push_lore("flatline", &self.keeper.clone(), format!("no pulse since {}; flatlined at {}", self.last_pulse, f));
        self.push_lore("takeover", &caller, format!("takeover #{} opened: {} nominated as keeper", tk.id, steward));
        revive_event("takeover_started", json!({ "id": tk.id, "steward": steward, "proposer": caller, "ends_at": tk.ends_at, "flatline_at": f }));
        self.charge_storage(TAKEOVER_BYTES, TAKEOVER_BOND);
        tk
    }

    /// Vote with your token-seconds from before the flatline. Attach a little
    /// NEAR (0.01 is plenty) for the ballot's storage; the rest is refunded.
    #[payable]
    pub fn vote(&mut self, support: bool) -> Ballot {
                let caller = env::predecessor_account_id();
        let mut tk = self.takeover.clone().expect("no takeover vote is open");
        require!(now() < tk.ends_at, "the vote has closed");
        require!(caller != self.keeper && caller != self.creator, "the keeper and creator cannot vote");
        let key = (tk.id, caller.clone());
        require!(!self.ballots.contains_key(&key), "already voted");
        let weight = self.weight_at(&caller, tk.flatline_at);
        require!(weight > 0, "no voting weight: only tokens held before the flatline count");
        if support {
            tk.yes = U128(tk.yes.0 + weight);
        } else {
            tk.no = U128(tk.no.0 + weight);
        }
        tk.voters += 1;
        let ballot = Ballot { support, weight: U128(weight) };
        self.ballots.insert(key, ballot.clone());
        revive_event("vote", json!({ "id": tk.id, "voter": caller, "support": support, "weight": weight.to_string() }));
        self.takeover = Some(tk);
        self.charge_storage(VOTE_BYTES, 0);
        ballot
    }

    /// Anyone closes the vote once it has ended. Returns true if the coin revived.
    /// Attach a little NEAR (0.01 is plenty) for the history entry; the rest is refunded.
    #[payable]
    pub fn finalize_takeover(&mut self) -> bool {
                let tk = self.takeover.clone().expect("no takeover vote is open");
        let t = now();
        require!(t >= tk.ends_at, "the vote is still open");
        self.takeover = None;
        let quorum_met = tk.yes.0 * 10_000 >= tk.total_eligible.0 * self.cfg.quorum_bps as u128;
        let passed = tk.yes.0 > tk.no.0 && quorum_met;
        let caller = env::predecessor_account_id();
        if !passed {
            let why = if tk.yes.0 <= tk.no.0 { "holders voted no" } else { "quorum not reached" };
            self.push_lore("takeover_failed", &caller, format!("takeover #{} failed: {}", tk.id, why));
            revive_event("takeover_failed", json!({ "id": tk.id, "reason": why }));
            self.treasury += tk.bond.0; // a failed takeover's bond stays with the coin
            self.charge_storage(FINALIZE_BYTES, 0);
            return false;
        }
        let _ = Promise::new(tk.proposer.clone()).transfer(NearToken::from_yoctonear(tk.bond.0));

        let old_keeper = self.keeper.clone();
        self.keeper = tk.steward.clone();
        let mut burned = 0;
        if self.dev_active {
            burned = self.cfg.dev_allocation.0 - self.dev_claimed;
            self.dev_active = false;
            self.dev_burned = burned;
            self.total_supply -= burned;
            if burned > 0 {
                emit("nep141", "1.0.0", "ft_burn", json!({ "owner_id": env::current_account_id(), "amount": burned.to_string(), "memo": "creator allocation burned on revival" }));
            }
        }
        self.last_pulse = t;
        self.late_excess = 0;
        self.period_start = t;
        self.spent_in_period = 0;
        self.lives += 1;
        self.push_lore(
            "revived",
            &tk.steward,
            format!("life #{}: keeper {} replaced by {}; {} creator tokens burned", self.lives, old_keeper, tk.steward, burned),
        );
        revive_event("revived", json!({ "id": tk.id, "old_keeper": old_keeper, "new_keeper": tk.steward, "lives": self.lives, "burned": burned.to_string() }));
        self.charge_storage(FINALIZE_BYTES, 0);
        true
    }

    // ----------------------------------------------------------------- views

    pub fn get_status(&self) -> StatusView {
        StatusView {
            status: self.status(),
            now: now(),
            keeper: self.keeper.clone(),
            creator: self.creator.clone(),
            lives: self.lives,
            launched_at: self.launched_at,
            last_pulse: self.last_pulse,
            flatline_at: self.flatline_at(),
            total_supply: U128(self.total_supply),
            circulating: U128(self.circulating),
            treasury: U128(self.treasury),
            spent_in_period: U128(self.spent_in_period),
            period_resets_at: self.period_start + self.cfg.spend_period_secs,
            dev_locked: U128(self.dev_locked()),
            dev_claimed: U128(self.dev_claimed),
            dev_burned: U128(self.dev_burned),
            takeover: self.takeover.clone(),
            takeover_count: self.takeover_count,
            config: self.cfg.clone(),
            icon: self.icon.clone(),
            links: self.links.clone(),
        }
    }

    /// Weight the account would vote with now (against the open vote's
    /// flatline, or the coming flatline if no vote is open).
    pub fn get_vote_weight(&self, account_id: AccountId) -> U128 {
        let f = self.takeover.as_ref().map(|tk| tk.flatline_at).unwrap_or_else(|| self.flatline_at());
        if account_id == self.keeper || account_id == self.creator {
            return U128(0);
        }
        U128(self.weight_at(&account_id, f))
    }

    pub fn get_ballot(&self, account_id: AccountId) -> Option<Ballot> {
        let tk = self.takeover.as_ref()?;
        self.ballots.get(&(tk.id, account_id)).cloned()
    }

    pub fn get_lore(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<Lore> {
        let from = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(50).min(100);
        (from..self.lore.len().min(from.saturating_add(limit)))
            .filter_map(|i| self.lore.get(i).cloned())
            .collect()
    }

    pub fn get_lore_count(&self) -> u32 {
        self.lore.len()
    }
}

// ------------------------------------------------------------------ internal

impl Revive {
    pub(crate) fn flatline_at(&self) -> u64 {
        self.last_pulse + self.cfg.pulse_interval_secs
    }

    pub(crate) fn status(&self) -> String {
        let t = now();
        match &self.takeover {
            Some(tk) if t >= tk.ends_at => "vote_closed",
            Some(_) => "voting",
            None if t > self.flatline_at() => "flatlined",
            None => "alive",
        }
        .to_string()
    }

    fn assert_alive(&self) {
        require!(self.takeover.is_none() && now() <= self.flatline_at(), "the coin has flatlined");
    }

    fn dev_vested(&self) -> u128 {
        let total = self.cfg.dev_allocation.0;
        if self.cfg.vest_duration_secs == 0 {
            return total;
        }
        let elapsed = (now() - self.launched_at).min(self.cfg.vest_duration_secs) as u128;
        total * elapsed / self.cfg.vest_duration_secs as u128
    }

    fn dev_locked(&self) -> u128 {
        if self.dev_active {
            self.cfg.dev_allocation.0 - self.dev_claimed
        } else {
            0
        }
    }

    /// Token-seconds the account held before `f`, scaled. Tokens that arrive
    /// after `f` pull `since` forward, so bought-after-death tokens add nothing
    /// and dilute the buyer's older tokens instead.
    pub(crate) fn weight_at(&self, account: &AccountId, f: u64) -> u128 {
        match self.holders.get(account) {
            Some(h) if h.since < f => h.balance * (f - h.since) as u128 / WEIGHT_SCALE,
            _ => 0,
        }
    }

    /// Everyone's token-seconds before `f`, minus the keeper's and creator's.
    /// Uses the running sum, so it costs the same with 10 holders or 10 million.
    fn eligible_total(&self, f: u64) -> u128 {
        let all = (self.circulating * f as u128 + self.late_excess).saturating_sub(self.sum_bal_since) / WEIGHT_SCALE;
        let mut excluded = self.weight_at(&self.keeper, f);
        if self.creator != self.keeper {
            excluded += self.weight_at(&self.creator, f);
        }
        all.saturating_sub(excluded)
    }

    /// How far a holding sits after the flatline, in token-seconds (0 if it arrived before).
    pub(crate) fn late_part(&self, balance: u128, since: u64) -> u128 {
        let f = self.flatline_at();
        if since > f { balance * (since - f) as u128 } else { 0 }
    }

    fn push_lore(&mut self, kind: &str, actor: &AccountId, note: String) {
        self.lore.push(Lore { at: now(), kind: kind.to_string(), actor: actor.clone(), note });
    }

    /// The caller pays for `bytes` of storage plus `extra` (a bond) out of the attached
    /// deposit; whatever is left goes back to them.
    pub(crate) fn charge_storage(&self, bytes: u128, extra: u128) {
        let cost = bytes * env::storage_byte_cost().as_yoctonear();
        let deposit = env::attached_deposit().as_yoctonear();
        require!(
            deposit >= cost + extra,
            format!("attach at least {} yoctoNEAR ({} for storage, {} bond)", cost + extra, cost, extra)
        );
        let refund = deposit - cost - extra;
        if refund > 0 {
            let _ = Promise::new(env::predecessor_account_id()).transfer(NearToken::from_yoctonear(refund));
        }
    }

    /// Sending `amount` must leave enough NEAR to pay for the account's storage.
    pub(crate) fn assert_storage_covered(&self, amount: u128) {
        let locked = env::storage_usage() as u128 * env::storage_byte_cost().as_yoctonear();
        require!(
            env::account_balance().as_yoctonear() >= amount + locked,
            "the treasury NEAR is needed to pay for the coin's storage"
        );
    }
}
