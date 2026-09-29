use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub const JOB_TYPE_COMPACT_KNOWLEDGE: &str = "compact_knowledge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl BatchStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchTrigger {
    Scheduled,
    Manual,
    Retry,
}

impl BatchTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
            Self::Retry => "retry",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "scheduled" => Some(Self::Scheduled),
            "manual" => Some(Self::Manual),
            "retry" => Some(Self::Retry),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompactionBatch {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub agent_name: Option<String>,
    pub run_id: Option<Uuid>,
    pub status: BatchStatus,
    pub trigger: BatchTrigger,
    pub cycle_id: Uuid,
    pub cycle_started_at: OffsetDateTime,
    pub ticket_count: i32,
    pub candidate_count: Option<i32>,
    pub summary: Option<String>,
    pub error_message: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub started_at: Option<OffsetDateTime>,
    pub ended_at: Option<OffsetDateTime>,
}

/// A new scheduled cycle starts one interval after the later of the previous
/// cycle start and the oldest waiting ticket, so a lone fresh ticket waits a
/// full interval while an old backlog compacts right away.
pub fn next_scheduled_at(
    last_cycle_started_at: Option<OffsetDateTime>,
    oldest_eligible_enqueued_at: Option<OffsetDateTime>,
    interval: Duration,
) -> Option<OffsetDateTime> {
    let oldest = oldest_eligible_enqueued_at?;
    let anchor = match last_cycle_started_at {
        Some(last) if last > oldest => last,
        _ => oldest,
    };
    Some(anchor + interval)
}

/// Keep queue order and stop at the first ticket that would overflow the byte
/// budget. The first ticket is always taken; its context is truncated later.
pub fn select_within_bytes(candidates: &[(Uuid, i64)], max_bytes: usize) -> Vec<Uuid> {
    let max_bytes = i64::try_from(max_bytes).unwrap_or(i64::MAX);
    let mut total: i64 = 0;
    let mut selected = Vec::new();
    for (ticket_id, bytes) in candidates {
        let bytes = (*bytes).max(0);
        if !selected.is_empty() && total.saturating_add(bytes) > max_bytes {
            break;
        }
        total = total.saturating_add(bytes);
        selected.push(*ticket_id);
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_and_triggers_round_trip() {
        for status in [
            BatchStatus::Queued,
            BatchStatus::Running,
            BatchStatus::Succeeded,
            BatchStatus::Failed,
        ] {
            assert_eq!(BatchStatus::parse(status.as_str()), Some(status));
        }
        for trigger in [
            BatchTrigger::Scheduled,
            BatchTrigger::Manual,
            BatchTrigger::Retry,
        ] {
            assert_eq!(BatchTrigger::parse(trigger.as_str()), Some(trigger));
        }
    }

    #[test]
    fn next_scheduled_at_waits_from_the_later_anchor() {
        let interval = Duration::minutes(30);
        let old = OffsetDateTime::from_unix_timestamp(1_788_000_000).unwrap();
        let recent = old + Duration::hours(2);
        assert_eq!(next_scheduled_at(None, None, interval), None);
        assert_eq!(next_scheduled_at(Some(recent), None, interval), None);
        assert_eq!(
            next_scheduled_at(None, Some(old), interval),
            Some(old + interval)
        );
        assert_eq!(
            next_scheduled_at(Some(recent), Some(old), interval),
            Some(recent + interval)
        );
        assert_eq!(
            next_scheduled_at(Some(old), Some(recent), interval),
            Some(recent + interval)
        );
    }

    #[test]
    fn select_within_bytes_keeps_order_and_always_takes_one() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        assert_eq!(
            select_within_bytes(&[(a, 40), (b, 50), (c, 5)], 100),
            vec![a, b, c]
        );
        assert_eq!(
            select_within_bytes(&[(a, 60), (b, 50), (c, 5)], 100),
            vec![a]
        );
        assert_eq!(select_within_bytes(&[(a, 500), (b, 1)], 100), vec![a]);
        assert!(select_within_bytes(&[], 100).is_empty());
    }
}
