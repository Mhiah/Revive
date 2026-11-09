//! NEP-141 fungible token, NEP-145 storage and NEP-148 metadata.
//! Every balance change also moves the holder's `since`, which is what makes
//! vote weight depend on how long tokens were held.

use crate::*;
use near_sdk::{assert_one_yocto, ext_contract, Gas, PromiseError, PromiseOrValue};

/// Flat storage fee per account, refundable on unregister.
pub const STORAGE_COST: u128 = 1_250_000_000_000_000_000_000; // 0.00125 NEAR
const GAS_FOR_FT_ON_TRANSFER: Gas = Gas::from_tgas(30);
const GAS_FOR_RESOLVE: Gas = Gas::from_tgas(10);

#[allow(dead_code)]
#[ext_contract(ext_ft_receiver)]
pub trait FungibleTokenReceiver {
    fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> PromiseOrValue<U128>;
}

#[near(serializers = [json])]
pub struct FungibleTokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<String>,
    pub decimals: u8,
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

#[near]
impl Revive {
    #[payable]
    pub fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        self.internal_transfer(&sender_id, &receiver_id, amount.0, false);
        transfer_event(&sender_id, &receiver_id, amount.0, memo.as_deref());
    }

    #[payable]
    pub fn ft_transfer_call(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>, msg: String) -> PromiseOrValue<U128> {
        assert_one_yocto();
        require!(env::prepaid_gas() > GAS_FOR_FT_ON_TRANSFER.saturating_add(GAS_FOR_RESOLVE), "more gas is required");
        let sender_id = env::predecessor_account_id();
        self.internal_transfer(&sender_id, &receiver_id, amount.0, false);
        transfer_event(&sender_id, &receiver_id, amount.0, memo.as_deref());
        ext_ft_receiver::ext(receiver_id.clone())
            .with_static_gas(GAS_FOR_FT_ON_TRANSFER)
            .ft_on_transfer(sender_id.clone(), amount, msg)
            .then(Self::ext(env::current_account_id()).with_static_gas(GAS_FOR_RESOLVE).ft_resolve_transfer(sender_id, receiver_id, amount))
            .into()
    }

    /// Returns the amount the receiver kept. Refunded tokens go back to the
    /// sender with the sender's original `since`, so a DEX refund does not
    /// cost anyone hold time.
    #[private]
    pub fn ft_resolve_transfer(&mut self, sender_id: AccountId, receiver_id: AccountId, amount: U128, #[callback_result] used: Result<U128, PromiseError>) -> U128 {
        let unused = match used {
            Ok(u) => u.0.min(amount.0),
            Err(_) => amount.0,
        };
        if unused == 0 {
            return amount;
        }
        let receiver_balance = self.holders.get(&receiver_id).map(|h| h.balance).unwrap_or(0);
        let refund = unused.min(receiver_balance);
        if refund == 0 {
            return amount;
        }
        if self.holders.contains_key(&sender_id) {
            self.internal_transfer(&receiver_id, &sender_id, refund, true);
            transfer_event(&receiver_id, &sender_id, refund, Some("refund"));
        } else {
            // The sender unregistered in the meantime: burn instead.
            self.internal_debit(&receiver_id, refund);
            self.total_supply -= refund;
            emit("nep141", "1.0.0", "ft_burn", json!({ "owner_id": receiver_id, "amount": refund.to_string(), "memo": "refund to unregistered sender" }));
        }
        U128(amount.0 - refund)
    }

    pub fn ft_total_supply(&self) -> U128 {
        U128(self.total_supply)
    }

    /// The contract's own balance is the creator's locked allocation.
    pub fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        if account_id == env::current_account_id() {
            return U128(self.dev_locked());
        }
        U128(self.holders.get(&account_id).map(|h| h.balance).unwrap_or(0))
    }

    pub fn ft_metadata(&self) -> FungibleTokenMetadata {
        FungibleTokenMetadata {
            spec: "ft-1.0.0".to_string(),
            name: self.cfg.name.clone(),
            symbol: self.cfg.symbol.clone(),
            icon: self.icon.clone(),
            reference: None,
            reference_hash: None,
            decimals: self.cfg.decimals,
        }
    }

    // ----------------------------------------------------------- NEP-145

    #[payable]
    pub fn storage_deposit(&mut self, account_id: Option<AccountId>, registration_only: Option<bool>) -> StorageBalance {
        let _ = registration_only;
        let account = account_id.unwrap_or_else(env::predecessor_account_id);
        let deposit = env::attached_deposit().as_yoctonear();
        let refund = if self.holders.contains_key(&account) {
            deposit
        } else {
            require!(deposit >= STORAGE_COST, "attach at least 0.00125 NEAR to register");
            self.holders.insert(account, Holder { balance: 0, since: now() });
            deposit - STORAGE_COST
        };
        if refund > 0 {
            let _ = Promise::new(env::predecessor_account_id()).transfer(NearToken::from_yoctonear(refund));
        }
        storage_balance()
    }

    pub fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.holders.contains_key(&account_id).then(storage_balance)
    }

    pub fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        StorageBalanceBounds { min: U128(STORAGE_COST), max: Some(U128(STORAGE_COST)) }
    }

    /// Nothing is ever withdrawable: the flat fee covers exactly one account.
    #[payable]
    pub fn storage_withdraw(&mut self, amount: Option<U128>) -> StorageBalance {
        assert_one_yocto();
        let account = env::predecessor_account_id();
        require!(self.holders.contains_key(&account), "account is not registered");
        require!(amount.map(|a| a.0 == 0).unwrap_or(true), "nothing to withdraw");
        storage_balance()
    }

    #[payable]
    pub fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        assert_one_yocto();
        let account = env::predecessor_account_id();
        let Some(holder) = self.holders.get(&account).cloned() else {
            return false;
        };
        require!(account != self.keeper && account != self.creator, "the keeper and creator stay registered");
        if holder.balance > 0 {
            require!(force.unwrap_or(false), "balance is not empty; pass force=true to burn it");
            self.internal_debit(&account, holder.balance);
            self.total_supply -= holder.balance;
            emit("nep141", "1.0.0", "ft_burn", json!({ "owner_id": account, "amount": holder.balance.to_string(), "memo": "force unregister" }));
        }
        self.holders.remove(&account);
        let _ = Promise::new(account).transfer(NearToken::from_yoctonear(STORAGE_COST));
        true
    }
}

fn storage_balance() -> StorageBalance {
    StorageBalance { total: U128(STORAGE_COST), available: U128(0) }
}

fn transfer_event(from: &AccountId, to: &AccountId, amount: u128, memo: Option<&str>) {
    let mut data = json!({ "old_owner_id": from, "new_owner_id": to, "amount": amount.to_string() });
    if let Some(m) = memo {
        data["memo"] = json!(m);
    }
    emit("nep141", "1.0.0", "ft_transfer", data);
}

impl Revive {
    /// Move tokens. Normal transfers stamp the incoming tokens with "now";
    /// refunds keep the receiver's own stored `since`.
    pub(crate) fn internal_transfer(&mut self, sender_id: &AccountId, receiver_id: &AccountId, amount: u128, refund: bool) {
        require!(amount > 0, "amount must be positive");
        require!(sender_id != receiver_id, "sender and receiver must differ");
        require!(self.holders.contains_key(receiver_id), "receiver is not registered; call storage_deposit first");
        self.internal_debit(sender_id, amount);
        let stamp = if refund { self.holders.get(receiver_id).map(|h| h.since).unwrap_or_else(now) } else { now() };
        self.internal_credit(receiver_id, amount, stamp);
    }

    pub(crate) fn internal_debit(&mut self, account: &AccountId, amount: u128) {
        let mut h = self.holders.get(account).cloned().unwrap_or_else(|| env::panic_str("account is not registered"));
        require!(h.balance >= amount, "not enough balance");
        self.sum_bal_since -= amount * h.since as u128;
        self.late_excess -= self.late_part(amount, h.since);
        h.balance -= amount;
        self.circulating -= amount;
        self.holders.insert(account.clone(), h);
    }

    /// Credit tokens that are `stamp` old. The new `since` is the
    /// balance-weighted average of the old tokens and the new ones.
    pub(crate) fn internal_credit(&mut self, account: &AccountId, amount: u128, stamp: u64) {
        let mut h = self.holders.get(account).cloned().unwrap_or_else(|| env::panic_str("account is not registered"));
        let old = h.balance * h.since as u128;
        let new_balance = h.balance + amount;
        // Round up: rounding must never make tokens look older than they are.
        let new_since = (old + amount * stamp as u128).div_ceil(new_balance) as u64;
        self.sum_bal_since = self.sum_bal_since - old + new_balance * new_since as u128;
        self.late_excess = self.late_excess - self.late_part(h.balance, h.since) + self.late_part(new_balance, new_since);
        h.balance = new_balance;
        h.since = new_since;
        self.circulating += amount;
        self.holders.insert(account.clone(), h);
    }
}
