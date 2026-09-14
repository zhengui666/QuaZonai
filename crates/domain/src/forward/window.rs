//! Select original complete corrected windows, never infer statistical qualification.
use super::*;
use contracts::{forward::*, science::NativeReturnV1, DbCounter, Id};
use std::collections::BTreeMap;

pub struct ForwardWindowSource {
    pub message: ForwardMessageViewV1,
    pub report: ForwardReportV1,
}
// Deliberately not Serialize: the public view must never expose original returns.
pub struct ForwardWindow {
    pub view: ForwardWindowViewV1,
    pub returns: Vec<NativeReturnV1>,
}
pub fn window(
    handoff: Id,
    stream: &str,
    sources: &[ForwardWindowSource],
) -> Result<ForwardWindow, DomainError> {
    text(stream, 1, 200, false)?;
    if sources.len() > 10000 {
        return Err(DomainError::CapabilityUnavailable("forward_window_limit"));
    }
    let mut groups: BTreeMap<u64, Vec<&ForwardWindowSource>> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut points = 0usize;
    for source in sources {
        let m = &source.message;
        let r = &source.report;
        let c = &r.content;
        text(&m.external_message_id, 1, 200, false)?;
        super::report(c)?;
        points = points
            .checked_add(c.returns.len())
            .ok_or(DomainError::Invalid("forward_window_limit"))?;
        if points > 1000000 {
            return Err(DomainError::CapabilityUnavailable("forward_window_limit"));
        }
        let coverage = if c.supersedes_message_id.is_some() {
            ForwardCoverageV1::Correction
        } else if c.complete {
            ForwardCoverageV1::Complete
        } else {
            ForwardCoverageV1::Partial
        };
        if !ids.insert(m.id)
            || m.handoff_id != handoff
            || c.handoff_id != handoff
            || m.stream_id != stream
            || c.stream_id != stream
            || m.project_id != c.project_id
            || m.downstream_id != r.downstream_id
            || m.release_id != r.release_id
            || m.sequence != c.sequence
            || m.message_revision != c.message_revision
            || m.supersedes_message_id != c.supersedes_message_id
            || m.window_start != c.window_start
            || m.window_end != c.window_end
            || m.issued_at != c.issued_at
            || m.coverage_status != coverage
            || m.observation_count.get()
                != c.returns.iter().filter(|p| p.value.is_some()).count() as u64
            || m.window_end > m.received_at
            || m.issued_at > m.received_at + chrono::Duration::seconds(5)
        {
            return Err(DomainError::Invalid("forward_source_binding"));
        }
        if let Some(first) = sources.first() {
            if m.project_id != first.message.project_id
                || m.release_id != first.message.release_id
                || m.downstream_id != first.message.downstream_id
                || r.environment != first.report.environment
                || c.external_claim_id != first.report.content.external_claim_id
            {
                return Err(DomainError::Invalid("forward_source_binding"));
            }
        }
        groups.entry(m.sequence.get()).or_default().push(source);
    }
    let mut selected = Vec::new();
    for versions in groups.values_mut() {
        versions.sort_by_key(|s| s.message.message_revision);
        if versions[0].message.message_revision != 1
            || versions[0].message.supersedes_message_id.is_some()
        {
            return Err(DomainError::Invalid("forward_original_chain"));
        }
        for pair in versions.windows(2) {
            let (old, new) = (&pair[0].message, &pair[1].message);
            if u64::from(new.message_revision) != u64::from(old.message_revision) + 1
                || new.supersedes_message_id != Some(old.id)
                || new.window_start != old.window_start
                || new.window_end != old.window_end
            {
                return Err(DomainError::Invalid("forward_original_chain"));
            }
        }
        selected.push(
            *versions
                .last()
                .ok_or(DomainError::Invalid("forward_original_chain"))?,
        );
    }
    let mut reasons = BTreeSet::new();
    if selected.is_empty() {
        reasons.insert(ForwardWindowReasonV1::NoMessages);
    }
    let mut next_sequence = 1;
    let mut previous_end = None;
    let mut timestamps = BTreeSet::new();
    let mut returns = Vec::new();
    for source in &selected {
        let m = &source.message;
        let r = &source.report.content;
        if m.sequence.get() != next_sequence {
            reasons.insert(ForwardWindowReasonV1::SequenceGap);
        }
        next_sequence = m.sequence.get() + 1;
        if !r.complete {
            reasons.insert(ForwardWindowReasonV1::Partial);
        }
        if r.returns.iter().any(|p| p.value.is_none()) {
            reasons.insert(ForwardWindowReasonV1::MissingReturns);
        }
        if let Some(end) = previous_end {
            if m.window_start > end {
                reasons.insert(ForwardWindowReasonV1::WindowGap);
            }
            if m.window_start < end {
                reasons.insert(ForwardWindowReasonV1::WindowOverlap);
            }
        }
        previous_end = Some(m.window_end);
        for point in &r.returns {
            if !timestamps.insert(point.timestamp_ns) {
                reasons.insert(ForwardWindowReasonV1::SampleOverlap);
            }
            returns.push(point.clone());
        }
    }
    let is_contiguous = reasons.is_empty();
    if !is_contiguous {
        returns.clear();
    }
    Ok(ForwardWindow {
        view: ForwardWindowViewV1 {
            handoff_id: handoff,
            stream_id: stream.into(),
            latest_message_ids: selected.iter().map(|s| s.message.id).collect(),
            window_start: selected.iter().map(|s| s.message.window_start).min(),
            window_end: selected.iter().map(|s| s.message.window_end).max(),
            complete_observations: DbCounter::new(returns.len() as u64)
                .map_err(|_| DomainError::Invalid("forward_window_limit"))?,
            is_contiguous,
            reason_codes: reasons.into_iter().collect(),
        },
        returns,
    })
}
