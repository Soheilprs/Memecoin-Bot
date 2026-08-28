//! Strategy research lab. May use outcomes. Must not feed them to FeatureEngine or EntryStrategy.

pub mod analysis;
pub mod exp001;
pub mod experiment;
pub mod integrity;
pub mod persist;
pub mod pons_exp;
pub mod pons_run;
pub mod reconcile;
pub mod run;
pub mod split;

pub use analysis::{
    chronological_drawdown_bps, cohort_stats, feature_lift, moonshot_funnel,
    moonshot_precision_bps, moonshot_recall_bps, quantile_outcome_rates, right_tail_share_bps,
    train_thresholds, FeatureSample, Funnel, HypothesisVerdict, ResearchVerdict,
};
pub use experiment::{config_hash, ExperimentStatus, StrategyExperiment};
pub use persist::SimStore;
pub use run::run_with_strategy;
pub use split::{assign_split, chronological_split, SplitBounds, SplitKind};
