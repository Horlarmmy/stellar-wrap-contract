# Security Audit Checklist for Mainnet Readiness

**Contract:** Stellar Wrap Registry  
**Version:** 0.1.0  
**Repository:** https://github.com/zintarh/stellar-wrap-contract  
**Date:** June 2026

---

## Overview

This checklist provides a formal security audit framework for the Stellar Wrap Contract before mainnet deployment. Each item is linked to its implementation location in the codebase and includes acceptance criteria.

**Audit Status:** ⚠️ INCOMPLETE - Blocking security findings open  
**External Reviewer Sign-off:** _______________  
**Date:** _______________

---

## Overall Security Status

**Status:** ❌ NOT READY FOR MAINNET — BLOCKED BY OPEN SECURITY FINDINGS

This status is **derived from the open `security`-labelled issues** in the tracker, not hand-maintained. As long as any issue carrying the `security` label remains open, this checklist MUST report NOT READY and mainnet deployment MUST be blocked. The status cannot be marked ready by editing this document alone; it flips to ready only when the derived query below returns zero open `security` issues.

**Derivation (run as part of the release process):**

```
# Any open issue with the `security` label blocks mainnet readiness.
# If this returns any results, the status above is NOT READY.
gh issue list --label security --state open
```

Do not hand-edit the status line to "READY" while the query above returns results.

---

## Blocking Findings (checked during release)

This section is verified as part of the release process. Every item below is an **open** `security`-labelled issue and blocks mainnet deployment until resolved and closed.

- [ ] **#647 — Arbitrary contract invocation.** A caller can invoke an arbitrary contract; must be constrained before mainnet.
- [ ] **#650 — Missing Merkle domain separation.** Merkle leaves are not domain-separated, enabling cross-context proof reuse; must be fixed before mainnet.
- [ ] **#651 — Unchecked arithmetic with overflow checks disabled.** Arithmetic in a profile with overflow checks disabled can wrap; must be made checked before mainnet.
- [ ] **#653 — Mint signatures never expire.** Mint signatures remain valid indefinitely; must gain expiry/replay bounds before mainnet.
- [ ] **#672 — Timelock actions stay executable forever.** Timelock actions never expire and remain executable indefinitely; must be bounded before mainnet.

**Release gate:** Mainnet deployment is blocked while any box above is unchecked. Re-run the derivation query (`gh issue list --label security --state open`) and confirm it returns no results before proceeding.

---

## Checklist Items

### 1. Ed25519 Signature Verification

**Status:** ✅ IMPLEMENTED  
**Location:** `src/lib.rs:173-175` (mint_wrap), `src/lib.rs:528-529` (update_wrap)

**Implementation Details:**
- Uses Soroban's built-in `e.crypto().ed25519_verify()`
- Payload binds: `contract_id ‖ user ‖ period ‖ archetype ‖ data_hash`
- Prevents cross-contract replay by including `current_contract_address()`
- Prevents identity theft by including user address
- Prevents period replay by including period in storage key

**Acceptance Criteria:**
- [x] Signature verification uses Ed25519 cryptographic primitives
- [x] Payload includes contract address (prevents cross-contract replay)
- [x] Payload includes user address (prevents identity theft)
- [x] Payload includes period (prevents time-based replay)
- [x] Payload includes data_hash (prevents data tampering)
- [x] Invalid signatures cause transaction to fail
- [x] All-zero or all-ones signatures are rejected (VM crypto panic)

**Test Coverage:**
- `test_replay_attack_same_period_fails` ✅
- `test_replay_attack_different_hash_same_period_fails` ✅
- `test_signature_cannot_be_stolen_by_another_user` ✅
- `test_cross_contract_replay_protection` ✅
- `test_mint_with_all_zero_signature_rejected` ✅
- `test_mint_with_all_ones_signature_rejected` ✅
- `test_mint_with_tampered_signature_rejected` ✅

**Related Issues:** #653 (mint signatures never expire — open, blocking)

---

### 2. Admin Key Rotation (No Brickable Keys)

**Status:** ✅ IMPLEMENTED  
**Location:** `src/lib.rs:89-103` (update_admin)

**Implementation Details:**
- `update_admin()` function allows current admin to designate new admin
- Requires authorization from current admin (`current_admin.require_auth()`)
- No special key material (only admin address in instance storage)
- Admin pubkey for signature verification is separate and can be rotated via contract upgrade

**Acceptance Criteria:**
- [x] Admin key can be rotated without contract redeployment
- [x] Rotation requires authorization from current admin
- [x] No single point of failure (admin address + admin pubkey are separate)
- [x] Admin pubkey can be updated via upgrade mechanism
- [x] Event emitted on admin change for monitoring

**Test Coverage:**
- `test_update_admin_success` ✅

**Related Issues:** None

---

### 3. Storage TTL Management (Data Loss Prevention)

**Status:** ✅ IMPLEMENTED  
**Location:** `src/lib.rs:274-278`, `src/lib.rs:411-425`, `src/lib.rs:670-689`

**Implementation Details:**
- All persistent storage uses 1-year TTL: `17280 * 365` ledgers
- `extend_ttl()` function allows anyone to renew user storage
- TTL extended on all writes: wrap records, count, latest period, opt-out flags
- Merkle claim records also have 1-year TTL

**Acceptance Criteria:**
- [x] Persistent storage entries have explicit TTL
- [x] TTL is sufficient for reasonable use (1 year)
- [x] Mechanism exists to extend TTL before expiration
- [x] TTL is renewed on all state-changing operations
- [x] No data loss due to TTL expiration in normal operation

**Test Coverage:**
- `test_extend_ttl_existing_wrap` ✅
- `test_extend_ttl_nonexistent_wrap_does_not_panic` ✅

**Related Issues:** None

---

### 4. Integer Overflow Protection

**Status:** ⚠️ PARTIAL — see blocking finding #651  
**Location:** `src/lib.rs:429` (count increment), `src/lib.rs:584` (count decrement)

**Implementation Details:**
- Line 429: `current_count.checked_add(1).unwrap()` (u32) - FIXED
- Line 584: `current_count - 1` with guard `if current_count > 0`
- Uses `checked_add` to prevent overflow, will panic if overflow occurs
- Production behavior is now safe and predictable

**Acceptance Criteria:**
- [x] Decrement operation has underflow guard (`current_count > 0`)
- [x] Increment operation has overflow protection (using `checked_add`)
- [x] Overflow will panic with clear error message
- [x] Document maximum wrap count per user (u32 max: 4,294,967,295)

**Test Coverage:**
- No specific overflow tests found
- **RECOMMENDATION:** Add test for maximum wrap count scenario

**Related Issues:** #651 (unchecked arithmetic with overflow checks disabled — open, blocking)

**Fix Applied:** Changed `current_count + 1` to `current_count.checked_add(1).unwrap()`

---

### 5. Error Handling (No Silent Failures)

**Status:** ✅ IMPLEMENTED  
**Location:** `src/lib.rs:27-50` (ContractError enum)

**Implementation Details:**
- All error paths use `panic_with_error!` with explicit error codes
- Error codes: 1-11 defined in ContractError enum
- No silent failures - all error conditions panic with descriptive errors

**Error Codes:**
1. AlreadyInitialized
2. NotInitialized
3. Unauthorized
4. WrapAlreadyExists
5. WrapNotFound
6. InvalidSignature
7. InvalidDataHash
8. MerkleRootNotSet
9. InvalidMerkleProof
10. MerkleAlreadyClaimed
11. InvalidMigration

**Acceptance Criteria:**
- [x] All error conditions have explicit error codes
- [x] No silent failures (all errors panic)
- [x] Error codes are documented and unique
- [x] Error messages are descriptive

**Test Coverage:**
- `test_initialize_twice_fails` ✅ (Error #1)
- `test_duplicate_period_fails` ✅ (Error #4)
- Multiple security tests verify error conditions ✅

**Related Issues:** None

---

### 6. Event Emission for State Changes

**Status:** ✅ FIXED  
**Location:** Multiple locations in `src/lib.rs`

**Implementation Details:**
Events emitted:
- `initialize` - line 78-81 (initialize) - FIXED
- `admin updated` - line 99-102 (update_admin)
- `pause` - line 103-104 (pause) - NEW
- `unpause` - line 124-125 (unpause) - NEW
- `merkle root` - line 212-213 (set_merkle_root)
- `schema migrat` - line 311-314 (migrate)
- `migrat` - line 396-399 (lazy migration)
- `opt_out` - line 345-346 (opt_out)
- `opt_in` - line 359-360 (opt_in)
- `mint` - line 445-446 (persist_wrap_record)
- `update` - line 559-560 (update_wrap)
- `revoke` - line 587-588 (revoke_wrap)
- `extend_ttl` - line 696-697 (extend_ttl) - FIXED

**Acceptance Criteria:**
- [x] All admin operations emit events
- [x] All user state changes emit events
- [x] Contract initialization emits event (FIXED)
- [x] TTL extension emits event (FIXED)
- [x] Events include relevant data for indexing

**Test Coverage:**
- `test_mint_emits_event` ✅

**Related Issues:** None

**Fixes Applied:**
- Added `initialize` events for admin and pubkey
- Added `extend_ttl` event with TTL value
- Added `pause` and `unpause` events

---

### 7. Upgrade Mechanism (Admin-Gated)

**Status:** ✅ IMPLEMENTED  
**Location:** `src/lib.rs:741-750` (upgrade function)

**Implementation Details:**
- `upgrade()` function allows admin to update WASM blob
- Requires admin authorization
- Soroban runtime validates WASM hash against uploaded blob
- Persistent storage preserved across upgrades
- Schema migration mechanism (`migrate`) for data structure changes

**Acceptance Criteria:**
- [x] Upgrade mechanism exists
- [x] Upgrade requires admin authorization
- [x] WASM hash validation by Soroban runtime
- [x] Schema migration mechanism for data compatibility
- [x] Migration is version-controlled (from_version → to_version)
- [x] Migration can only advance one version at a time

**Test Coverage:**
- No specific upgrade tests found
- **RECOMMENDATION:** Add test for upgrade flow

**Related Issues:** None

---

### 8. Reentrancy Protection

**Status:** ✅ IMPLEMENTED  
**Location:** `src/lib.rs:147-151` (mint_wrap), `src/lib.rs:230-234` (claim_wrap)

**Implementation Details:**
- Uses `MintGuard` in temporary storage
- Guard set at function entry, removed at exit
- If guard exists, function panics with Unauthori

/* … truncated 6810 chars — edit only what you need near the top … */
