# Grill-Me Decision 37: Governed Autonomous Data Acquisition

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai autonomously discovers, acquires, validates and versions research data only through administrator-approved data sources and connectors. Codex does not receive unrestricted network scraping authority and does not turn arbitrary Internet content into canonical research data.

When a Research Program requires data that is not available through the approved capability set, QuaZonai creates an Administration capability task rather than interrupting the normal Research Program workflow or asking the user to manually upload data as a recurring research step.

## Data Source Registry

QuaZonai maintains a versioned registry of approved research-data capabilities. A source entry records at least:

- `data_source_id`;
- source type and provider identity;
- connector version;
- supported datasets and fields;
- market and universe scope;
- licensing / usage classification;
- permitted research uses;
- required credentials or entitlement class;
- temporal availability semantics;
- revision policy;
- expected update cadence;
- quality and completeness expectations;
- enabled / disabled lifecycle state.

The registry is an Administration capability surface. It is not edited by Codex Missions.

## Autonomous acquisition flow

For an approved source that is already configured and requires no new human authorization:

```text
Research Mission identifies a data requirement
→ QuaZonai resolves matching approved Data Source / Connector
→ Connector acquires data
→ temporal, schema and quality validation
→ immutable Dataset Revision registration
→ Program continues automatically
```

Codex may ask QuaZonai structured research tools for available fields, time coverage, permitted uses and known limitations. Codex does not receive provider secrets.

## Capability gap flow

When no approved source can satisfy a material Research Charter requirement:

```text
DATA_CAPABILITY_REQUIRED
```

QuaZonai records:

- missing data domain;
- required fields and horizon;
- why the data is material to the hypothesis;
- known compatible source categories, when available;
- whether a new credential, license or administrator decision is required;
- affected Programs and Missions.

The affected Program may enter `BLOCKED` or `COOLING` depending on whether useful research can continue without the missing capability. Administration receives the configuration task. The ordinary user is not asked to perform repetitive data-engineering work inside the Research Program flow.

## Codex network boundary

Codex Mission workspaces must not use unrestricted network access to bypass the Data Source Registry.

Codex must not:

- execute arbitrary `curl`, `wget` or scraping workflows to create canonical data;
- accept new provider licensing terms;
- request or read provider API keys, passwords or tokens;
- install an unapproved downloader to bypass Connector governance;
- treat search-engine snippets, webpages or model recall as canonical market data;
- write directly into Sealed Evaluation datasets;
- infer that data is point-in-time safe merely because a provider returns a historical timestamp.

Internet or web research may be used for qualitative hypothesis formation when permitted by a Research Charter and product policy, but quantitative data entering the canonical Research Engine must pass through an approved Connector and Dataset Revision workflow.

## Dataset provenance and point-in-time semantics

Every canonical Dataset Revision records explicit business provenance and temporal fields, including at least:

- `data_source_id` and connector version;
- dataset revision number;
- provider or source record identity where applicable;
- event / observation time;
- real-world availability time when known;
- QuaZonai ingestion time;
- market / universe scope;
- field definitions and units;
- revision / restatement policy;
- licensing classification and allowed use;
- quality-validation result;
- look-ahead and point-in-time assessment;
- upstream dependencies.

QuaZonai must distinguish when an event occurred from when the information became knowable to a historical strategy. Revised fundamentals, corrected datasets, delayed publications and future-restated classifications cannot silently replace point-in-time values used in earlier Evaluation Episodes.

## Quality and leakage controls

Canonical ingestion performs domain-appropriate checks for:

- schema and type validity;
- timestamp ordering;
- duplicate observations;
- missingness and coverage;
- universe membership timing;
- revision and survivorship behavior;
- future information leakage;
- stale-source detection;
- unit and currency consistency where applicable.

A quality failure may block or invalidate a Dataset Revision, but it must not be interpreted as Alpha or Portfolio failure.

## Sealed Evaluation isolation

Discovery connectors and Codex tools do not expose Sealed Promotion data. Sealed Episodes use separately governed dataset allocations and access paths. A source being approved for Discovery does not by itself authorize Codex to read data reserved for Sealed Evaluation.

## User experience

The normal Research Program flow remains:

```text
Propose Idea
→ autonomous research
→ Candidate approval
```

When required data is available through approved capabilities, acquisition is invisible except for provenance and quality views in Research Observatory.

When an unavailable capability is essential, the Program clearly reports that research is blocked by a data capability rather than by negative evidence. Administration displays the one-time configuration task.

## Product boundaries

- Data capability configuration remains a low-frequency Administration responsibility.
- Codex does not become an unrestricted web scraper or credential holder.
- QuaZonai does not require the ordinary user to repeatedly upload research files.
- A new Data Source or Connector does not rewrite historical Dataset Revisions, Search Ledgers or Evidence Exposure.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.
