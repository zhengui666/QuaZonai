// QuaZonai modification, 2026-09-27. Licensed under LGPL-3.0-only, as this crate.
//! A definition update carried by the existing native data iterator and clocks.

use std::{any::Any, sync::Arc};

use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{CustomData, CustomDataTrait, Data, HasTsInit},
    instruments::{Instrument, InstrumentAny},
};
use serde::{Deserialize, Serialize};

/// Replay an original definition at its receive time, without replacing its matching engine.
/// Callers must validate which economic changes their strategy and dataset support.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstrumentUpdate(pub InstrumentAny);

impl HasTsInit for InstrumentUpdate {
    fn ts_init(&self) -> UnixNanos {
        Instrument::ts_init(&self.0)
    }
}

impl CustomDataTrait for InstrumentUpdate {
    fn type_name(&self) -> &'static str {
        Self::type_name_static()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn ts_event(&self) -> UnixNanos {
        self.0.ts_event()
    }

    fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    fn clone_arc(&self) -> Arc<dyn CustomDataTrait> {
        Arc::new(self.clone())
    }

    fn eq_arc(&self, other: &dyn CustomDataTrait) -> bool {
        other.as_any().downcast_ref::<Self>().is_some_and(|other| {
            self.to_json()
                .is_ok_and(|value| other.to_json().is_ok_and(|v| v == value))
        })
    }

    fn from_json(value: serde_json::Value) -> anyhow::Result<Arc<dyn CustomDataTrait>> {
        Ok(Arc::new(serde_json::from_value::<Self>(value)?))
    }
}

impl From<InstrumentUpdate> for Data {
    fn from(value: InstrumentUpdate) -> Self {
        Self::Custom(CustomData::from_arc(Arc::new(value)))
    }
}
