//! Phase 6 simulation. No live execution, no private keys, no broadcast.
//! FeatureEngine must not import this module (outcome leakage).
#![allow(clippy::too_many_arguments)]

pub mod descriptive;
pub mod exec;
pub mod harness;
pub mod impact;
pub mod models;
pub mod outcome;
pub mod policy;
pub mod position;
pub mod types;

pub use descriptive::DescriptiveTokenOutcome;
pub use exec::{
    EntryRequest, ExecutionEngine, ExecutionQuote, ExecutionResult, ExitRequest,
    HistoricalExecutionEngine, LiveExecutionEngine, PaperExecutionEngine, SnapshotBook,
};
pub use harness::{run_historical, SimulatedOrder, SimulationReport};
pub use impact::{executable_fill, mark_exit_quote, max_quote_at_impact, spot_price_1e18};
pub use models::SimConfig;
pub use outcome::{policy_performance, MissReason, OutcomeEngine, PolicyPerformance, TokenOutcome};
pub use policy::{all_entry_policies, all_exit_ids, exit_policy, may_enter, EntryPolicyId};
pub use position::{capture_ratio_bps, ExitPolicy, FlowSignal, PositionManager, SimulatedPosition};
pub use types::{
    ExecutionQuality, ExecutionStatus, ExitReason, LatencyScenario, OrderSide, SimulationMode,
    SimulationRun, EXECUTION_MODEL_VERSION, FEE_MODEL_VERSION, IMPACT_MODEL_VERSION,
    OUTCOME_MODEL_VERSION,
};
