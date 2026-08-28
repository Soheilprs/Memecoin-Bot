/// Configurable milestone + periodic snapshot schedule. Not hard-coded into domain types.
#[derive(Debug, Clone)]
pub struct SnapshotSchedule {
    /// Ages in milliseconds from discovery: 5s, 15s, 30s, 60s, 2m, 5m, 15m, 30m, 1h.
    pub milestones_ms: Vec<i64>,
    pub bands: Vec<PeriodicBand>,
}

#[derive(Debug, Clone, Copy)]
pub struct PeriodicBand {
    pub from_age_ms: i64,
    pub to_age_ms: i64,
    pub every_ms: i64,
}

impl SnapshotSchedule {
    pub fn default_research() -> Self {
        Self {
            milestones_ms: vec![
                5_000, 15_000, 30_000, 60_000, 120_000, 300_000, 900_000, 1_800_000, 3_600_000,
            ],
            bands: vec![
                PeriodicBand {
                    from_age_ms: 0,
                    to_age_ms: 300_000,
                    every_ms: 5_000,
                },
                PeriodicBand {
                    from_age_ms: 300_000,
                    to_age_ms: 1_800_000,
                    every_ms: 15_000,
                },
                PeriodicBand {
                    from_age_ms: 1_800_000,
                    to_age_ms: 7_200_000,
                    every_ms: 60_000,
                },
            ],
        }
    }

    pub fn due_times(
        &self,
        discovered_ms: i64,
        from_exclusive: i64,
        until_inclusive: i64,
    ) -> Vec<i64> {
        let mut times = Vec::new();
        for m in &self.milestones_ms {
            let t = discovered_ms.saturating_add(*m);
            if t > from_exclusive && t <= until_inclusive {
                times.push(t);
            }
        }
        for band in &self.bands {
            let start = discovered_ms.saturating_add(band.from_age_ms);
            let end = discovered_ms.saturating_add(band.to_age_ms);
            let mut t = start;
            if t <= from_exclusive {
                let skipped = (from_exclusive - start) / band.every_ms + 1;
                t = start.saturating_add(skipped.saturating_mul(band.every_ms));
            }
            while t <= until_inclusive && t <= end {
                if t > from_exclusive {
                    times.push(t);
                }
                t = t.saturating_add(band.every_ms);
            }
        }
        times.sort_unstable();
        times.dedup();
        times
    }

    pub fn last_milestone_ms(&self) -> i64 {
        self.milestones_ms.last().copied().unwrap_or(0)
    }
}

impl Default for SnapshotSchedule {
    fn default() -> Self {
        Self::default_research()
    }
}

#[derive(Debug, Clone)]
pub struct MemoryPolicy {
    pub hot_ms: i64,
    pub warm_ms: i64,
    pub cold_ms: i64,
}

impl MemoryPolicy {
    pub fn default_research() -> Self {
        Self {
            hot_ms: 30 * 60 * 1000,
            warm_ms: 2 * 60 * 60 * 1000,
            cold_ms: 2 * 60 * 60 * 1000,
        }
    }
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self::default_research()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTier {
    Hot,
    Warm,
    Cold,
}
