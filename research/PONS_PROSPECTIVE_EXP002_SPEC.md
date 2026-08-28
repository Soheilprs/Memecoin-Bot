# PONS_PROSPECTIVE_EXP002 — clean prospective paper experiment

Successor to INVALIDATED `PONS_PROSPECTIVE_EXP001`. Strategy **identical**. Integrity gates stricter.

- Policies: P0–P4 unchanged
- Exits: X2_TIME_5M, X6_PARTIAL_RUNNER, X9_DYNAMIC_RUNNER
- Size: 1000000000 raw quote
- Coverage: calendar ≥ 7 days **and** valid_observation_coverage ≥ 95%
- Re-entry: false
- Token gate: `discovered_at >= started_at`
- Template: `PONS_TEMPLATE_HASH_UNPINNED`, `allow_security_warn = true`

Start only after 10-minute integrity qualification using `PONS_PROSPECTIVE_EXP002_QUAL`.
Qualification data is not part of EXP002. Arm queries use `{experiment_id}:%` so
`PONS_PROSPECTIVE_EXP002` never matches `PONS_PROSPECTIVE_EXP002_QUAL`.
