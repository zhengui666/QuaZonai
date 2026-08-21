//! QuaZonai native risk primitives.
//!
//! The Nautilus `ExecutionClientFactory` binding is a separate P0 spike. This crate
//! currently owns only deterministic gross-exposure arithmetic and does not define
//! another order, fill, position, or venue protocol.

pub mod reservation;

pub use reservation::{
    ExposureSnapshot, PusdMicros, ReservationError, ReservationRequest, reserve,
};
