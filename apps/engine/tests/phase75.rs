use memecoin_engine::lab::pons_exp::{
    all_arms, arm_id, canonical_sha256, is_exp001_arm, parse_exit_policy, Exp001Lock, Exp001State,
    ENTRY_POLICIES, EXIT_POLICIES, EXP001_ID,
};
use memecoin_engine::security::evm::template::{
    PONS_TEMPLATE_REGISTRY_VERSION, PONS_TOKEN_RUNTIME_HASH_STATUS,
};
use memecoin_engine::strategy::ProspectivePolicy;

#[test]
fn fifteen_isolated_arms() {
    let arms = all_arms();
    assert_eq!(arms.len(), 15);
    let set: std::collections::HashSet<_> = arms.iter().cloned().collect();
    assert_eq!(set.len(), 15);
    assert!(is_exp001_arm(&arm_id(
        "P1_SOLANA_BUYERS_3_30S",
        "X2_TIME_5M"
    )));
    assert!(!is_exp001_arm("PIPELINE_SMOKE_POLICY"));
}

#[test]
fn parse_exit_from_arm() {
    assert_eq!(
        parse_exit_policy(&arm_id("P0_FIRST_ELIGIBLE_CONTROL", "X6_PARTIAL_RUNNER")),
        "X6_PARTIAL_RUNNER"
    );
    assert_eq!(parse_exit_policy("PIPELINE_SMOKE_POLICY"), "X1_TIME_2M");
}

#[test]
fn p0_p4_ids_locked() {
    let ids: Vec<_> = ProspectivePolicy::all()
        .into_iter()
        .map(|p| p.id())
        .collect();
    assert_eq!(ids, ENTRY_POLICIES);
    assert_eq!(
        EXIT_POLICIES,
        ["X2_TIME_5M", "X6_PARTIAL_RUNNER", "X9_DYNAMIC_RUNNER"]
    );
}

#[test]
fn lock_hash_stable() {
    let a = Exp001Lock::predeclared();
    let b = Exp001Lock::predeclared();
    assert_eq!(a.config_hash(), b.config_hash());
    assert_eq!(a.config_hash(), canonical_sha256(&a.to_value()));
    let st = Exp001State::locked(None);
    assert!(st.verify_lock().is_ok());
    assert_eq!(st.lock.experiment_id, EXP001_ID);
    assert_eq!(
        st.lock.pons_token_runtime_hash_status,
        PONS_TOKEN_RUNTIME_HASH_STATUS
    );
    assert_eq!(
        st.lock.template_registry_version,
        PONS_TEMPLATE_REGISTRY_VERSION
    );
    assert!(st.lock.allow_security_warn);
}

#[test]
fn smoke_policy_not_an_exp_arm() {
    for a in all_arms() {
        assert!(!a.contains("PIPELINE_SMOKE_POLICY"));
        assert!(a.starts_with(EXP001_ID));
    }
}

#[test]
fn p1_p4_overlap_is_labeled_not_changed() {
    assert_eq!(ENTRY_POLICIES[1], "P1_SOLANA_BUYERS_3_30S");
    assert_eq!(ENTRY_POLICIES[4], "P4_LOW_PARTICIPATION_FILTER");
}
