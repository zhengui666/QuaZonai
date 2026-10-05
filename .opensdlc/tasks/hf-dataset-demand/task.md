# On-demand Hugging Face dataset acquisition

## Request and scope

Requested on 2026-10-05: move already prepared source data to Hugging Face to
free local storage, and support Hugging Face data-source plugins with on-demand
downloads. This slice implements acquisition in the existing operator source
entrypoint; uploading, deletion and account setup are separate authorized work.

Delivery base: `6139e5421cfb45aebfd56d5d65510103ddf6c428` in
`zhengui666/QuaZonai`. Keep this feature separate from the current release fix
and its CI. The delivery PR is to be linked when the independent branch exists.

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

## Remaining production bridge

The existing native history converter accepts its original `snapshot.json`,
not the new `selection.json`. A subsequent thin explicit pre-research adapter
must connect acquired files to native conversion/preparation, actual catalog
registration and existing source/dataset/frozen-input validation. Preserve the
existing scientific job's offline boundary and never download after input
freezing. This acquisition slice alone does not complete that research chain.
