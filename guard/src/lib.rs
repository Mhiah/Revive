//! Revive guard: an onchain community takeover for a token that already exists.
//!
//! The token (any NEP-141, e.g. launched on meme.cooking) is not changed. The
//! guard holds what can be enforced: the creator's own tokens ("the bag"), a NEAR
//! treasury, and holders' staked tokens.
//!
//! * The creator locks their bag here with `ft_transfer_call(msg = "lock")`; it
//!   vests to them only while they keep pulsing.
//! * Holders stake with `ft_transfer_call(msg = "stake")`. Vote weight is staked
//!   tokens times seconds staked before the flatline, so staking after the death
//!   buys no say. Holders can unstake at any time.
//! * If the keeper misses a pulse, holders vote a Sputnik DAO in as keeper. A
//!   passed vote seals the creator's unvested bag in the guard forever and hands
//!   the treasury to the DAO.

use near_sdk::json_types::U128;
use near_sdk::serde_json::{self, json};
use near_sdk::store::{LookupMap, Vector};
use near_sdk::{
    env, ext_contract, log, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise, PromiseError, PromiseOrValue,
};

#[cfg(test)]
mod tests;

const WEIGHT_SCALE: u128 = 1_000_000_000_000;
/// Supply cap used to keep weight math in u128: 1e30 base units.
/// Weight maths runs in "weight units" of 10^(decimals - 12) base units, so a
/// 24-decimal token with a trillion supply still fits in u128 (see `w`). The sum
/// of everyone's weight units is capped here: 1e28 * a timestamp stays below 2^128.
const MAX_WEIGHT_UNITS: u128 = 10_000_000_000_000_000_000_000_000_000;
pub const MAX_PERIOD_SECS: u64 = 30 * 86_400;
pub const MIN_VEST_PULSES: u64 = 4;
pub const MIN_PENALTY: u128 = 10_000_000_000_000_000_000_000; // 0.01 NEAR
pub const TAKEOVER_BOND: u128 = 100_000_000_000_000_000_000_000; // 0.1 NEAR
/// Bytes each call pays for (generous; anything unused stays as storage headroom).
/// Writes only land after a call returns, so these are fixed rather than measured.
pub const VOTE_BYTES: u128 = 250;
pub const TAKEOVER_BYTES: u128 = 900;
pub const FINALIZE_BYTES: u128 = 400;
pub const LINKS_BASE_BYTES: u128 = 300;
/// What a staker pays once (storage_deposit) for their entry in the guard.
pub const STAKER_STORAGE: u128 = 2_000_000_000_000_000_000_000; // 0.002 NEAR
const GAS_FT_TRANSFER: Gas = Gas::from_tgas(10);
const GAS_CALLBACK: Gas = Gas::from_tgas(10);

pub fn now() -> u64 {
    env::block_timestamp() / 1_000_000_000
}

#[allow(dead_code)]
#[ext_contract(ext_ft)]
pub trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[near(serializers = [borsh])]
#[derive(BorshStorageKey)]
enum StorageKey {
    Stakers,
    Ballots,
    Lore,
}

#[near(serializers = [json])]
pub struct StorageBalance {
    pub total: U128,
    pub available: U128,
}

#[near(serializers = [json])]
pub struct StorageBalanceBounds {
    pub min: U128,
    pub max: Option<U128>,
}

#[near(serializers = [borsh])]
#[derive(Clone)]
pub struct Stake {
    pub balance: u128,
    pub since: u64,
}

#[near(serializers = [json])]
pub struct ConnectConfig {
    /// The NEP-141 token this guard protects.
    pub token_id: AccountId,
    /// Filled in by the factory from the token's ft_metadata.
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub links: Option<String>,
    pub pulse_interval_secs: u64,
    pub vote_duration_secs: u64,
    pub quorum_bps: u16,
    pub vest_duration_secs: u64,
    pub spend_cap: U128,
    pub spend_period_secs: u64,
    pub comeback_penalty: U128,
    pub dao_suffix: String,
    /// Set only by the parent factory, naming the real creator.
    pub creator: Option<AccountId>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Config {
    pub token_id: AccountId,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
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
    pub bond: U128,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Ballot {
    pub support: bool,
    pub weight: U128,
}

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
    pub kind: String,
    pub status: String,
    pub now: u64,
    pub token_id: AccountId,
    pub keeper: AccountId,
    pub creator: AccountId,
    pub lives: u32,
    pub launched_at: u64,
    pub last_pulse: u64,
    pub flatline_at: u64,
    pub staked: U128,
    pub stakers: u32,
    pub treasury: U128,
    pub spent_in_period: U128,
    pub period_resets_at: u64,
    /// Creator tokens still locked in the guard (vesting).
    pub dev_locked: U128,
    pub dev_claimed: U128,
    /// Creator tokens sealed forever by a revival ("burned" from circulation).
    pub dev_burned: U128,
    pub takeover: Option<Takeover>,
    pub takeover_count: u32,
    pub config: Config,
    pub links: Option<String>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Guard {
    cfg: Config,
    links: Option<String>,
    creator: AccountId,
    keeper: AccountId,
    launched_at: u64,
    last_pulse: u64,
    bag_total: u128,
    bag_claimed: u128,
    bag_sealed: u128,
    vest_start: u64,
    /// Bag tokens already vested when the vesting schedule last restarted (a later lock
    /// restarts it, so newly locked tokens vest from the moment they arrive).
    vest_carry: u128,
    dev_active: bool,
    /// One weight unit in base units: 10^(decimals - 12), at least 1.
    unit: u128,
    /// Sum of every staker's weight units (see `w`).
    w_total: u128,
    stakers: LookupMap<AccountId, Stake>,
    staker_count: u32,
    staked_total: u128,
    /// Sum over stakers of w(balance) * since.
    sum_bal_since: u128,
    /// Stake that arrived after the flatline, as stake * (since - flatline). Added back so the
    /// eligible total is exact; resets to 0 on every new heartbeat (see the coin contract).
    late_excess: u128,
    treasury: u128,
    period_start: u64,
    spent_in_period: u128,
    takeover: Option<Takeover>,
    takeover_count: u32,
    ballots: LookupMap<(u32, AccountId), Ballot>,
    lives: u32,
    lore: Vector<Lore>,
}

fn emit(event: &str, data: serde_json::Value) {
    log!("EVENT_JSON:{}", json!({ "standard": "revive", "version": "1.0.0", "event": event, "data": [data] }));
}

#[near]
impl Guard {
    #[init]
    #[payable]
    pub fn new(config: ConnectConfig) -> Self {
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
        require!(config.decimals <= 32, "decimals must be at most 32");
        require!(config.links.as_ref().is_none_or(|l| l.len() <= 1_000), "links too long");
        let caller = env::predecessor_account_id();
        let creator = match config.creator {
            Some(c) => {
                require!(env::current_account_id().as_str().ends_with(&format!(".{}", caller)), "only the parent factory can name the creator");
                c
            }
            None => caller,
        };
        require!(config.token_id != env::current_account_id(), "a guard cannot guard itself");
        let t = now();
        let unit = 10u128.pow(config.decimals.saturating_sub(12) as u32);
        // Names and symbols come from the token's own metadata: keep them a sane size.
        let name: String = config.name.chars().take(64).collect();
        let symbol: String = config.symbol.chars().take(16).collect();
        let mut this = Self {
            cfg: Config {
                token_id: config.token_id,
                name,
                symbol,
                decimals: config.decimals,
                pulse_interval_secs: config.pulse_interval_secs,
                vote_duration_secs: config.vote_duration_secs,
                quorum_bps: config.quorum_bps,
                vest_duration_secs: config.vest_duration_secs,
                spend_cap: config.spend_cap,
                spend_period_secs: config.spend_period_secs,
                comeback_penalty: config.comeback_penalty,
                dao_suffix: config.dao_suffix,
            },
            links: config.links,
            creator: creator.clone(),
            keeper: creator.clone(),
            launched_at: t,
            last_pulse: t,
            bag_total: 0,
            bag_claimed: 0,
            bag_sealed: 0,
            vest_start: 0,
            vest_carry: 0,
            dev_active: true,
            unit,
            w_total: 0,
            stakers: LookupMap::new(StorageKey::Stakers),
            staker_count: 0,
            staked_total: 0,
            sum_bal_since: 0,
            late_excess: 0,
            treasury: env::attached_deposit().as_yoctonear(),
            period_start: t,
            spent_in_period: 0,
            takeover: None,
            takeover_count: 0,
            ballots: LookupMap::new(StorageKey::Ballots),
            lives: 0,
            lore: Vector::new(StorageKey::Lore),
        };
        let note = format!("{} ({}) connected by {}", this.cfg.symbol, this.cfg.token_id, creator);
        this.push_lore("born", &creator, note);
        emit("connected", json!({ "token_id": this.cfg.token_id, "creator": creator }));
        this
    }

    // ------------------------------------------------------------ tokens in

    /// NEP-141 receiver. `msg = "lock"` from the creator locks their bag;
    /// `msg = "stake"` from anyone else stakes for voting weight.
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128> {
        require!(env::predecessor_account_id() == self.cfg.token_id, "only the guarded token can be sent here");
        require!(amount.0 > 0, "amount must be positive");
        match msg.as_str() {
            "lock" => {
                require!(sender_id == self.creator && self.dev_active && self.keeper == self.creator, "only the living creator locks a bag");
                self.assert_alive();
                require!(self.bag_total.checked_add(amount.0).is_some(), "amount too large");
                // What has vested so far stays vested; everything still locked, old and new,
                // vests over a fresh full period from now. A top-up can't unlock early.
                self.vest_carry = self.vested();
                self.vest_start = now();
                self.bag_total += amount.0;
                self.push_lore("lock", &sender_id, format!("creator locked {} base units", amount.0));
                emit("lock", json!({ "creator": sender_id, "amount": amount }));
            }
            "stake" => {
                require!(sender_id != self.creator && sender_id != self.keeper, "the creator and keeper cannot stake for votes");
                require!(self.stakers.contains_key(&sender_id), "register first: call storage_deposit on the guard");
                require!(self.staked_total.checked_add(amount.0).is_some(), "amount too large");
                self.credit(&sender_id, amount.0, now());
                require!(self.w_total <= MAX_WEIGHT_UNITS, "amount too large");
                emit("stake", json!({ "account_id": sender_id, "amount": amount }));
            }
            _ => env::panic_str("msg must be \"lock\" or \"stake\""),
        }
        PromiseOrValue::Value(U128(0))
    }

    /// Register to stake (NEP-145 style): pays once for your entry in the guard.
    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<AccountId>, registration_only: Option<bool>) -> StorageBalance {
        let _ = registration_only;
        let account = account_id.unwrap_or_else(env::predecessor_account_id);
        let deposit = env::attached_deposit().as_yoctonear();
        let refund = if self.stakers.contains_key(&account) {
            deposit
        } else {
            require!(deposit >= STAKER_STORAGE, "attach at least 0.002 NEAR to register");
            self.stakers.insert(account.clone(), Stake { balance: 0, since: now() });
            self.staker_count += 1;
            deposit - STAKER_STORAGE
        };
        if refund > 0 {
            let _ = Promise::new(env::predecessor_account_id()).transfer(NearToken::from_yoctonear(refund));
        }
        StorageBalance { total: U128(STAKER_STORAGE), available: U128(0) }
    }

    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.stakers.contains_key(&account_id).then_some(StorageBalance { total: U128(STAKER_STORAGE), available: U128(0) })
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds { min: U128(STAKER_STORAGE), max: Some(U128(STAKER_STORAGE)) }
    }

    /// Take staked tokens back. Allowed at any time; tokens staked later carry less weight.
    pub fn unstake(&mut self, amount: U128) -> Promise {
        let account = env::predecessor_account_id();
        let since = self.stakers.get(&account).map(|s| s.since).unwrap_or_else(|| env::panic_str("nothing staked"));
        self.debit(&account, amount.0);
        emit("unstake", json!({ "account_id": account, "amount": amount }));
        ext_ft::ext(self.cfg.token_id.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FT_TRANSFER)
            .ft_transfer(account.clone(), amount, Some("unstake".into()))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_unstake(account, amount, since))
    }

    /// If the token transfer failed, put the stake back as it was.
    #[private]
    pub fn on_unstake(&mut self, account_id: AccountId, amount: U128, since: u64, #[callback_result] r: Result<(), PromiseError>) -> bool {
        if r.is_ok() {
            return true;
        }
        self.credit(&account_id, amount.0, since);
        false
    }

    // ---------------------------------------------------------------- keeper

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
                let _ = Promise::new(tk.proposer.clone()).transfer(NearToken::from_yoctonear(tk.bond.0));
            }
            let keeper = self.keeper.clone();
            self.push_lore("comeback", &keeper, note);
            emit("comeback", json!({ "keeper": keeper, "penalty": deposit.to_string() }));
        }
        self.treasury += deposit;
        self.last_pulse = t;
        self.late_excess = 0;
        emit("pulse", json!({ "keeper": self.keeper, "next_deadline": self.flatline_at() }));
    }

    /// Creator takes what has vested from the bag, while alive and still keeper.
    pub fn claim_vested(&mut self) -> Promise {
        let caller = env::predecessor_account_id();
        require!(caller == self.creator && self.keeper == self.creator && self.dev_active, "only the living creator can claim");
        self.assert_alive();
        let amount = self.vested() - self.bag_claimed;
        require!(amount > 0, "nothing vested yet");
        self.bag_claimed += amount;
        emit("claim", json!({ "creator": caller, "amount": amount.to_string() }));
        ext_ft::ext(self.cfg.token_id.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FT_TRANSFER)
            .ft_transfer(caller, U128(amount), Some("vested".into()))
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_CALLBACK).on_claim(U128(amount)))
    }

    #[private]
    pub fn on_claim(&mut self, amount: U128, #[callback_result] r: Result<(), PromiseError>) -> bool {
        if r.is_err() {
            self.bag_claimed -= amount.0;
            return false;
        }
        true
    }

    #[payable]
    pub fn fund_treasury(&mut self) {
        let amount = env::attached_deposit().as_yoctonear();
        require!(amount > 0, "attach NEAR to fund the treasury");
        self.treasury += amount;
        emit("treasury_funded", json!({ "by": env::predecessor_account_id(), "amount": amount.to_string() }));
    }

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
        Promise::new(receiver_id).transfer(NearToken::from_yoctonear(amount.0))
    }

    #[payable]
    pub fn set_links(&mut self, links: Option<String>) {
        require!(env::predecessor_account_id() == self.keeper, "only the keeper can change the links");
        self.assert_alive();
        if let Some(l) = &links {
            require!(l.len() <= 1_000, "links too long");
        }
                self.links = links;
        self.charge_storage(LINKS_BASE_BYTES + self.links.as_ref().map_or(0, |l| l.len() as u128), 0);
    }

    // -------------------------------------------------------------- takeover

    #[payable]
    pub fn start_takeover(&mut self, steward: AccountId) -> Takeover {
                let t = now();
        let caller = env::predecessor_account_id();
        require!(self.takeover.is_none(), "a takeover vote is already open");
        require!(t > self.flatline_at(), "the coin still has a pulse");
        let f = self.flatline_at();
        require!(self.weight_at(&caller, f) > 0, "only holders who staked before the flatline can start a takeover");
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
        let keeper = self.keeper.clone();
        self.push_lore("flatline", &keeper, format!("no pulse since {}; flatlined at {}", self.last_pulse, f));
        self.push_lore("takeover", &caller, format!("takeover #{} opened: {} nominated as keeper", tk.id, steward));
        emit("takeover_started", json!({ "id": tk.id, "steward": steward, "proposer": caller, "ends_at": tk.ends_at }));
        self.charge_storage(TAKEOVER_BYTES, TAKEOVER_BOND);
        tk
    }

    #[payable]
    pub fn vote(&mut self, support: bool) -> Ballot {
                let caller = env::predecessor_account_id();
        let mut tk = self.takeover.clone().expect("no takeover vote is open");
        require!(now() < tk.ends_at, "the vote has closed");
        let key = (tk.id, caller.clone());
        require!(!self.ballots.contains_key(&key), "already voted");
        let weight = self.weight_at(&caller, tk.flatline_at);
        require!(weight > 0, "no voting weight: only tokens staked before the flatline count");
        if support {
            tk.yes = U128(tk.yes.0 + weight);
        } else {
            tk.no = U128(tk.no.0 + weight);
        }
        tk.voters += 1;
        let ballot = Ballot { support, weight: U128(weight) };
        self.ballots.insert(key, ballot.clone());
        emit("vote", json!({ "id": tk.id, "voter": caller, "support": support, "weight": weight.to_string() }));
        self.takeover = Some(tk);
        self.charge_storage(VOTE_BYTES, 0);
        ballot
    }

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
            emit("takeover_failed", json!({ "id": tk.id, "reason": why }));
            self.treasury += tk.bond.0;
            self.charge_storage(FINALIZE_BYTES, 0);
            return false;
        }
        let _ = Promise::new(tk.proposer.clone()).transfer(NearToken::from_yoctonear(tk.bond.0));
        let old_keeper = self.keeper.clone();
        self.keeper = tk.steward.clone();
        let mut sealed = 0;
        if self.dev_active {
            sealed = self.bag_total - self.bag_claimed;
            self.bag_sealed = sealed;
            self.dev_active = false;
        }
        self.last_pulse = t;
        self.late_excess = 0;
        self.period_start = t;
        self.spent_in_period = 0;
        self.lives += 1;
        self.push_lore(
            "revived",
            &tk.steward,
            format!("life #{}: keeper {} replaced by {}; {} creator base units sealed forever", self.lives, old_keeper, tk.steward, sealed),
        );
        emit("revived", json!({ "id": tk.id, "old_keeper": old_keeper, "new_keeper": tk.steward, "lives": self.lives, "sealed": sealed.to_string() }));
        self.charge_storage(FINALIZE_BYTES, 0);
        true
    }

    // ----------------------------------------------------------------- views

    pub fn get_status(&self) -> StatusView {
        StatusView {
            kind: "guard".into(),
            status: self.status(),
            now: now(),
            token_id: self.cfg.token_id.clone(),
            keeper: self.keeper.clone(),
            creator: self.creator.clone(),
            lives: self.lives,
            launched_at: self.launched_at,
            last_pulse: self.last_pulse,
            flatline_at: self.flatline_at(),
            staked: U128(self.staked_total),
            stakers: self.staker_count,
            treasury: U128(self.treasury),
            spent_in_period: U128(self.spent_in_period),
            period_resets_at: self.period_start + self.cfg.spend_period_secs,
            dev_locked: U128(if self.dev_active { self.bag_total - self.bag_claimed } else { 0 }),
            dev_claimed: U128(self.bag_claimed),
            dev_burned: U128(self.bag_sealed),
            takeover: self.takeover.clone(),
            takeover_count: self.takeover_count,
            config: self.cfg.clone(),
            links: self.links.clone(),
        }
    }

    pub fn get_stake(&self, account_id: AccountId) -> U128 {
        U128(self.stakers.get(&account_id).map(|s| s.balance).unwrap_or(0))
    }

    pub fn get_vote_weight(&self, account_id: AccountId) -> U128 {
        let f = self.takeover.as_ref().map(|tk| tk.flatline_at).unwrap_or_else(|| self.flatline_at());
        U128(self.weight_at(&account_id, f))
    }

    pub fn get_ballot(&self, account_id: AccountId) -> Option<Ballot> {
        let tk = self.takeover.as_ref()?;
        self.ballots.get(&(tk.id, account_id)).cloned()
    }

    pub fn get_lore(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<Lore> {
        let from = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(50).min(100);
        (from..self.lore.len().min(from.saturating_add(limit))).filter_map(|i| self.lore.get(i).cloned()).collect()
    }

    pub fn get_lore_count(&self) -> u32 {
        self.lore.len()
    }
}

impl Guard {
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

    fn vested(&self) -> u128 {
        if self.vest_start == 0 {
            return 0;
        }
        let elapsed = (now() - self.vest_start).min(self.cfg.vest_duration_secs) as u128;
        self.vest_carry + (self.bag_total - self.vest_carry) * elapsed / self.cfg.vest_duration_secs as u128
    }

    /// Weight units for a balance (see `unit`).
    pub(crate) fn w(&self, balance: u128) -> u128 {
        balance / self.unit
    }

    pub(crate) fn weight_at(&self, account: &AccountId, f: u64) -> u128 {
        if account == &self.keeper || account == &self.creator {
            return 0;
        }
        match self.stakers.get(account) {
            Some(s) if s.since < f => self.w(s.balance) * (f - s.since) as u128 / WEIGHT_SCALE,
            _ => 0,
        }
    }

    pub(crate) fn eligible_total(&self, f: u64) -> u128 {
        let all = (self.w_total * f as u128 + self.late_excess).saturating_sub(self.sum_bal_since) / WEIGHT_SCALE;
        // A DAO that staked before it was elected can't vote once it is keeper, so its stake
        // is left out (the creator can never stake, so only the keeper needs this).
        let keeper_part = match self.stakers.get(&self.keeper) {
            Some(s) if s.since < f => self.w(s.balance) * (f - s.since) as u128 / WEIGHT_SCALE,
            _ => 0,
        };
        all.saturating_sub(keeper_part)
    }

    /// Adds `amount` that arrived at `stamp` to the account's stake. All weight sums are in
    /// weight units, and each holding's old contribution is taken out and its new one put
    /// back, so the running sums always equal the per-staker totals exactly.
    pub(crate) fn credit(&mut self, account: &AccountId, amount: u128, stamp: u64) {
        let mut s = self.stakers.get(account).cloned().unwrap_or_else(|| env::panic_str("register first: call storage_deposit on the guard"));
        let (ow, nb) = (self.w(s.balance), s.balance + amount);
        let nw = self.w(nb);
        // Balance-weighted average arrival time, in weight units. Rounded towards the
        // later time when it moves forward and towards less change when it moves back,
        // so rounding can only ever lower a weight.
        let ns = if nw == 0 || ow == 0 {
            if ow == 0 { stamp.max(s.since) } else { s.since }
        } else if stamp >= s.since {
            s.since + ((nw - ow) * (stamp - s.since) as u128).div_ceil(nw) as u64
        } else {
            s.since - ((nw - ow) * (s.since - stamp) as u128 / nw) as u64
        };
        self.sum_bal_since = self.sum_bal_since - ow * s.since as u128 + nw * ns as u128;
        self.late_excess = self.late_excess - self.late_part(ow, s.since) + self.late_part(nw, ns);
        self.w_total = self.w_total - ow + nw;
        s.balance = nb;
        s.since = ns;
        self.staked_total += amount;
        self.stakers.insert(account.clone(), s);
    }

    pub(crate) fn debit(&mut self, account: &AccountId, amount: u128) {
        let mut s = self.stakers.get(account).cloned().unwrap_or_else(|| env::panic_str("nothing staked"));
        require!(amount > 0 && s.balance >= amount, "not enough staked");
        let (ow, nw) = (self.w(s.balance), self.w(s.balance - amount));
        self.sum_bal_since = self.sum_bal_since - ow * s.since as u128 + nw * s.since as u128;
        self.late_excess = self.late_excess - self.late_part(ow, s.since) + self.late_part(nw, s.since);
        self.w_total = self.w_total - ow + nw;
        s.balance -= amount;
        self.staked_total -= amount;
        self.stakers.insert(account.clone(), s);
    }

    /// `w` weight units held since `since`, past the flatline, in unit-seconds (0 if before it).
    pub(crate) fn late_part(&self, w: u128, since: u64) -> u128 {
        let f = self.flatline_at();
        if since > f { w * (since - f) as u128 } else { 0 }
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

    pub(crate) fn assert_storage_covered(&self, amount: u128) {
        let locked = env::storage_usage() as u128 * env::storage_byte_cost().as_yoctonear();
        require!(
            env::account_balance().as_yoctonear() >= amount + locked,
            "the treasury NEAR is needed to pay for the guard's storage"
        );
    }

    fn push_lore(&mut self, kind: &str, actor: &AccountId, note: String) {
        self.lore.push(Lore { at: now(), kind: kind.to_string(), actor: actor.clone(), note });
    }
}
