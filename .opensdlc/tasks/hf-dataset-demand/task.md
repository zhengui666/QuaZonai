# On-demand Hugging Face dataset acquisition

## Request and scope

Requested on 2026-10-05: move already prepared source data to Hugging Face to
free local storage, and support Hugging Face data-source plugins with on-demand
downloads. This slice implements acquisition in the existing operator source
entrypoint; uploading, deletion and account setup are separate authorized work.

First acquisition delivery: `efd0057b99e3c4333da899334c330039c6332611` in
`zhengui666/QuaZonai`, [PR167](https://github.com/zhengui666/QuaZonai/pull/167).
Its temporary base is the native strategy-paper branch at
`6139e5421cfb45aebfd56d5d65510103ddf6c428` and it depends on
[PR166](https://github.com/zhengui666/QuaZonai/pull/166). Keep feature changes
separate from the release repair. A final `dev` delivery needs that dependency
merged or an explicit correct rebase; the temporary base does not prove independence.

## Implementation

- [Plugin and index contract](../../../runtimes/data/hf-dataset.md)
- [Request selection and cache handoff](../../../runtimes/data/hf_dataset.py)
- [Shared Hub metadata and resumable transport](../../../runtimes/data/snapshot.py)
- [Installed dispatcher checks](../../../deploy/docker/hf_source_test.py)

Use actual repository partitions and half-open market/date selection, ordinary
revision refs, shared completed files, resumable partial bytes and atomic final
publication. Do not infer coverage or calculate file checksums on the new path.
Existing source-plugin manifests and adapters remain compatible.

## Acceptance evidence

Run the existing and new data tests, the scoped installed-manager tests, plugin
inventory/help and patch application checks. Independent review must examine
the final slice. Installed dispatch uses the real registry; mock Docker metadata
checks are not a built container run or live Hub/native-research acceptance.

## Explicit production bridge and remaining acceptance

The native history converter now has a separate explicit `--selection` branch
for acquired cache Parquet files; the original `--snapshot` path stays compatible.
Python `convert`/`prepare` reuse native row conversion and final catalog-metadata
handoffs without computing new file/instruments checksums. Real partition facts,
original definitions, source clock meaning and ordinary mutation observations
remain explicit. The single pinned native library target and independent review
must actually pass; mocked Python publications alone are not native acceptance.

Actual Runtime/catalog registration and existing source/dataset/frozen-input
validation still follow the returned metadata handoff. Preserve scientific jobs'
offline boundary and never download after input freezing. Do not call the full
research chain complete before actual registration, fresh validation and execution.
