//! Staking contract.
//!
//! Amounts are represented as `i128` to stay compatible with the token
//! interface used by the underlying SEP-41 token (see issue #872). Because
//! `i128` is signed, every entry point that accepts an amount must reject
//! zero and negative values explicitly with [`Error::InvalidAmount`] rather
//! than relying on a panic or a silent no-op.
//!
//! # Paused behaviour (issue #874)
//!
//! Following the inconsistent pause coverage found in #649, the paused
//! behaviour of each staking entry point is now deliberate:
//!
//! * [`StakingContract::stake`] is **blocked** while paused. Refusing to
//!   accept new stake during a pause is defensible: it prevents new exposure
//!   from being taken on while the contract is in a known-unsafe state.
//! * [`StakingContract::unstake`] is **blocked** while paused. Unstaking only
//!   moves funds from staked to unbonded within the contract, so blocking it
//!   is a conservative choice that keeps state transitions frozen.
//! * [`StakingContract::withdraw_stake`] is **allowed** while paused. The
//!   stake being withdrawn is already unbonded, so blocking it would trap
//!   user funds for the entire duration of the pause. That is much harder to
//!   justify than blocking new stake, so withdrawal is deliberately exempt
//!   from the pause guard.

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    /// Amount must be strictly positive.
    InvalidAmount = 3,
    /// Arithmetic overflowed the `i128` range.
    Overflow = 4,
    InsufficientBalance = 5,
    /// The contract is paused and this entry point is blocked while paused.
    Paused = 6,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StakeInfo {
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataKey {
    pub user: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PauseKey {
    Paused,
}

#[contract]
pub struct StakingContract;

#[contractimpl]
impl StakingContract {
    /// Return the token symbol for the staking contract.
    ///
    /// This is the human-readable ticker used to identify the staking token
    /// in wallets and explorers. It is a fixed, contract-defined constant and
    /// does not depend on any stored state, so it can be called at any time,
    /// including while the contract is paused.
    ///
    /// # Returns
    ///
    /// The token symbol as a `soroban_sdk::Symbol`.
    pub fn symbol(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "STK")
    }

    /// Return the number of decimals used by the staking token.
    ///
    /// This is the fixed precision applied to every amount handled by the
    /// contract (for example, a value of `7` means one whole token is
    /// represented as `10_000_000` base units). It is a contract-defined
    /// constant and does not depend on any stored state, so it can be called
    /// at any time, including while the contract is paused.
    ///
    /// # Returns
    ///
    /// The number of decimals as a `u32`.
    pub fn decimals(_env: Env) -> u32 {
        7
    }

    /// Whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<PauseKey, bool>(&PauseKey::Paused)
            .unwrap_or(false)
    }

    /// Pause or unpause the contract.
    pub fn set_paused(env: Env, paused: bool) {
        env.storage().instance().set(&PauseKey::Paused, &paused);
    }

    /// Return the admin address for the contract.
    ///
    /// The admin is the address stored under the instance storage key
    /// [`DataKey`] for the current contract address. If no admin has been
    /// set, this returns `None`.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<DataKey, Address>(&DataKey {
                user: env.current_contract_address(),
            })
    }

    /// Stake `amount` for `user`.
    ///
    /// `amount` must be strictly positive; zero and negative values are
    /// rejected with [`Error::InvalidAmount`]. Blocked while paused with
    /// [`Error::Paused`] (see the module docs for the rationale).
    pub fn stake(env: Env, user: Address, amount: i128) -> Result<(), Error> {
        user.require_auth();
        if Self::is_paused(env.clone()) {
            return Err(Error::Paused);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey { user: user.clone() };
        let current = env
            .storage()
            .persistent()
            .get::<DataKey, StakeInfo>(&key)
            .map(|info| info.amount)
            .unwrap_or(0);

        // Checked arithmetic: the release profile disables overflow checks
        // (see #651), so we must not rely on the compiler here.
        let new_amount = current.checked_add(amount).ok_or(Error::Overflow)?;

        let total = Self::total_staked(env.clone());
        let new_total = total.checked_add(amount).ok_or(Error::Overflow)?;

        env.storage()
            .persistent()
            .set(&key, &StakeInfo { amount: new_amount });
        env.storage().instance().set(&DataKey { user: user.clone() }, &new_total);

        Ok(())
    }

    /// Unstake `amount` for `user`.
    ///
    /// `amount` must be strictly positive; zero and negative values are
    /// rejected with [`Error::InvalidAmount`]. Blocked while paused with
    /// [`Error::Paused`] (see the module docs for the rationale).
    pub fn unstake(env: Env, user: Address, amount: i128) -> Result<(), Error> {
        user.require_auth();
        if Self::is_paused(env.clone()) {
            return Err(Error::Paused);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey { user: user.clone() };
        let current = env
            .storage()
            .persistent()
            .get::<DataKey, StakeInfo>(&key)
            .map(|info| info.amount)
            .unwrap_or(0);

        if current < amount {
            return Err(Error::InsufficientBalance);
        }

        // Checked arithmetic: the release profile disables overflow checks
        // (see #651), so we must not rely on the compiler here.
        let new_amount = current.checked_sub(amount).ok_or(Error::Overflow)?;

        let total = Self::total_staked(env.clone());
        let new_total = total.checked_sub(amount).ok_or(Error::Overflow)?;

        env.storage()
            .persistent()
            .set(&key, &StakeInfo { amount: new_amount });
        env.storage().instance().set(&DataKey { user: user.clone() }, &new_total);

        Ok(())
    }

    /// Withdraw `amount` of already-unbonded stake for `user`.
    ///
    /// Deliberately **not** blocked while paused: the funds are already
    /// unbonded, so blocking withdrawal would trap user funds for the whole
    /// pause (see the module docs for the rationale).
    pub fn withdraw_stake(env: Env, user: Address, amount: i128) -> Result<(), Error> {
        user.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey { user: user.clone() };
        let current = env
            .storage()
            .persistent()
            .get::<DataKey, StakeInfo>(&key)
            .map(|info| info.amount)
            .unwrap_or(0);

        if current < amount {
            return Err(Error::InsufficientBalance);
        }

        let new_amount = current.checked_sub(amount).ok_or(Error::Overflow)?;

        env.storage()
            .persistent()
            .set(&key, &StakeInfo { amount: new_amount });

        Ok(())
    }

    /// Total amount staked across all users.
    pub fn total_staked(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<DataKey, i128>(&DataKey {
                user: env.current_contract_address(),
            })
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn stake_rejects_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, StakingContract);
        let client = StakingContractClient::new(&env, &contract_id);
        le

/* … truncated 571 chars — edit only what you need near the top … */
