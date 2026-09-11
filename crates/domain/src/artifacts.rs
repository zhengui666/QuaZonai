//! Validation of untrusted research text; never a scientific evidence gate.
use crate::{research::invalid, DomainError};
use contracts::{artifacts::*, DbCounter};

pub fn upload(request: &ArtifactCreate) -> Result<DbCounter, DomainError> {
    let bytes = request.content.len();
    if bytes == 0 || bytes > MAX_UPLOAD_BYTES || request.content.trim().is_empty() {
        return Err(invalid("content", "ARTIFACT_SIZE"));
    }
    if request.content.contains('\0') {
        return Err(invalid("content", "ARTIFACT_TEXT"));
    }
    if request.kind != ResearchArtifactKind::Code {
        let document: serde_json::Value = serde_json::from_str(&request.content)
            .map_err(|_| invalid("content", "ARTIFACT_DOCUMENT"))?;
        if !document.is_object() || document.get("schema_version") != Some(&serde_json::json!(1)) {
            return Err(invalid("content", "ARTIFACT_DOCUMENT"));
        }
    }
    DbCounter::new(bytes as u64).map_err(|_| invalid("content", "ARTIFACT_SIZE"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::{Id, SchemaV1};

    fn request(kind: ResearchArtifactKind, content: &str) -> ArtifactCreate {
        ArtifactCreate {
            schema_version: SchemaV1,
            project_id: Id::new(),
            kind,
            content: content.into(),
        }
    }

    #[test]
    fn counts_utf8_bytes_and_requires_an_actual_versioned_document() {
        assert_eq!(
            upload(&request(ResearchArtifactKind::Code, "// 中文"))
                .unwrap()
                .get(),
            9
        );
        for kind in [
            ResearchArtifactKind::Parameters,
            ResearchArtifactKind::Report,
        ] {
            assert!(upload(&request(
                kind,
                " {\"schema_version\":1,\"text\":\"untrusted\"}\n"
            ))
            .is_ok());
            for invalid in [
                "{}",
                "[]",
                "null",
                "not json",
                "{\"schema_version\":2}",
                "{\"schema_version\":\"1\"}",
            ] {
                assert!(upload(&request(kind, invalid)).is_err());
            }
        }
    }

    #[test]
    fn rejects_empty_binary_and_oversized_content_without_compiling_source() {
        for content in ["", "\n\t ", "source\0"] {
            assert!(upload(&request(ResearchArtifactKind::Code, content)).is_err());
        }
        let accepted = "x".repeat(MAX_UPLOAD_BYTES);
        assert!(upload(&request(ResearchArtifactKind::Code, &accepted)).is_ok());
        let rejected = "界".repeat(MAX_UPLOAD_BYTES / 3 + 1);
        assert!(upload(&request(ResearchArtifactKind::Code, &rejected)).is_err());
        assert!(upload(&request(
            ResearchArtifactKind::Code,
            "deliberately not valid Rust; validate in the isolated job"
        ))
        .is_ok());
    }
}
