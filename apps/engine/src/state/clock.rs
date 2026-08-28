use chrono::{DateTime, Utc};

/// Logical market time in unix milliseconds. Historical replay never uses wall clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct StateTime {
    pub unix_ms: i64,
}

impl StateTime {
    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        Self {
            unix_ms: dt.timestamp_millis(),
        }
    }

    pub fn from_unix_secs(secs: i64) -> Self {
        Self {
            unix_ms: secs.saturating_mul(1000),
        }
    }

    pub fn datetime(self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.unix_ms).unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
    }

    pub fn saturating_add_ms(self, ms: i64) -> Self {
        Self {
            unix_ms: self.unix_ms.saturating_add(ms),
        }
    }
}

pub trait StateClock: Send {
    fn now(&self) -> StateTime;
    fn advance_to(&mut self, t: StateTime);
}

#[derive(Debug, Clone)]
pub struct ReplayClock {
    now: StateTime,
}

impl ReplayClock {
    pub fn new() -> Self {
        Self {
            now: StateTime { unix_ms: 0 },
        }
    }

    pub fn at(t: StateTime) -> Self {
        Self { now: t }
    }
}

impl Default for ReplayClock {
    fn default() -> Self {
        Self::new()
    }
}

impl StateClock for ReplayClock {
    fn now(&self) -> StateTime {
        self.now
    }

    fn advance_to(&mut self, t: StateTime) {
        if t > self.now {
            self.now = t;
        }
    }
}

/// Live clock: max(last event time, wall clock). Timers use this for periodic snapshots.
#[derive(Debug, Clone)]
pub struct LiveClock {
    last_event: StateTime,
}

impl LiveClock {
    pub fn new() -> Self {
        Self {
            last_event: StateTime {
                unix_ms: Utc::now().timestamp_millis(),
            },
        }
    }
}

impl Default for LiveClock {
    fn default() -> Self {
        Self::new()
    }
}

impl StateClock for LiveClock {
    fn now(&self) -> StateTime {
        let wall = StateTime {
            unix_ms: Utc::now().timestamp_millis(),
        };
        wall.max(self.last_event)
    }

    fn advance_to(&mut self, t: StateTime) {
        if t > self.last_event {
            self.last_event = t;
        }
    }
}

#[derive(Debug, Clone)]
pub enum EngineClock {
    Replay(ReplayClock),
    Live(LiveClock),
}

impl EngineClock {
    pub fn replay() -> Self {
        Self::Replay(ReplayClock::new())
    }

    pub fn live() -> Self {
        Self::Live(LiveClock::new())
    }

    pub fn is_replay(&self) -> bool {
        matches!(self, Self::Replay(_))
    }
}

impl StateClock for EngineClock {
    fn now(&self) -> StateTime {
        match self {
            Self::Replay(c) => c.now(),
            Self::Live(c) => c.now(),
        }
    }

    fn advance_to(&mut self, t: StateTime) {
        match self {
            Self::Replay(c) => c.advance_to(t),
            Self::Live(c) => c.advance_to(t),
        }
    }
}
