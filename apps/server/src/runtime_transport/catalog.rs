//! Bounded native catalog metadata over the existing deployment-approved TLS transport.
use super::{RuntimeRequestError, RuntimeTransport};
use chrono::Utc;
use contracts::catalogs::RuntimeCatalogMetadataV1;
use reqwest::StatusCode;

/// Exact native document retained for audit. No Debug for an untrusted remote body.
pub struct ReceivedCatalogMetadata {
    pub metadata: RuntimeCatalogMetadataV1,
    pub raw_document: Vec<u8>,
}

impl RuntimeTransport {
    pub async fn catalog_metadata(
        &self,
        registered_ref: &str,
        storage_version: &str,
    ) -> Result<ReceivedCatalogMetadata, RuntimeRequestError> {
        domain::data::registry_key(registered_ref).map_err(|_| RuntimeRequestError::Contract)?;
        domain::runtime_jobs::storage_version(storage_version)
            .map_err(|_| RuntimeRequestError::Contract)?;
        let mut url = self.resource(&["catalogs", registered_ref, "metadata"])?;
        url.query_pairs_mut()
            .append_pair("storage_version", storage_version);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| RuntimeRequestError::Unavailable)?;
        let (metadata, raw_document): (RuntimeCatalogMetadataV1, _) = self
            .json_response(response, &[StatusCode::OK], 1024 * 1024)
            .await?;
        if metadata.registered_ref != registered_ref || metadata.storage_version != storage_version
        {
            return Err(RuntimeRequestError::Contract);
        }
        domain::catalogs::metadata(&metadata, Utc::now())
            .map_err(|_| RuntimeRequestError::Contract)?;
        Ok(ReceivedCatalogMetadata {
            metadata,
            raw_document,
        })
    }
}
