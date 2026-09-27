# Timelock controller for administrative actions

Privileged actions on this contract used to take effect in the same transaction
that requested them: a compromised or careless admin key could hand over the
admin role, rotate the signing key, or swap the contract WASM with no warning.
The timelock controller ([`src/timelock.rs`](../src/timelock.rs)) puts a
mandatory, publicly observable waiting period in front of those actions.

## Architecture

```
enable_timelock(delay)      one-way switch, admin only
        │
        ▼
timelock_schedule(action) ──► TimelockOp(id) { action, eta, scheduled_at }
        │                              │
        │ eta = now + delay            │  observable on-chain + event
        ▼                              ▼
timelock_execute(id)  ◄── only when ledger timestamp >= eta
timelock_cancel(id)   ◄── admin may drop it at any point before execution
timelock_sweep_expired(id) ◄── anyone may remove expired ops (now > eta + GRACE_PERIOD)
```

Two pieces of state, both introduced in `DataKey`:

- `TimelockDelay` (instance) — the delay in seconds. **Its presence is what
  enables the timelock.** Absent ⇒ controller disabled.
- `TimelockOp(id)` (persistent, ~1 year TTL) — one scheduled operation.
  `TimelockOps` (instance) holds the id list for enumeration.

### Grace period

Every scheduled operation has a **grace period** of `GRACE_PERIOD` (14 days,
1,209,600 seconds) after its ETA. The operation must be executed within this
window:

```
scheduled_at ──delay──► ETA ──GRACE_PERIOD──► expiry
                            │
                            ├── execute()   (eta ≤ now ≤ expiry)
                            └── sweep_expired()  (now > expiry)
```

- `timelock_execute` succeeds only while `now ≤ eta + GRACE_PERIOD`.
- After `now > eta + GRACE_PERIOD` the operation is **expired** and can no
  longer be executed. It remains in storage until someone calls
  `timelock_sweep_expired` to remove it from the pending list.
- The grace period bounds the execution window, preventing stale operations
  (e.g. a `SetAdmin` to a retired key, or an `Upgrade` to a superseded WASM
  hash) from being executed months later by an unsuspecting admin.

The constants are defined in [`src/timelock.rs`](../src/timelock.rs):
- `MIN_DELAY = 1 hour`
- `MAX_DELAY = 30 days`
- `GRACE_PERIOD = 14 days`

### Operation ids

An id is `SHA256(variant_tag || XDR(payload))` — deterministic and independent
of the ETA. Two consequences:

- The same action cannot be queued twice concurrently
  (`TimelockOperationExists`), so the queue can't be spammed with duplicates.
- Off-chain tooling can pre-compute the id it will later need to execute;
  `timelock_operation_id(action)` exposes the same computation as a view.

### Actions

`TimelockAction` is a closed enum, so a scheduled operation can never invoke
something the contract does not already expose:

| Variant | Effect on execute |
| --- | --- |
| `SetAdmin(address)` | Replaces `Admin`, clears any `PendingAdmin`, emits `("admin", "updated")`. |
| `SetAdminPubKey(bytes32)` | Rotates the Ed25519 mint-signing key. |
| `Upgrade(wasm_hash)` | Emits `("upgrade",)` then `update_current_contract_wasm`. |
| `SetWhitelistRoot(bytes32)` | Publishes a new whitelist merkle root. |
| `SetTimelockDelay(seconds)` | Changes the delay itself. |

The delay is bounded to `MIN_DELAY` (1 hour) … `MAX_DELAY` (30 days).
`InvalidTimelockDelay` is raised at *schedule* time as well as at execute time,
so an out-of-range value can never sit in the queue waiting to brick the
controller.

## Privileged entrypoint coverage

Once enabled, entrypoints marked **Yes** reject direct calls with
`TimelockRequired` and must be reached through `timelock_schedule` plus
`timelock_execute`. Entry points marked **No** remain direct admin calls by
design and are not represented by a `TimelockAction` variant.

| Entrypoint | Timelocked? | Rationale |
| --- | --- | --- |
| `initialize` | No | Deployment bootstrap is single-use and must be signed by the configured admin account. |
| `update_admin` | Yes | Ownership changes need an observable delay. |
| `propose_admin` / `accept_admin` | Yes | Two-step handover must not bypass the delay. |
| `cancel_proposed_admin` | No | Clearing a stale handover does not grant access or change ownership. |
| `upgrade` | Yes | WASM changes need an observable delay. |
| `set_whitelist_root` / `clear_whitelist_root` | Yes | Whitelist access-control changes need an observable delay. |
| `TimelockAction::SetAdminPubKey` | Yes | Mint-signing key rotation needs an observable delay; it has no direct entrypoint. |
| `migrate` | No | No corresponding timelock action exists; retained as a direct admin migration operation. |
| `set_name` / `set_symbol` | No | No corresponding timelock action exists; retained as direct admin metadata configuration. |
| `backfill_wrap_periods` | No | No corresponding timelock action exists; retained as a direct admin migration operation. |
| `pause` / `unpause` | No | An emergency stop must remain immediately available. |
| `set_transfer_fee` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_expiration_duration` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_fee_params` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_stake_config` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_bridge_relayer` | No | No corresponding timelock action exists; retained as a direct admin configuration. |
| `set_chain_status` | No | No corresponding timelock action exists; retained as a direct admin configuration. |

The **No** entries are an explicit scope decision: they remain admin-only, but
the current closed `TimelockAction` enum provides no delayed operation for them.

## Governance and timelock interaction

Governance (`execute_admin_proposal`) and the timelock are two overlapping
paths to the same privileged action — changing the admin. Their interaction is
specified here and covered by tests in
[`src/tests/governance_timelock.rs`](../src/tests/governance_timelock.rs).

### Does a governance proposal execute immediately?

It depends on whether the timelock is enabled, and this is explicit in code:

- **Timelock disabled** (`TimelockDelay` absent): a passing proposal executes
  immediately. `execute_admin_proposal` sets `Admin` in the same transaction.
- **Timelock enabled** (`TimelockDelay` present): a passing proposal does **not**
  execute immediately. The current admin must authorize execution, and the
  passing proposal queues `TimelockAction::SetAdmin`; the admin remains
  unchanged until that queued operation reaches its ETA and is executed via
  `timelock_execute`.

So governance never bypasses the timelock: when the timelock is on, the
proposal's effect is routed through the same delay as a direct admin handover.

### Can a timelocked action change the admin while a proposal is open?

Yes, and it is not a bypass. A `SetAdmin` scheduled through the timelock is an
independent, admin-authorized operation. If it executes while a governance
proposal to change the admin is still open, it simply replaces `Admin` and
clears any `PendingAdmin`; the open proposal is then evaluated against the new
admin. Both paths require the current admin's authorization, so neither can
silently override the other without the admin's involvement.

### Can governance schedule, cancel, or shorten a timelocked action?

No. Governance has no entrypoint that touches the timelock queue:

- **Schedule** — only `timelock_schedule` (admin-only) queues operations.
  `execute_admin_proposal` may *cause* a `SetAdmin` to be queued when the
  timelock is enabled, but it cannot schedule arbitrary actions.
- **Cancel** — only `timelock_cancel` (admin-only) removes a queued operation.
- **Shorten** — the delay can only be changed by
  `TimelockAction::SetTimelockDelay`, which is itself a timelocked action and is
  bounded to `MIN_DELAY` … `MAX_DELAY`. Governance cannot shorten it.

### No path bypasses both mechanisms

Every privileged action is reachable only through one of two doors, and both
require the current admin:

- Direct admin entrypoints marked **Yes** reject calls with `TimelockRequired`
  once the timelock is enabled, so they must go through
  `timelock_schedule` + `timelock_execute`.
- `execute_admin_proposal` requires the current admin's authorization and, when
  the timelock is enabled, routes its effect through the timelock.

There is no entrypoint that reaches a privileged action without either the
timelock (when enabled) or the admin's authorization, so no path bypasses both.

## Guarantees and caveats

- **One-way switch.** `enable_timelock` can be called once
  (`TimelockAlreadyEnabled`). There is no disable path; lengthening or
  shortening the delay is itself a timelocked action, so any weakening of the
  protection is announced by the same delay it is trying to weaken.
- **Execution is not automatic.** After the ETA passes, the admin must still
  call `timelock_execute`. A queued operation is removed from storage *before*
  its effect is applied, so it can never be replayed.
- **`eta` uses ledger timestamps**, not wall clock; treat the delay as
  approximate to within normal ledger-close drift.
- **Cancellation is admin-only.** The timelock buys observers time to react
  (withdraw, alert, fork); it does not give them a veto.
- Existing deployments are unaffected until `enable_timelock` is called, so this
  is a backwards-compatible addition.

## Operator runbook

```bash
# 1. Turn it on with a 48-hour delay (one-way).
enable_timelock --delay_seconds 172800

# 2. Queue an admin handover; note the returned id (or pre-compute it with
#    timelock_operation_id).
timelock_schedule --action '{"SetAdmin":"G..."}'

# 3. Anyone can audit the queue while the clock runs.
timelock_pending
timelock_operation --id <id>     # -> { action, eta, scheduled_at }

# 4. After eta, apply it.
timelock_execute --id <id>

# Abort instead, at any time before step 4:
timelock_cancel --id <id>
```

## Errors

| Code | Error | Meaning |
| --- | --- | --- |
| 17 | `TimelockNotReady` | ETA not reached. |
| 18 | `TimelockOperationNotFound` | Unknown or already-executed id. |
| 19 | `TimelockOperationExists` | Identical action already queued. |
| 20 | `InvalidTimelockDelay` | Delay out of bounds, or not enabled. |
| 21 | `TimelockRequired` | Direct call to a timelocked entrypoint. |
| 22 | `TimelockAlreadyEnabled` | `enable_timelock` called twice. |
| 23 | `TimelockExpired` | Operation past `eta + GRACE_PERIOD`. |
