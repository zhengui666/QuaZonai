# Dataset registration evidence read

Provide an owner-only, Dataset-bound read-only evidence summary for native registrations.
Generic artifact authorization remains unchanged. Reject Sealed before native I/O;
no arbitrary artifact/path input, raw documents, market values or new eligibility.
Preserve registered origin/PIT and current license state. Validate metadata/quality
schema, artifact kind, immutable source/revision association and original quality
agreement, then expose typed references and registration-time measurement metadata only.
Do not return free-text provenance explanations or native source paths/URLs.

Candidate only. No remote Git, deployment, database migration or production data edits.
Validation and generated outputs must be recorded separately; fixture success does
not validate any user's native data or prove historical availability.
