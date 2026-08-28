//! Descriptive research stats. Outcomes are evaluation-only.

use serde::{Deserialize, Serialize};

use crate::candidate::CandidateState;
use crate::security::assessment::SecurityVerdict;
use crate::sim::harness::SimulationReport;
use crate::sim::outcome::TokenOutcome;
use crate::sim::policy_performance;
use crate::sim::position::return_bps;
use crate::strategy::{quantile_i64, quantile_u64, StrategyThresholds};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResearchVerdict {
    EdgeSupported,
    PromisingButInsufficient,
    NoReliableEdge,
    Exp001BlockedDataset,
}

impl ResearchVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EdgeSupported => "EDGE_SUPPORTED",
            Self::PromisingButInsufficient => "PROMISING_BUT_INSUFFICIENT",
            Self::NoReliableEdge => "NO_RELIABLE_EDGE",
            Self::Exp001BlockedDataset => "EXP001_BLOCKED_DATASET",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HypothesisVerdict {
    Supported,
    NotSupported,
    Inconclusive,
}

#[derive(Debug, Clone)]
pub struct FeatureSample {
    pub token: String,
    pub value: Option<i64>,
    pub max_return_bps: Option<i64>,
    pub reached_5x: bool,
    pub reached_10x: bool,
    pub security: Option<SecurityVerdict>,
    pub eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortStats {
    pub n: usize,
    pub median: Option<i64>,
    pub p25: Option<i64>,
    pub p75: Option<i64>,
    pub missing_rate_bps: u32,
}

pub fn cohort_stats(
    samples: &[FeatureSample],
    pred: impl Fn(&FeatureSample) -> bool,
) -> CohortStats {
    let group: Vec<_> = samples.iter().filter(|s| pred(s)).collect();
    let n = group.len();
    let missing = group.iter().filter(|s| s.value.is_none()).count();
    let vals: Vec<i64> = group.iter().filter_map(|s| s.value).collect();
    CohortStats {
        n,
        median: quantile_i64(vals.clone(), 50),
        p25: quantile_i64(vals.clone(), 25),
        p75: quantile_i64(vals, 75),
        missing_rate_bps: u32::try_from(missing.saturating_mul(10_000).checked_div(n).unwrap_or(0))
            .unwrap_or(0),
    }
}

pub fn feature_lift(
    samples: &[FeatureSample],
    signal: impl Fn(&FeatureSample) -> bool,
) -> (u32, u32) {
    let base_n = samples.len().max(1);
    let base_5 = samples.iter().filter(|s| s.reached_5x).count();
    let sig: Vec<_> = samples.iter().filter(|s| signal(s)).collect();
    let sig_5 = sig.iter().filter(|s| s.reached_5x).count();
    let base_bps = u32::try_from(base_5.saturating_mul(10_000) / base_n).unwrap_or(0);
    let sig_bps = if sig.is_empty() {
        0
    } else {
        u32::try_from(sig_5.saturating_mul(10_000) / sig.len()).unwrap_or(0)
    };
    (base_bps, sig_bps)
}

pub fn quantile_outcome_rates(samples: &[FeatureSample]) -> Vec<(u8, usize, u32)> {
    let mut with_v: Vec<_> = samples
        .iter()
        .filter(|s| s.value.is_some())
        .cloned()
        .collect();
    with_v.sort_by_key(|s| s.value.unwrap_or(0));
    if with_v.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for q in 0..5u8 {
        let lo = with_v.len() * q as usize / 5;
        let hi = with_v.len() * (q as usize + 1) / 5;
        let slice = &with_v[lo..hi.max(lo + 1).min(with_v.len())];
        let n = slice.len();
        let r5 = slice.iter().filter(|s| s.reached_5x).count();
        let bps = u32::try_from(r5.saturating_mul(10_000).checked_div(n).unwrap_or(0)).unwrap_or(0);
        out.push((q + 1, n, bps));
    }
    out
}

pub fn train_thresholds(accels: Vec<i64>, vels: Vec<i64>, buyers: Vec<u64>) -> StrategyThresholds {
    StrategyThresholds::from_train_quantile(
        quantile_i64(accels, 50).unwrap_or(1),
        quantile_i64(vels, 50).unwrap_or(2),
        quantile_u64(buyers, 50).unwrap_or(3),
    )
}

pub fn moonshot_recall_bps(entered_moonshots: usize, observable_moonshots: usize) -> Option<u32> {
    if observable_moonshots == 0 {
        return None;
    }
    Some(
        u32::try_from(entered_moonshots.saturating_mul(10_000) / observable_moonshots).unwrap_or(0),
    )
}

pub fn moonshot_precision_bps(entered_that_hit: usize, entered: usize) -> Option<u32> {
    if entered == 0 {
        return None;
    }
    Some(u32::try_from(entered_that_hit.saturating_mul(10_000) / entered).unwrap_or(0))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Funnel {
    pub future_5x: usize,
    pub future_10x: usize,
    pub security_allowed_5x: usize,
    pub eligible_5x: usize,
    pub entered_5x: usize,
    pub captured_5x: usize,
}

pub fn moonshot_funnel(
    outcomes: &[TokenOutcome],
    security_ok: impl Fn(&str) -> bool,
    eligible: impl Fn(&str) -> bool,
    entered: impl Fn(&str) -> bool,
    captured: impl Fn(&str) -> bool,
) -> Funnel {
    let mut f = Funnel::default();
    for o in outcomes {
        if o.reached_5x {
            f.future_5x += 1;
            if security_ok(&o.token) {
                f.security_allowed_5x += 1;
            }
            if eligible(&o.token) {
                f.eligible_5x += 1;
            }
            if entered(&o.token) {
                f.entered_5x += 1;
            }
            if captured(&o.token) {
                f.captured_5x += 1;
            }
        }
        if o.reached_10x {
            f.future_10x += 1;
        }
    }
    f
}

/// Chronological max drawdown in bps of peak equity (quote units as i128).
pub fn chronological_drawdown_bps(pnls_in_time_order: &[i64]) -> i64 {
    let mut eq: i128 = 0;
    let mut peak: i128 = 0;
    let mut max_dd: i128 = 0;
    for p in pnls_in_time_order {
        eq += i128::from(*p);
        if eq > peak {
            peak = eq;
        }
        let dd = peak - eq;
        if dd > max_dd {
            max_dd = dd;
        }
    }
    if peak <= 0 {
        return if max_dd > 0 { 10_000 } else { 0 };
    }
    ((max_dd * 10_000) / peak) as i64
}

pub fn right_tail_share_bps(pnls: &[i64], top_n: usize) -> Option<u32> {
    if pnls.is_empty() {
        return None;
    }
    let total: i128 = pnls.iter().map(|p| i128::from(*p).max(0)).sum();
    if total <= 0 {
        return Some(0);
    }
    let mut pos: Vec<i64> = pnls.iter().copied().filter(|p| *p > 0).collect();
    pos.sort_unstable();
    pos.reverse();
    let take: i128 = pos.iter().take(top_n).map(|p| i128::from(*p)).sum();
    Some(u32::try_from((take * 10_000) / total).unwrap_or(0))
}

pub fn report_pnl_i64(report: &SimulationReport) -> Vec<i64> {
    let mut rows: Vec<_> = report
        .positions
        .iter()
        .filter_map(|p| {
            let t = p.closed_at.or(Some(p.opened_at))?;
            let r = return_bps(&p.quote_cost, &p.realized_quote).unwrap_or(0);
            Some((t, r))
        })
        .collect();
    rows.sort_by_key(|r| r.0);
    rows.into_iter().map(|r| r.1).collect()
}

pub fn sample_label(n: usize) -> &'static str {
    if n < 30 {
        "INSUFFICIENT"
    } else if n < 200 {
        "PRELIMINARY"
    } else {
        "ADEQUATE"
    }
}

pub fn _keep_perf(r: &SimulationReport) {
    let _ = policy_performance(r);
}

pub fn security_ok_verdict(v: Option<SecurityVerdict>) -> bool {
    matches!(v, Some(SecurityVerdict::Pass) | Some(SecurityVerdict::Warn))
}

pub fn eligible_state(s: CandidateState) -> bool {
    s == CandidateState::Eligible
}
