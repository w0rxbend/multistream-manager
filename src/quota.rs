//! The YouTube daily quota estimate, shared by everything that spends it.
//!
//! Google's Data API is metered in units against a per-project daily
//! allowance, refilled at midnight Pacific time. Nothing tells this program
//! how much of that allowance is left, so the only way to know is to count
//! what has been spent — which means every part of the program that talks to
//! YouTube has to count into the same place.
//!
//! It lived inside the chat module, so the chat polling and message sending
//! were counted and the statistics polling was not. At the default 15-second
//! interval that is roughly 5,700 units a day — more than half of a default
//! 10,000-unit project — invisible to the reserve that exists specifically to
//! keep message sending working when reading has to stop. The reserve was
//! being computed from a number that was wrong by more than half the budget.
//!
//! One store, cloned to everything that spends. Persisted, because the
//! allowance is daily while a session is not.

use chrono::Utc;
use serde::Deserialize;

/// What each request costs, in Google's own units.
///
/// These are documented per endpoint rather than derived, so they are written
/// down once here and referenced by name at each call site. A wrong number is
/// a reserve that triggers at the wrong time, which is worse than no reserve.
pub mod cost {
    /// `liveChatMessages.list` — the chat poll.
    pub const LIST_CHAT: u64 = 5;
    /// `liveChatMessages.insert` — sending a message.
    pub const INSERT_CHAT: u64 = 50;
    /// `liveChatBans.insert` / `liveChatMessages.delete` — moderating.
    pub const MODERATE: u64 = 50;
    /// `videos.list` — the statistics poll's view and like counts.
    pub const VIDEOS_LIST: u64 = 1;
    /// `channels.list` — the statistics poll's subscriber count.
    pub const CHANNELS_LIST: u64 = 1;
    /// `liveBroadcasts.list` — one page of the housekeeping listing.
    pub const LIST_BROADCASTS: u64 = 1;
    /// `liveBroadcasts.delete` — removing one abandoned broadcast.
    pub const DELETE_BROADCAST: u64 = 50;
    /// `liveStreams.list` — one page of the reusable-stream listing.
    pub const LIST_STREAMS: u64 = 1;
}

/// A local estimate of the daily unit spend. Every dispatched request is
/// charged before its response is read, because Google charges failed
/// requests too. A limit of zero disables the ledger entirely.
///
/// The allowance is a *daily* budget: Google refills it at midnight Pacific
/// time. The ledger therefore remembers which Pacific day it is counting and
/// starts over when that day ends — without this, a session that exhausted
/// the estimate stayed parked forever, because the real quota came back at
/// midnight but the local count never did. Like the yc reference's fallback
/// path, "Pacific" is a fixed UTC−8 (no tz database dependency); being an
/// hour early during daylight-saving time only makes the estimate more
/// conservative, never less.
#[derive(Debug, Clone, Copy)]
pub(crate) struct QuotaLedger {
    limit: u64,
    used: u64,
    /// The Pacific calendar day (days since the epoch, UTC−8) `used` counts.
    day: i64,
}

/// The fixed offset standing in for America/Los_Angeles (yc's own fallback
/// when the tz database is unavailable).
const PACIFIC_FALLBACK_OFFSET_SECS: i64 = -8 * 3600;

/// The Pacific calendar day for `now`, as days since the Unix epoch.
fn pacific_day(now: chrono::DateTime<Utc>) -> i64 {
    (now.timestamp() + PACIFIC_FALLBACK_OFFSET_SECS).div_euclid(86_400)
}

impl QuotaLedger {
    fn new(limit: u64) -> Self {
        Self {
            limit,
            used: 0,
            day: pacific_day(Utc::now()),
        }
    }

    /// Start a fresh count when the Pacific day has rolled over.
    fn roll_over_if_due(&mut self) {
        let today = pacific_day(Utc::now());
        if today != self.day {
            self.day = today;
            self.used = 0;
        }
    }

    fn charge(&mut self, units: u64) {
        self.roll_over_if_due();
        self.used = self.used.saturating_add(units);
    }

    fn remaining(&mut self) -> u64 {
        self.roll_over_if_due();
        self.limit.saturating_sub(self.used)
    }

    /// Why polling must pause, if it must. The reserve exists so running out
    /// of read budget does not also take away the ability to send, which is
    /// the half of the client a stream owner cannot do without.
    fn pause_reason(&mut self, reserve_percent: u8) -> Option<String> {
        if self.limit == 0 {
            return None;
        }
        // No override is offered here on purpose. ctrl+r only clears the
        // reserve, so once nothing is left this pause would come straight
        // back — after another resolve-and-poll had already spent quota.
        if self.remaining() == 0 {
            return Some(
                "estimated daily API quota exhausted; polling stopped until the \
                 Pacific-midnight reset"
                    .to_string(),
            );
        }
        if reserve_percent == 0 {
            return None;
        }
        let reserve = self.limit * u64::from(reserve_percent.min(100)) / 100;
        if self.remaining() > reserve {
            return None;
        }
        Some(
            "estimated quota reserve reached; polling paused so message sending \
             keeps working. Press ctrl+r to override"
                .to_string(),
        )
    }
}

/// The shared, persisted quota estimate.
///
/// Every open YouTube chat spends from the *same* project allowance, so the
/// pollers must share one count — per-poller ledgers would each grant the
/// full daily budget. And the allowance is daily while sessions are not:
/// yc persists its ledger across restarts (internal/youtube/quota_store.go),
/// and so does this — a tiny JSON `{day, used}` file, written after every
/// charge (polls are seconds apart; the write is trivial) and reloaded on
/// startup, rolling over with the Pacific day like the in-memory count.
#[derive(Clone)]
pub struct QuotaStore {
    inner: std::sync::Arc<std::sync::Mutex<QuotaLedger>>,
    path: Option<std::path::PathBuf>,
}

#[derive(serde::Serialize, Deserialize)]
struct PersistedQuota {
    day: i64,
    used: u64,
}

impl QuotaStore {
    /// A store counting against `limit`, persisted at `path` when given.
    /// A saved count from an earlier session today is resumed; one from a
    /// previous Pacific day starts fresh.
    pub fn new(limit: u64, path: Option<std::path::PathBuf>) -> Self {
        let mut ledger = QuotaLedger::new(limit);
        if let Some(path) = &path {
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(saved) = serde_json::from_str::<PersistedQuota>(&text) {
                    if saved.day == ledger.day {
                        ledger.used = saved.used;
                    }
                }
            }
        }
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(ledger)),
            path,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, QuotaLedger> {
        // A poisoned mutex means another poller panicked mid-charge; the
        // count itself is a plain integer and still usable.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn charge(&self, units: u64) {
        // The write happens INSIDE the lock, deliberately: two pollers
        // charging concurrently must not interleave their writes, or the
        // slower snapshot overwrites the newer one and the persisted count
        // runs backwards — a restart would then resume below what Google has
        // already charged. The file is a few dozen bytes on the config
        // filesystem and charges arrive at most once per poll interval, so
        // the held-lock write is bounded and cheap; correctness of the spend
        // record wins over microseconds of contention.
        let mut ledger = self.lock();
        ledger.charge(units);
        if let Some(path) = &self.path {
            let snapshot = PersistedQuota {
                day: ledger.day,
                used: ledger.used,
            };
            // Atomic temp+rename: a crash mid-write must leave the previous
            // complete file, never a truncated one that silently resets the
            // budget to zero on the next start. Best-effort beyond that — a
            // full disk must not cost the chat session, and the in-memory
            // count still protects this run.
            if let Ok(text) = serde_json::to_string(&snapshot) {
                let tmp = path.with_extension("json.tmp");
                if std::fs::write(&tmp, text).is_ok() {
                    let _ = std::fs::rename(&tmp, path);
                }
            }
        }
    }

    /// Units left in today's estimate. Used by the tests, and by anything
    /// that wants the raw figure rather than [`Self::summary`]'s percentage.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn remaining(&self) -> u64 {
        self.lock().remaining()
    }

    /// The day's estimate, for showing on screen: units used, the limit, and
    /// the percentage of it spent.
    ///
    /// `None` when the limit is zero, which means the estimate is switched
    /// off and there is nothing honest to report.
    pub fn summary(&self) -> Option<(u64, u64, u8)> {
        let mut ledger = self.lock();
        ledger.roll_over_if_due();
        if ledger.limit == 0 {
            return None;
        }
        let percent = (ledger.used.saturating_mul(100) / ledger.limit).min(100) as u8;
        Some((ledger.used, ledger.limit, percent))
    }

    pub fn pause_reason(&self, reserve_percent: u8) -> Option<String> {
        self.lock().pause_reason(reserve_percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared store persists across sessions within one Pacific day and
    /// starts fresh on the next; two clones spend from the same count.
    #[test]
    fn the_quota_store_is_shared_and_persisted() {
        let scratch = crate::paths::test_support::ScratchConfigDir::new("quota-store");
        let path = scratch.path().join("quota.json");

        let store = QuotaStore::new(100, Some(path.clone()));
        let clone = store.clone();
        store.charge(30);
        clone.charge(20);
        assert_eq!(store.remaining(), 50, "clones spend from one count");

        // A new store (a new session) resumes today's count from disk.
        let resumed = QuotaStore::new(100, Some(path.clone()));
        assert_eq!(resumed.remaining(), 50);

        // A saved count from yesterday is discarded on load.
        let stale = PersistedQuota {
            day: pacific_day(Utc::now()) - 1,
            used: 90,
        };
        std::fs::write(&path, serde_json::to_string(&stale).unwrap()).unwrap();
        let fresh = QuotaStore::new(100, Some(path));
        assert_eq!(fresh.remaining(), 100, "yesterday's spend is not today's");
    }

    /// The daily budget refills at the Pacific midnight; the ledger must
    /// start over then instead of parking the session forever.
    #[test]
    fn the_quota_ledger_rolls_over_at_the_pacific_day_boundary() {
        let mut ledger = QuotaLedger::new(100);
        ledger.charge(100);
        assert_eq!(ledger.remaining(), 0);
        assert!(ledger.pause_reason(0).is_some(), "exhausted pauses");

        // Pretend the count belongs to yesterday: the next check refills.
        ledger.day -= 1;
        assert_eq!(ledger.remaining(), 100, "a new Pacific day starts fresh");
        assert!(ledger.pause_reason(10).is_none());
    }

    /// The fixed UTC−8 day arithmetic: one second before and after the
    /// boundary land on different days.
    #[test]
    fn pacific_day_changes_exactly_at_utc_minus_eight_midnight() {
        use chrono::TimeZone as _;
        // 08:00:00 UTC == 00:00:00 UTC−8.
        let boundary = Utc.with_ymd_and_hms(2026, 8, 15, 8, 0, 0).unwrap();
        assert_eq!(
            pacific_day(boundary) - 1,
            pacific_day(boundary - chrono::Duration::seconds(1))
        );
        assert_eq!(
            pacific_day(boundary),
            pacific_day(boundary + chrono::Duration::hours(23))
        );
    }

    #[test]
    fn the_quota_reserve_pauses_at_the_threshold_and_exhaustion_always_pauses() {
        let mut ledger = QuotaLedger::new(10_000);
        assert!(ledger.pause_reason(10).is_none());

        // Spend down to exactly the 10% reserve: 9000 used, 1000 remaining.
        ledger.charge(9_000);
        let reason = ledger.pause_reason(10).expect("the reserve must trip");
        assert!(
            reason.contains("sending"),
            "the reserve message must say sends still work: {reason}"
        );
        // One unit above the reserve does not trip.
        let mut above = QuotaLedger::new(10_000);
        above.charge(8_999);
        assert!(above.pause_reason(10).is_none());

        // With the reserve overridden (0%), only exhaustion pauses.
        assert!(ledger.pause_reason(0).is_none());
        ledger.charge(1_000);
        assert!(ledger.pause_reason(0).is_some());

        // A zero limit disables the ledger entirely.
        let mut unlimited = QuotaLedger::new(0);
        unlimited.charge(1_000_000);
        assert!(unlimited.pause_reason(10).is_none());
    }

    #[test]
    fn only_the_reserve_pause_offers_the_ctrl_r_override() {
        // The override clears the reserve and nothing else. When the whole
        // day's quota is gone the pause returns no matter what, so advertising
        // ctrl+r sent the user round a loop: each press re-resolved and polled
        // (spending real units) and then parked again immediately.
        let mut reserve_hit = QuotaLedger::new(10_000);
        reserve_hit.charge(9_000);
        let reserve = reserve_hit.pause_reason(10).expect("the reserve trips");
        assert!(reserve.contains("ctrl+r"), "reserve pause: {reserve}");

        let mut spent = QuotaLedger::new(10_000);
        spent.charge(10_000);
        let exhausted = spent.pause_reason(10).expect("exhaustion always pauses");
        assert!(
            !exhausted.contains("ctrl+r"),
            "exhaustion must not promise an override: {exhausted}"
        );
        // And with the reserve already overridden, the message is the same.
        assert!(!spent
            .pause_reason(0)
            .expect("still paused")
            .contains("ctrl+r"));
    }

    /// The reserve exists so that running out of *read* budget does not also
    /// take away the ability to send, which is the half of a chat client a
    /// stream owner cannot do without.
    #[test]
    fn the_reserve_pauses_before_the_budget_is_gone() {
        let mut ledger = QuotaLedger::new(1000);

        ledger.charge(700);
        assert!(
            ledger.pause_reason(10).is_none(),
            "300 of 1000 left is well above a 10% reserve"
        );

        ledger.charge(200);
        assert!(
            ledger.pause_reason(10).is_some(),
            "100 of 1000 left is at the reserve, so reading stops"
        );

        // A reserve of zero is somebody saying they want every unit.
        assert!(ledger.pause_reason(0).is_none());
    }

    /// A limit of zero means "do not estimate", not "everything is exhausted".
    #[test]
    fn a_zero_limit_never_pauses() {
        let mut ledger = QuotaLedger::new(0);
        ledger.charge(1_000_000);
        assert!(ledger.pause_reason(50).is_none());
    }
}
