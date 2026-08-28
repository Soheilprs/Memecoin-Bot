# Candidate engine (Phase 5)

Lifecycle for “may a future strategy even look at this token?” **Not a buy engine.**

```
DISCOVERED
    → SECURITY_PENDING          (no assessment yet)
    → DATA_INCOMPLETE           (security UNKNOWN, or WARN disallowed)
    → SECURITY_REJECTED         (verdict REJECT — terminal for eligibility)
    → WATCHING                  (PASS or WARN, policy allowing WARN)
        → CONFIRMING            (min age / trades / unique buyers)
            → ELIGIBLE          (enough data for a future strategy to evaluate)
    → EXPIRED                   (NO_ACTIVITY, INSUFFICIENT_BUYERS, MARKET_DEAD,
                                 MAX_WATCH_AGE, PROTOCOL_ENDED)
```

There is no `BUY` state. `ELIGIBLE` does not create an order.

## Security cannot be overridden

| Verdict | Candidate |
|---|---|
| REJECT | `SECURITY_REJECTED` from any prior state. Strong flow features do not matter. |
| UNKNOWN | `DATA_INCOMPLETE`. Never `ELIGIBLE`. Fail closed. |
| PASS / WARN | May enter `WATCHING` if `allow_security_warn` (WARN) is set |

Rejected tokens **keep receiving feature vectors** so later research can ask what happened to them.

## Policy (`CandidatePolicy`)

Thresholds live in config, not magic numbers in domain code. Defaults are **research priors**, not PnL-optimized:

| Field | `default` 5.0.0 | `conservative` |
|---|---|---|
| `min_confirm_age_ms` | 5s | 5s |
| `min_trades_for_confirmation` | 1 | 1 |
| `min_unique_buyers_for_confirmation` | 1 | 1 |
| `min_eligible_age_ms` | 15s | 30s |
| `min_trades_for_eligible` | 3 | 8 |
| `min_unique_buyers_for_eligible` | 2 | 5 |
| `max_candidate_age_ms` | 1h | 1h |
| `expire_no_activity_ms` | 5m | 5m |
| `allow_security_warn` | true | true |

Confirming prerequisites are deliberately broad. A future strategy policy will tighten them.

The machine walks one hop at a time (`WATCHING` → `CONFIRMING` → `ELIGIBLE`). It does not skip `CONFIRMING`.

## Versioning / parallel policies

Transitions are **append-only**:

```
candidate_state_transitions (policy_id, policy_version, from_state, to_state, reason, as_of_time, …)
token_current_candidate     PRIMARY KEY (chain, token_address, policy_id)
```

Old rows are never rewritten. Two policies can score the same token independently (`default` vs `conservative`).

## Expire reasons

| Reason | When |
|---|---|
| `NO_ACTIVITY` | Zero trades and age ≥ `expire_no_activity_ms` |
| `MARKET_DEAD` | Time since last trade ≥ `expire_no_activity_ms` |
| `INSUFFICIENT_BUYERS` | Age > max and unique buyers below eligible floor |
| `MAX_WATCH_AGE` | Age > max |
| `PROTOCOL_ENDED` | Lifecycle `INACTIVE` |
| `SECURITY_REJECT` | Assessment REJECT |

## Metrics

`candidate_transition_total{chain,launchpad,candidate_state}`, `candidate_expired_total{reason}`. No token-address labels.
