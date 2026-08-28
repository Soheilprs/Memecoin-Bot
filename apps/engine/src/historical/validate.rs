//! Research-valid dataset gate. Incomplete data must not become HISTORICAL_REPLAY.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::domain::{
    AmountQuality, CorpusEventType, IdentityQuality, QualityStatus, RawEvent,
    SOURCE_KIND_DECODED_RESEARCH_CORPUS,
};

use super::manifest::DatasetManifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DatasetVerdict {
    ResearchValid,
    FeatureOnly,
    Invalid,
    NotFound,
}

impl DatasetVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResearchValid => "RESEARCH_VALID",
            Self::FeatureOnly => "FEATURE_ONLY",
            Self::Invalid => "INVALID",
            Self::NotFound => "NOT_FOUND",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GraduationBias {
    AllLaunches,
    GraduatedOnly,
    Unknown,
}

impl GraduationBias {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllLaunches => "ALL_LAUNCHES",
            Self::GraduatedOnly => "GRADUATED_ONLY",
            Self::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatasetScan {
    pub events: u64,
    pub invalid_rows: u64,
    pub rejected_rows: u64,
    pub exact_dup_rows: u64,
    pub dup_event_ids: u64,
    pub ordering_violations: u64,
    pub has_slot: u64,
    pub has_signature: u64,
    pub amount_onchain_integer: u64,
    pub amount_float: u64,
    pub amount_missing: u64,
    pub trades: u64,
    pub launches: u64,
    pub graduations: u64,
    pub schema_ok: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverageCounts {
    pub launches: u64,
    pub launches_with_trade: u64,
    pub launches_with_10_trades: u64,
    pub survived_30s: u64,
    pub survived_60s: u64,
    pub survived_2m: u64,
    pub survived_5m: u64,
    pub survived_15m: u64,
    pub graduated: u64,
    pub zero_trade: u64,
    pub incomplete_lifecycle: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuplicateReport {
    pub exact_duplicate_rows: u64,
    pub duplicate_event_ids: u64,
    pub duplicate_launches: u64,
    pub duplicate_trades: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissingInterval {
    pub start: String,
    pub end: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatasetValidation {
    pub schema_valid: bool,
    pub ordering_valid: bool,
    pub launch_population_valid: bool,
    pub dead_tokens_present: bool,
    pub trade_amounts_valid: bool,
    pub curve_reconstructable: bool,
    pub temporal_coverage_valid: bool,
    pub execution_valid: bool,
    pub feature_valid: bool,
    pub identity_quality: IdentityQuality,
    pub graduation_bias: GraduationBias,
    pub source_kind: String,
    pub quality_status: QualityStatus,
    pub coverage: CoverageCounts,
    pub duplicates: DuplicateReport,
    pub missing_intervals: Vec<MissingInterval>,
    pub invalid_rows: u64,
    pub rejected_rows: u64,
    pub checks: Vec<CheckResult>,
    pub dataset_hash: Option<String>,
    pub verdict: DatasetVerdict,
}

impl DatasetValidation {
    pub fn research_session_complete(&self) -> bool {
        self.execution_valid
            && self.feature_valid
            && self.launch_population_valid
            && self.dead_tokens_present
            && !matches!(self.graduation_bias, GraduationBias::GraduatedOnly)
    }

    pub fn session_quality(&self) -> QualityStatus {
        if self.research_session_complete() {
            QualityStatus::HistoricalReplay
        } else {
            QualityStatus::HistoricalPartial
        }
    }
}

#[derive(Debug, Default)]
struct TokenAcc {
    launch: bool,
    graduated: bool,
    trades: u64,
    max_age_milli: i64,
}

/// Incremental scan. Does not retain RawEvent payloads.
pub struct StreamingScan {
    scan: DatasetScan,
    tokens: HashMap<String, TokenAcc>,
    seen_ids: HashSet<String>,
    seen_row: HashSet<(String, u64, String)>,
    launch_mints: HashSet<String>,
    prev_key: Option<(i64, u8, String, String, u64, u64)>,
    hours: BTreeSet<i64>,
    store_ids: bool,
}

impl StreamingScan {
    pub fn new(store_ids: bool) -> Self {
        Self {
            scan: DatasetScan {
                schema_ok: true,
                ..DatasetScan::default()
            },
            tokens: HashMap::new(),
            seen_ids: HashSet::new(),
            seen_row: HashSet::new(),
            launch_mints: HashSet::new(),
            prev_key: None,
            hours: BTreeSet::new(),
            store_ids,
        }
    }

    pub fn push(&mut self, raw: &RawEvent) {
        self.scan.events += 1;
        let Some(c) = raw.as_corpus() else {
            self.scan.invalid_rows += 1;
            self.scan.schema_ok = false;
            return;
        };
        if c.mint.is_empty() {
            self.scan.invalid_rows += 1;
            return;
        }
        let eid = raw.event_id();
        if self.store_ids {
            if !self.seen_ids.insert(eid) {
                self.scan.dup_event_ids += 1;
            }
            let row_key = (
                c.source_file.clone(),
                c.source_row,
                c.event_type.as_str().to_string(),
            );
            if !self.seen_row.insert(row_key) {
                self.scan.exact_dup_rows += 1;
            }
        } else if let Some(prev) = &self.prev_key {
            let key = c.order_key();
            if *prev == key {
                self.scan.exact_dup_rows += 1;
            }
        }
        let key = c.order_key();
        if let Some(prev) = &self.prev_key {
            if key < *prev {
                self.scan.ordering_violations += 1;
            }
        }
        self.prev_key = Some(key);
        self.hours.insert(c.timestamp.timestamp().div_euclid(3600));
        if c.slot.is_some() {
            self.scan.has_slot += 1;
        }
        if c.signature.as_ref().is_some_and(|s| !s.is_empty()) {
            self.scan.has_signature += 1;
        }
        match c.amount_quality {
            AmountQuality::OnchainInteger => self.scan.amount_onchain_integer += 1,
            AmountQuality::FloatNotInteger | AmountQuality::IntegerValuedFloat => {
                self.scan.amount_float += 1
            }
            AmountQuality::Missing | AmountQuality::Inconsistent => self.scan.amount_missing += 1,
        }
        let acc = self.tokens.entry(c.mint.clone()).or_default();
        match c.event_type {
            CorpusEventType::Launch => {
                self.launch_mints.insert(c.mint.clone());
                acc.launch = true;
                self.scan.launches += 1;
            }
            CorpusEventType::Trade => {
                acc.trades += 1;
                self.scan.trades += 1;
                if let Some(ms) = c.seconds_since_launch_milli {
                    acc.max_age_milli = acc.max_age_milli.max(ms);
                }
            }
            CorpusEventType::Graduation => {
                acc.graduated = true;
                self.scan.graduations += 1;
            }
        }
    }

    pub fn finish(
        self,
    ) -> (
        DatasetScan,
        CoverageCounts,
        DuplicateReport,
        Vec<MissingInterval>,
    ) {
        let scan = self.scan;
        let mut dup_launches = 0u64;
        if scan.launches > self.launch_mints.len() as u64 {
            dup_launches = scan.launches - self.launch_mints.len() as u64;
        }
        let mut coverage = CoverageCounts {
            launches: self.launch_mints.len() as u64,
            graduated: self.tokens.values().filter(|t| t.graduated).count() as u64,
            ..CoverageCounts::default()
        };
        for t in self.tokens.values() {
            if t.trades >= 1 {
                coverage.launches_with_trade += 1;
            }
            if t.trades >= 10 {
                coverage.launches_with_10_trades += 1;
            }
            if t.trades == 0 {
                coverage.zero_trade += 1;
            }
            if t.max_age_milli >= 30_000 {
                coverage.survived_30s += 1;
            }
            if t.max_age_milli >= 60_000 {
                coverage.survived_60s += 1;
            }
            if t.max_age_milli >= 120_000 {
                coverage.survived_2m += 1;
            }
            if t.max_age_milli >= 300_000 {
                coverage.survived_5m += 1;
            }
            if t.max_age_milli >= 900_000 {
                coverage.survived_15m += 1;
            }
            if t.launch && !t.graduated && t.trades == 0 {
                coverage.incomplete_lifecycle += 1;
            }
        }
        let duplicates = DuplicateReport {
            exact_duplicate_rows: scan.exact_dup_rows,
            duplicate_event_ids: scan.dup_event_ids,
            duplicate_launches: dup_launches,
            duplicate_trades: 0,
        };
        let missing = detect_hour_gaps(&self.hours);
        (scan, coverage, duplicates, missing)
    }
}

/// Scan of already-normalized corpus RawEvents (tests / small subsets).
pub fn scan_raw_events<'a, I>(
    events: I,
) -> (
    DatasetScan,
    CoverageCounts,
    DuplicateReport,
    Vec<MissingInterval>,
)
where
    I: IntoIterator<Item = &'a RawEvent>,
{
    let mut s = StreamingScan::new(true);
    for raw in events {
        s.push(raw);
    }
    s.finish()
}

pub fn detect_hour_gaps(sorted_hours: &BTreeSet<i64>) -> Vec<MissingInterval> {
    let mut missing = Vec::new();
    let mut iter = sorted_hours.iter();
    let Some(mut prev) = iter.next().copied() else {
        return missing;
    };
    for &h in iter {
        if h > prev + 1 {
            missing.push(MissingInterval {
                start: format!("{}", prev + 1),
                end: format!("{}", h - 1),
                reason: format!("no events for {} hour(s)", h - prev - 1),
            });
        }
        prev = h;
    }
    missing
}

pub fn graduation_bias(launches: u64, graduated: u64) -> GraduationBias {
    if launches == 0 {
        GraduationBias::Unknown
    } else if graduated == launches {
        GraduationBias::GraduatedOnly
    } else {
        GraduationBias::AllLaunches
    }
}

/// Fail-closed gate used before EXP001.
pub fn validate_historical_dataset(
    manifest: Option<&DatasetManifest>,
    scan: &DatasetScan,
    coverage: &CoverageCounts,
    duplicates: &DuplicateReport,
    missing: &[MissingInterval],
) -> DatasetValidation {
    let mut checks = Vec::new();

    let schema_valid = scan.schema_ok && coverage.launches > 0;
    checks.push(CheckResult {
        name: "schema_valid".into(),
        passed: schema_valid,
        detail: format!("events={} invalid={}", scan.events, scan.invalid_rows),
    });

    let ordering_valid = scan.ordering_violations == 0;
    checks.push(CheckResult {
        name: "ordering_valid".into(),
        passed: ordering_valid,
        detail: format!("violations={}", scan.ordering_violations),
    });

    let bias = graduation_bias(coverage.launches, coverage.graduated);
    let launch_population_valid = coverage.launches > 0 && bias != GraduationBias::GraduatedOnly;
    checks.push(CheckResult {
        name: "launch_population_valid".into(),
        passed: launch_population_valid,
        detail: format!(
            "launches={} grads={} bias={}",
            coverage.launches,
            coverage.graduated,
            bias.as_str()
        ),
    });

    let dead_tokens_present = coverage.zero_trade > 0 || coverage.graduated < coverage.launches;
    checks.push(CheckResult {
        name: "dead_tokens_present".into(),
        passed: dead_tokens_present,
        detail: format!(
            "zero_trade={} non_grad={}",
            coverage.zero_trade,
            coverage.launches.saturating_sub(coverage.graduated)
        ),
    });

    let identity =
        if scan.has_signature == scan.events && scan.events > 0 && scan.has_slot == scan.events {
            IdentityQuality::OnchainExact
        } else {
            IdentityQuality::Derived
        };

    let trade_amounts_valid = scan.trades > 0
        && scan.amount_onchain_integer > 0
        && scan.amount_missing == 0
        && scan.amount_float == 0;
    checks.push(CheckResult {
        name: "trade_amounts_valid".into(),
        passed: trade_amounts_valid,
        detail: format!(
            "integer={} float={} missing={}",
            scan.amount_onchain_integer, scan.amount_float, scan.amount_missing
        ),
    });

    let curve_reconstructable = trade_amounts_valid && identity == IdentityQuality::OnchainExact;
    checks.push(CheckResult {
        name: "curve_reconstructable".into(),
        passed: curve_reconstructable,
        detail: "requires on-chain integer reserves and exact tx identity".into(),
    });

    let temporal_coverage_valid = true;
    checks.push(CheckResult {
        name: "temporal_coverage_valid".into(),
        passed: temporal_coverage_valid,
        detail: format!("missing_intervals={} (recorded, not hidden)", missing.len()),
    });

    let feature_valid = schema_valid
        && ordering_valid
        && launch_population_valid
        && dead_tokens_present
        && scan.events > 0;
    checks.push(CheckResult {
        name: "feature_valid".into(),
        passed: feature_valid,
        detail: "point-in-time counts/timing reconstructable; volume may be UNKNOWN".into(),
    });

    let execution_valid = feature_valid
        && trade_amounts_valid
        && curve_reconstructable
        && identity == IdentityQuality::OnchainExact
        && scan.has_slot == scan.events;
    checks.push(CheckResult {
        name: "execution_valid".into(),
        passed: execution_valid,
        detail: "Phase 6 integer curve fills require ONCHAIN_INTEGER amounts + slot/signature"
            .into(),
    });

    let verdict = if !schema_valid || !launch_population_valid {
        DatasetVerdict::Invalid
    } else if feature_valid && execution_valid {
        DatasetVerdict::ResearchValid
    } else if feature_valid {
        DatasetVerdict::FeatureOnly
    } else {
        DatasetVerdict::Invalid
    };

    let mut v = DatasetValidation {
        schema_valid,
        ordering_valid,
        launch_population_valid,
        dead_tokens_present,
        trade_amounts_valid,
        curve_reconstructable,
        temporal_coverage_valid,
        execution_valid,
        feature_valid,
        identity_quality: identity,
        graduation_bias: bias,
        source_kind: SOURCE_KIND_DECODED_RESEARCH_CORPUS.into(),
        quality_status: QualityStatus::HistoricalPartial,
        coverage: coverage.clone(),
        duplicates: duplicates.clone(),
        missing_intervals: missing.to_vec(),
        invalid_rows: scan.invalid_rows,
        rejected_rows: scan.rejected_rows,
        checks,
        dataset_hash: manifest.and_then(|m| m.dataset_hash.clone()),
        verdict,
    };
    v.quality_status = v.session_quality();
    v
}

/// EXP001 historical security: UNKNOWN is not PASS; archive mint state is not filled from now.
pub fn exp001_historical_security_policy() -> serde_json::Value {
    serde_json::json!({
        "unknown_is_not_pass": true,
        "historically_provable": [
            "launchpad_provenance_from_corpus_label",
            "protocol_template_pumpfun"
        ],
        "requires_archive_state": [
            "mint_authorities_at_slot",
            "freeze_authority_at_slot",
            "token_program_account_owner",
            "token2022_extensions"
        ],
        "blocks_research_valid_simulation_when_unknown": true,
        "do_not_substitute_current_chain_state": true
    })
}

pub fn hourly_counts(timestamps: &[i64]) -> BTreeMap<i64, u64> {
    let mut m = BTreeMap::new();
    for t in timestamps {
        *m.entry(t.div_euclid(3600)).or_insert(0) += 1;
    }
    m
}
