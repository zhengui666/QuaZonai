//! Shared text shape for native scientific identifiers and diagnostic fields.
use crate::DomainError;

pub fn name(value: &str) -> Result<(), DomainError> {
    text(value, 1, 120, false)
}

pub fn text(value: &str, min: usize, max: usize, multiline: bool) -> Result<(), DomainError> {
    let count = value.chars().count();
    if count < min
        || count > max
        || (min > 0 && value.trim().is_empty())
        || value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\t' | '\r')))
    {
        return Err(DomainError::Invalid("text"));
    }
    Ok(())
}
