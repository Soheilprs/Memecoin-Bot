//! EXP001 gate. Strategy TEST runs only on execution-valid HISTORICAL_REPLAY data.

use crate::historical::{DatasetValidation, DatasetVerdict};
use crate::lab::analysis::ResearchVerdict;
use crate::lab::experiment::StrategyExperiment;

pub fn exp001_may_run_test(validation: &DatasetValidation) -> Result<(), &'static str> {
    if validation.verdict != DatasetVerdict::ResearchValid {
        return Err("EXP001_BLOCKED_DATASET");
    }
    if !validation.execution_valid {
        return Err("EXECUTION_NOT_VALID");
    }
    if !validation.feature_valid {
        return Err("FEATURE_NOT_VALID");
    }
    if !validation.research_session_complete() {
        return Err("DATASET_NOT_RESEARCH_VALID");
    }
    Ok(())
}

pub fn exp001_verdict(validation: &DatasetValidation) -> ResearchVerdict {
    // Strategy expectancy is blocked unless a locked TEST actually ran on
    // execution-valid data. This gate never invents EDGE_SUPPORTED.
    let _ = exp001_may_run_test(validation);
    ResearchVerdict::Exp001BlockedDataset
}

pub fn refuse_if_not_locked_once(
    experiment: &mut StrategyExperiment,
    actual_dataset_hash: &str,
) -> Result<(), &'static str> {
    exp001_may_run_test_from_experiment(experiment)?;
    experiment.begin_out_of_sample_test(actual_dataset_hash)
}

fn exp001_may_run_test_from_experiment(
    experiment: &StrategyExperiment,
) -> Result<(), &'static str> {
    if !experiment.data_quality.is_research_complete() {
        return Err("EXP001_BLOCKED_DATASET");
    }
    Ok(())
}

/// Split isolation: a token's discovery time assigns the split; later lifecycle stays in that split.
pub fn lifecycle_split_ok(
    discovery: crate::lab::split::SplitKind,
    later_event: crate::lab::split::SplitKind,
) -> bool {
    discovery == later_event
}
