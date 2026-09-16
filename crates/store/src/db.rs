//! Native row decoding; these helpers grant no authority or database access.
use crate::StoreError;
use contracts::{Id, Revision};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

pub(crate) fn id(value: Uuid) -> Result<Id, StoreError> {
    Id::try_from(value.to_string()).map_err(|_| StoreError::Integrity)
}
pub(crate) fn revision(value: i64) -> Result<Revision, StoreError> {
    Revision::try_from(value.to_string()).map_err(|_| StoreError::Integrity)
}
pub(crate) fn optional_id(row: &PgRow, name: &str) -> Result<Option<Id>, StoreError> {
    row.try_get::<Option<Uuid>, _>(name)?.map(id).transpose()
}
pub(crate) fn enum_value<T: DeserializeOwned>(row: &PgRow, name: &str) -> Result<T, StoreError> {
    serde_json::from_value(serde_json::Value::String(row.try_get(name)?))
        .map_err(|_| StoreError::Integrity)
}
pub(crate) fn json<T: Serialize>(value: &T) -> Result<serde_json::Value, StoreError> {
    serde_json::to_value(value).map_err(|_| StoreError::Integrity)
}
pub(crate) fn code<T: Serialize>(value: &T) -> Result<String, StoreError> {
    json(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or(StoreError::Integrity)
}
