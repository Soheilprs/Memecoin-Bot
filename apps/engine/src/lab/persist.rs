//! Persist Phase 6 simulation reports and Phase 7 experiments.

use async_trait::async_trait;

use crate::error::Result;
use crate::sim::harness::SimulationReport;
use crate::sim::outcome::TokenOutcome;
use crate::sim::types::SimulationRun;
use crate::sim::PolicyPerformance;

use super::experiment::StrategyExperiment;

#[async_trait]
pub trait SimStore: Send + Sync {
    async fn insert_simulation_run(&self, r: &SimulationRun) -> Result<i64>;
    async fn get_simulation_run(&self, id: i64) -> Result<Option<SimulationRun>>;
    async fn persist_report(&self, report: &SimulationReport) -> Result<i64>;
    async fn load_report(&self, run_id: i64) -> Result<Option<SimulationReport>>;
    async fn insert_token_outcome(&self, o: &TokenOutcome) -> Result<i64>;
    async fn insert_policy_performance(&self, run_id: i64, p: &PolicyPerformance) -> Result<i64>;
    async fn upsert_experiment(&self, e: &StrategyExperiment) -> Result<()>;
    async fn get_experiment(&self, id: &str) -> Result<Option<StrategyExperiment>>;
}
