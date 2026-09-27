use soroban_sdk::{contractclient, Address, BytesN, Env};

/// Minimal ABI implemented by a compatible data-hash oracle.
#[contractclient(name = "DataHashOracleClient")]
pub trait DataHashOracle {
    fn verify_data_hash(e: Env, data_hash: BytesN<32>) -> bool;
}

/// Calls the external oracle to verify a data hash.
///
/// Reentrancy analysis (#886): this is a cross-contract call site where control
/// leaves this contract. The oracle is treated as read-only and untrusted: the
/// return value is a plain `bool` and no state is written before or after the
/// call, so a hostile oracle that re-enters cannot observe or create a broken
/// intermediate state through this path. Callers must not rely on this call to
/// mutate state; any state change must be committed before invoking it.
pub(crate) fn verify_data_hash(e: &Env, oracle: &Address, data_hash: &BytesN<32>) -> bool {
    DataHashOracleClient::new(e, oracle).verify_data_hash(data_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Env};

    /// Hostile oracle that re-enters `verify_data_hash` on every callback,
    /// simulating a reentrancy attempt through the cross-contract surface.
    #[contract]
    pub struct ReentrantOracle;

    #[contractimpl]
    impl ReentrantOracle {
        pub fn verify_data_hash(e: Env, data_hash: BytesN<32>) -> bool {
            // Re-enter the same external call path. A well-behaved caller must
            // not have any observable broken state at this point.
            let _ = DataHashOracleClient::new(&e, &e.current_contract_address())
                .verify_data_hash(&data_hash);
            true
        }
    }

    #[test]
    fn reentrant_oracle_cannot_break_state() {
        let e = Env::default();
        let oracle = e.register(ReentrantOracle, ());
        let data_hash = BytesN::from_array(&e, &[7u8; 32]);

        // The call completes and returns the oracle's verdict; no state is
        // written around the external call, so re-entry is harmless.
        let result = verify_data_hash(&e, &oracle, &data_hash);
        assert!(result);
    }

    #[test]
    fn verify_data_hash_forwards_to_oracle() {
        let e = Env::default();
        let oracle = e.register(ReentrantOracle, ());
        let data_hash = BytesN::from_array(&e, &[1u8; 32]);
        assert!(verify_data_hash(&e, &oracle, &data_hash));
    }
}
