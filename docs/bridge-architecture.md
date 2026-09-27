# Bridge Architecture

This document describes how cross-chain messages are relayed into the contract and how the bridge relayer fits into the contract's overall authority model.

## Relayers

Bridge relayers submit signed cross-chain messages. A relayer is a privileged actor: it can trigger any action that the bridge is authorized to perform on the destination chain. Relayers are registered and removed by the admin (see `docs/admin-rotation.md`).

## Authority Model

Privileged actions in this contract are reachable through more than one route. The routes are:

1. **Admin direct** — the admin calls the privileged function directly. No delay. The admin can cancel any pending governance or timelock action.
2. **Governance proposal** — token holders propose and vote; on success the action is queued. Delay is the governance voting period plus the timelock delay. The admin or governance can cancel a queued proposal before execution.
3. **Timelock** — a queued action executes after the timelock delay elapses. The admin can cancel a queued action before it executes. See `docs/timelock.md`.
4. **Bridge relayer** — a registered relayer submits a signed message that triggers the action. No delay beyond message finality. The admin can deregister a relayer, which prevents future messages but does not cancel an already-submitted message.

### Fastest path per action

The fastest path to any privileged action is the one with the smallest delay, not the largest. For most actions the admin direct route is fastest (no delay). For actions the admin cannot perform directly, the bridge relayer route is fastest (message finality only). The governance and timelock routes are always slower because they include a voting period and/or a timelock delay.

### Actions reachable by multiple routes

Any action reachable by both the admin direct route and the governance/timelock route has different guarantees depending on the route: the admin route is immediate and cancellable only by the admin, while the governance route is delayed and cancellable by the admin or governance. Any action reachable by both the bridge relayer route and the admin route is subject to the relayer's signing authority in addition to the admin's authority; the admin can revoke the relayer but cannot retroactively cancel a message the relayer already submitted.

## Privileged Actions

Each privileged function in the contract appears exactly once below, with all of its routes.

| Privileged action | Admin direct | Governance proposal | Timelock | Bridge relayer |
| --- | --- | --- | --- | --- |
| `setAdmin` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `setRelayer` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `setTimelockDelay` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `pause` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | yes (message finality) |
| `unpause` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | yes (message finality) |
| `upgrade` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | no |
| `withdraw` | yes (no delay) | yes (vote + timelock) | yes (timelock delay) | yes (message finality) |

### Who may initiate, delay, and cancel

- **Admin direct:** initiated by the admin; no delay; cancellable only by the admin (by not calling it).
- **Governance proposal:** initiated by any token holder meeting the proposal threshold; delay is the voting period plus the timelock delay; cancellable by the admin or by governance before execution.
- **Timelock:** initiated by the admin or by a passed governance proposal; delay is the timelock delay; cancellable by the admin before execution.
- **Bridge relayer:** initiated by a registered relayer; delay is message finality only; cancellable by the admin only by deregistering the relayer, which does not affect already-submitted messages.

## Cross-references

- `docs/admin-rotation.md` covers the admin direct route and admin rotation.
- `docs/timelock.md` covers the timelock route and timelock delay.
