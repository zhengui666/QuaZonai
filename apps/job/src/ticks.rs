//! Original native data adapters only. Derived cohort features are retired.
pub mod catalog;
pub mod economic;

/// Bound positive source-clock age; unsynchronized venue clocks may be ahead.
pub fn source_age_within(event: u64, received: u64, maximum: u64) -> bool {
    received.checked_sub(event).is_none_or(|age| age <= maximum)
}
