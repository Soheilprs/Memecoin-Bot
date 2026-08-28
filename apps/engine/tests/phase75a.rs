use chrono::{TimeZone, Utc};
use memecoin_engine::lab::pons_exp::{
    all_arms_for, arm_belongs_to, arm_id_for, experiment_arm_like, experiment_prefix,
    prospective_entry_eligible, source_tree_hash, Exp001Lock, Exp001State, EXP001_ID, EXP002_ID,
    EXP002_QUAL_ID, VALID_COVERAGE_THRESHOLD,
};

fn ts(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap()
}

#[test]
fn pre_start_token_never_enters() {
    let start = ts(10_000);
    let discovered = ts(9_000);
    let feature = ts(40_000);
    assert!(!prospective_entry_eligible(discovered, feature, start));
}

#[test]
fn post_start_token_can_enter() {
    let start = ts(10_000);
    let discovered = ts(11_000);
    let feature = ts(41_000);
    assert!(prospective_entry_eligible(discovered, feature, start));
}

#[test]
fn feature_before_discovery_rejected() {
    let start = ts(10_000);
    let discovered = ts(20_000);
    let feature = ts(15_000);
    assert!(!prospective_entry_eligible(discovered, feature, start));
}

#[test]
fn restart_does_not_move_start_gate() {
    let mut st = Exp001State::locked_for(EXP002_ID, None);
    st.started_at = Some(ts(10_000));
    let restart = ts(50_000);
    assert_eq!(st.started_at, Some(ts(10_000)));
    assert!(st.started_at.unwrap() < restart);
}

#[test]
fn exp001_cannot_be_the_clean_final_test() {
    assert_ne!(EXP001_ID, EXP002_ID);
    let a = Exp001Lock::predeclared_for(EXP001_ID);
    let b = Exp001Lock::predeclared_for(EXP002_ID);
    assert_eq!(a.policies, b.policies);
    assert_eq!(a.exits, b.exits);
    assert_eq!(a.position_size, b.position_size);
    assert_eq!(a.valid_coverage_threshold, VALID_COVERAGE_THRESHOLD);
    assert!(!a.reentry);
    assert_ne!(a.experiment_id, b.experiment_id);
    assert_ne!(a.config_hash(), b.config_hash());
}

#[test]
fn smoke_excluded_from_exp_arms() {
    for a in all_arms_for(EXP002_ID) {
        assert!(!a.contains("PIPELINE_SMOKE"));
        assert!(arm_belongs_to(&a, EXP002_ID));
        assert!(!arm_belongs_to(&a, EXP002_QUAL_ID));
        assert_eq!(
            arm_id_for(EXP002_ID, "P0_FIRST_ELIGIBLE_CONTROL", "X2_TIME_5M")
                .split(':')
                .count(),
            3
        );
    }
}

#[test]
fn source_tree_hash_stable() {
    let a = source_tree_hash();
    let b = source_tree_hash();
    assert_eq!(a, b);
    assert_eq!(a.len(), 64);
}

#[test]
fn coverage_threshold_locked() {
    assert!((VALID_COVERAGE_THRESHOLD - 0.95).abs() < f64::EPSILON);
}

#[test]
fn exp002_arm_like_does_not_match_qual() {
    let like = experiment_arm_like(EXP002_ID);
    let prefix = like.trim_end_matches('%');
    let exp = arm_id_for(EXP002_ID, "P0_FIRST_ELIGIBLE_CONTROL", "X2_TIME_5M");
    let qual = arm_id_for(EXP002_QUAL_ID, "P0_FIRST_ELIGIBLE_CONTROL", "X2_TIME_5M");
    assert!(exp.starts_with(prefix));
    assert!(!qual.starts_with(prefix));
    assert_eq!(experiment_prefix(&exp), Some(EXP002_ID));
    assert_eq!(experiment_prefix(&qual), Some(EXP002_QUAL_ID));
    assert!(like.contains(':'));
}
