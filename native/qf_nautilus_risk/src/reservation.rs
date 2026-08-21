//! Integer micro-pUSD central reservation arithmetic.

use core::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct PusdMicros(i128);

impl PusdMicros {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(value: i128) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> i128 {
        self.0
    }

    fn checked_add(self, other: Self) -> Result<Self, ReservationError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(ReservationError::ArithmeticOverflow)
    }

    fn is_negative(self) -> bool {
        self.0 < 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExposureSnapshot {
    pub position_entry_cost: PusdMicros,
    pub increasing_open_orders: PusdMicros,
    pub pending_reservations: PusdMicros,
}

impl ExposureSnapshot {
    pub fn gross(self) -> Result<PusdMicros, ReservationError> {
        validate_non_negative(self.position_entry_cost)?;
        validate_non_negative(self.increasing_open_orders)?;
        validate_non_negative(self.pending_reservations)?;
        self.position_entry_cost
            .checked_add(self.increasing_open_orders)?
            .checked_add(self.pending_reservations)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservationRequest {
    pub current: ExposureSnapshot,
    pub requested_increase: PusdMicros,
    pub gross_limit: PusdMicros,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservationError {
    NegativeAmount,
    ArithmeticOverflow,
    LimitExceeded {
        projected: PusdMicros,
        limit: PusdMicros,
    },
}

impl fmt::Display for ReservationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NegativeAmount => formatter.write_str("risk amount must not be negative"),
            Self::ArithmeticOverflow => formatter.write_str("risk arithmetic overflow"),
            Self::LimitExceeded { projected, limit } => write!(
                formatter,
                "projected gross exposure {} exceeds limit {}",
                projected.value(),
                limit.value()
            ),
        }
    }
}

impl std::error::Error for ReservationError {}

pub fn reserve(request: ReservationRequest) -> Result<PusdMicros, ReservationError> {
    validate_non_negative(request.requested_increase)?;
    validate_non_negative(request.gross_limit)?;
    let projected = request
        .current
        .gross()?
        .checked_add(request.requested_increase)?;
    if projected > request.gross_limit {
        return Err(ReservationError::LimitExceeded {
            projected,
            limit: request.gross_limit,
        });
    }
    Ok(projected)
}

fn validate_non_negative(value: PusdMicros) -> Result<(), ReservationError> {
    if value.is_negative() {
        return Err(ReservationError::NegativeAmount);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_within_gross_limit() {
        let projected = reserve(ReservationRequest {
            current: ExposureSnapshot {
                position_entry_cost: PusdMicros::new(40_000_000),
                increasing_open_orders: PusdMicros::new(20_000_000),
                pending_reservations: PusdMicros::new(10_000_000),
            },
            requested_increase: PusdMicros::new(25_000_000),
            gross_limit: PusdMicros::new(100_000_000),
        })
        .expect("reservation should fit");
        assert_eq!(projected, PusdMicros::new(95_000_000));
    }

    #[test]
    fn rejects_limit_excess() {
        let error = reserve(ReservationRequest {
            current: ExposureSnapshot {
                position_entry_cost: PusdMicros::new(90_000_000),
                ..ExposureSnapshot::default()
            },
            requested_increase: PusdMicros::new(25_000_000),
            gross_limit: PusdMicros::new(100_000_000),
        })
        .expect_err("reservation should exceed limit");
        assert!(matches!(error, ReservationError::LimitExceeded { .. }));
    }

    #[test]
    fn rejects_negative_inputs() {
        let error = reserve(ReservationRequest {
            current: ExposureSnapshot::default(),
            requested_increase: PusdMicros::new(-1),
            gross_limit: PusdMicros::new(100_000_000),
        })
        .expect_err("negative values are invalid");
        assert_eq!(error, ReservationError::NegativeAmount);
    }
}
