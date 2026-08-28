# PONS_PROSPECTIVE_EXP001 — locked prospective paper experiment spec

Status: **INVALIDATED** (`PREFLIGHT_DATA_INTEGRITY`). Rows preserved. Do not restart as the final test. Successor: `PONS_PROSPECTIVE_EXP002`.

This document pre-registers the first locked Robinhood/Pons V2 prospective paper experiment.
It is **not** an authorization to start the run. Start only in Phase 7.5 after
`ROBINHOOD MULTIDAY READINESS = READY` and an explicit lock.

No live capital. No private keys. No broadcast. No Yellowstone payment.

The whole future collection period is **PROSPECTIVE_TEST**. There is no train/test
split inside the same period. P0–P4 were transferred from Solana descriptive
research and must not be refit on Robinhood data.

---

## 1. Identity

| Field | Value |
|---|---|
| Experiment id | `PONS_PROSPECTIVE_EXP001` |
| Mode | `PROSPECTIVE_PAPER` |
| Chain | Robinhood (chain id 4663) |
| Launchpad | Pons V2 |
| Split | none — entire window is PROSPECTIVE_TEST |
| Alpha research valid | true for P0–P4 fills with `curve_state_quality ∈ {EXACT_BLOCK_READ, LIVE_LATEST_READ}` and `execution_model_valid = true` |
| Smoke policy | **excluded** from research statistics (`PIPELINE_SMOKE_POLICY`, `alpha_research_valid = false`) |

## 2. Duration

| | |
|---|---|
| Minimum | 7 calendar days |
| Preferred | 14 calendar days |
| Clock | wall / live (`LiveClock`) |
| Restart | process restart allowed; open paper positions must reload from Postgres with no duplicate entry |

Do not retune mid-run. A changed threshold, exit set, size, fee overlay, or
latency scenario is a **NEW_EXPERIMENT**, not EXP001.

## 3. Policies (unchanged from Phase 7.3)

Transferred from Solana descriptive hypotheses. Thresholds locked.

| Id | Rule (as implemented in `ProspectivePolicy`) |
|---|---|
| `P0_FIRST_ELIGIBLE_CONTROL` | Enter when candidate is ELIGIBLE (control) |
| `P1_SOLANA_BUYERS_3_30S` | Enter iff unique buyers in 30s window (else total unique buyers) ≥ 3 |
| `P2_SOLANA_BUYERS_PLUS_IMBALANCE` | Enter iff buyer growth (new unique buyers in 30s > 0 **or** unique-buyer acceleration 15s > 0) **and** trade-count imbalance > 0 |
| `P3_PRICE_WITHOUT_BUYERS_AVOID` | Enter unless price is up without buyer growth (`price_change_30s_bps` else `price_change_15s_bps` > 0 and no buyer growth) |
| `P4_LOW_PARTICIPATION_FILTER` | Same participation gate as P1: unique buyers 30s (else total) ≥ 3 |

Shared gates (all P0–P4):

- Security `REJECT` never enters.
- Candidate must be `ELIGIBLE`.
- Missing feature vector → `DATA_INCOMPLETE`, no enter.
- Security `WARN` is allowed (not auto-upgraded to PASS).

`PIPELINE_SMOKE_POLICY` is plumbing only and must not be mixed into EXP001
statistics.

## 4. Exit policies (small predeclared set)

| Id | Behaviour |
|---|---|
| `X2_TIME_5M` | Close remaining inventory 5 minutes after entry |
| `X6_PARTIAL_RUNNER` | Scale out on the existing PartialRunner stages; trail remainder |
| `X9_DYNAMIC_RUNNER` | PartialRunner + 15 minute time cap |

Qualification / plumbing used `X1_TIME_2M` only to prove close-path latency.
EXP001 does **not** include `X1_TIME_2M`.

Do not add further exits without a new experiment id.

## 5. Position size and execution model

| Field | Value |
|---|---|
| Quote notional | `1000000000` (integer raw quote units; not f64) |
| Latency scenario | `BASE` (`DelayModel` rh_base_ms = 1000) |
| Slippage model | `none` (adverse_bps = 0) plus venue impact vs max_slippage_bps = 10000 |
| Failure model | `none` (entry/exit failure_bps = 0) |
| Retry | research default (`max_entry_retries = 1`) |
| Snipe window | do **not** enter while `age_ms < pons_snipe_window_ms` (1000 ms); tax 9900 bps if a fill is forced inside the window |
| Graduation gap | `LaunchSwept → GRADUATION_GAP → PoolGraduated`; buys/sells `TEMPORARILY_UNAVAILABLE`; no fake execution |
| Fees | overlay **on-chain** `feeBps + creatorTaxBps` from verified getters at fill-time block; cap 2000 bps |
| Impact | existing Phase 6 constant-product bonding-curve math (`curve_swap`) on `getReserves()` |
| Virtual quote | `getReserves().quoteReserve` = phantomQuote + trackedQuote − quoteFeeBalance − creatorTaxBalance |
| Virtual token | `getReserves().tokenReserve` = trackedTokens |
| Real quote | `realQuoteReserve()` |
| Real token (buy cap) | `sellableTokens()` |
| Curve ABI | `v2-bondingcurve-getters-1` |
| Feature version | `5.0.0` |
| Security policy | `4.0.0` |
| Candidate policy | `5.0.0` |
| Strategy policy | `7.0.0` |
| Execution model | `6.0.0` (CP math) + curve-state provenance `7.4.0` |
| Fee model | `6.0.0` with live `pons_curve_bps` overlay |

Curve-state quality required for research-valid paper fills:

- `EXACT_BLOCK_READ` (preferred: `eth_call` at the fill-time block number)
- `LIVE_LATEST_READ` only if the provider cannot pin history **and** the observed head/block identity is stored on the fill

`PARTIAL`, `RECONSTRUCTED`, and `UNKNOWN` are **not** research-valid fills.
Event reconstruction is **not** enabled (CurveBuy/Sell logs do not carry virtual
reserves; no `RECONSTRUCTED_VALIDATED` mark).

## 6. Config hash / lock

Before start:

1. Persist this spec plus the lockable JSON:

```json
{
  "experiment_id": "PONS_PROSPECTIVE_EXP001",
  "policies": [
    "P0_FIRST_ELIGIBLE_CONTROL",
    "P1_SOLANA_BUYERS_3_30S",
    "P2_SOLANA_BUYERS_PLUS_IMBALANCE",
    "P3_PRICE_WITHOUT_BUYERS_AVOID",
    "P4_LOW_PARTICIPATION_FILTER"
  ],
  "exits": ["X2_TIME_5M", "X6_PARTIAL_RUNNER", "X9_DYNAMIC_RUNNER"],
  "position_size": "1000000000",
  "latency": "BASE",
  "feature_version": "5.0.0",
  "security_policy_version": "4.0.0",
  "candidate_policy_version": "5.0.0",
  "strategy_policy_version": "7.0.0",
  "execution_model_version": "6.0.0",
  "pons_curve_abi": "v2-bondingcurve-getters-1",
  "snipe_window_ms": 1000,
  "snipe_tax_bps": 9900,
  "max_total_trade_fee_bps": 2000
}
```

2. `config_hash = sha256(canonical_json(lockable_payload))` using the existing
   `StrategyExperiment::lock` hasher.
3. Insert into `strategy_experiments` with `status = LOCKED`.
4. Refuse start if `verify_lock` fails.

Any field change after lock → `NEW_EXPERIMENT`.

## 7. Outcomes

| Maturity | Meaning |
|---|---|
| `PENDING` | Token younger than the 1h descriptive horizon at last write |
| `MATURE` | 1h source-price horizon complete |
| `CENSORED_SESSION_END` | Still PENDING when the process/session ends |

Paper `TokenOutcome` / position PnL is **execution-modelled**, not chain-settled.
Do not publish headline returns. Censor incomplete horizons.

## 8. Restart recovery

Open positions persist in `simulated_positions` (`OPEN` / `SESSION_ENDED_OPEN`).
On restart:

- reload payload
- restore remaining tokens, quote cost, peak/trail, entry, exit policy, feature/security ids
- mark `entered` so the same token is not bought twice
- continue management until policy close

## 9. What this experiment will not do

- Live swaps, keys, or broadcast
- Paid Yellowstone
- ML / `OPPORTUNITY_SCORE`
- Mixing smoke fills into P0–P4 tables
- Treating Base Clanker v4 shadow as paper-valid
- Treating Solana descriptive moonshots as execution-valid
- Changing P1 buyer count or P2 imbalance because the live book looks thin

## 10. Start gate

Start only when Phase 7.4 returns:

- `ROBINHOOD MULTIDAY READINESS = READY`
- `ROBINHOOD PAPER VERDICT = END_TO_END_PAPER_PASS`

Phase 7.4 itself must **not** start this experiment.
