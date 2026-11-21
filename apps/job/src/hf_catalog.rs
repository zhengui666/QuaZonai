//! Public Hugging Face Hub directory discovery for the stock Nautilus HTTP Parquet catalog.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    ops::Range,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_util::{
    StreamExt, TryStreamExt,
    stream::{self, BoxStream},
};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use object_store::{
    CopyOptions, GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, ObjectStore,
    PutMultipartOptions, PutOptions, PutPayload, PutResult, path::Path as ObjectPath,
};
use reqwest::{Client, StatusCode, Url, header};
use serde::Deserialize;
use tokio::sync::OnceCell;

const ORIGIN: &str = "https://huggingface.co";
const STORE: &str = "Hugging Face Hub";

#[derive(Clone, Debug)]
struct HubDataset {
    owner: String,
    repo: String,
    commit: String,
}

impl HubDataset {
    fn parse(uri: &str) -> Result<Self> {
        let path = uri
            .strip_prefix("hf://datasets/")
            .context("Expected hf://datasets/<owner>/<repo>@<commit>")?;
        let (owner, path) = path.split_once('/').context("Missing dataset repository")?;
        let (repo, commit) = path.split_once('@').context("Missing Hub dataset commit")?;
        let slug = |s: &str| {
            s.bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                && !s.contains("..")
                && !s.contains("--")
                && !s.ends_with('.')
                && !s.ends_with('-')
        };
        ensure!(slug(owner) && slug(repo), "Invalid Hub dataset name");
        ensure!(
            commit.len() == 40
                && commit
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            "Hub catalog must specify an immutable 40-character commit"
        );
        Ok(Self {
            owner: owner.into(),
            repo: repo.into(),
            commit: commit.into(),
        })
    }

    fn resolve_prefix(&self) -> String {
        format!(
            "datasets/{}/{}/resolve/{}",
            self.owner, self.repo, self.commit
        )
    }

    fn tree_url(&self, relative: &str, recursive: bool) -> Result<Url> {
        let mut url = Url::parse(ORIGIN)?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| anyhow::anyhow!("Invalid Hub origin"))?;
            path.pop_if_empty().extend([
                "api",
                "datasets",
                &self.owner,
                &self.repo,
                "tree",
                &self.commit,
            ]);
            if !relative.is_empty() {
                path.extend(relative.split('/'));
            }
        }
        url.query_pairs_mut()
            .append_pair("recursive", if recursive { "true" } else { "false" });
        Ok(url)
    }
}

/// Validates the public immutable dataset reference without accessing the network.
pub fn validate_uri(uri: &str) -> Result<()> {
    HubDataset::parse(uri).map(|_| ())
}

/// Opens the official Nautilus Parquet catalog with Hub discovery and its native HTTP reader.
pub fn open(uri: &str) -> Result<ParquetDataCatalog> {
    let dataset = HubDataset::parse(uri)?;
    let root = ObjectPath::parse(dataset.resolve_prefix())?;
    let mut catalog =
        ParquetDataCatalog::from_uri(&format!("{ORIGIN}/{root}"), None, None, None, None)?;
    let metadata = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    catalog.object_store = Arc::new(HubStore {
        dataset,
        root,
        metadata,
        listings: Arc::default(),
        http: catalog.object_store.clone(),
    });
    register_currencies(&catalog)?;
    Ok(catalog)
}

fn register_currencies(catalog: &ParquetDataCatalog) -> Result<()> {
    let path = ObjectPath::parse(format!("{}/currencies.json", catalog.base_path))?;
    catalog.execute_async(|| async {
        match catalog
            .object_store
            .get_opts(&path, GetOptions::default())
            .await
        {
            Ok(response) => crate::catalog::currencies::register_json(&response.bytes().await?),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    })
}

#[derive(Clone, Debug)]
struct HubStore {
    dataset: HubDataset,
    root: ObjectPath,
    metadata: Client,
    listings: Arc<Mutex<Listings>>,
    http: Arc<dyn ObjectStore>,
}

// The stock catalog's forks clone this ObjectStore. Share exact complete metadata
// across those forks, never across separate opens or on disk.
type Listings = HashMap<(ObjectPath, bool), Arc<OnceCell<Arc<ListResult>>>>;

impl fmt::Display for HubStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", STORE, self.root)
    }
}

fn hub_error(source: anyhow::Error) -> object_store::Error {
    object_store::Error::Generic {
        store: STORE,
        source: source.into_boxed_dyn_error(),
    }
}

fn read_only() -> object_store::Error {
    object_store::Error::NotSupported {
        source: "The public Hub catalog is read-only".into(),
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum HubEntry {
    File { path: String, size: u64 },
    Directory { path: String },
}

/// Git tree entries have no object mtime unless Hub expands every row. The
/// pinned revision supplies identity, not an invented HTTP ETag or object version.
fn listed_file(location: ObjectPath, size: u64) -> ObjectMeta {
    ObjectMeta {
        location,
        last_modified: DateTime::<Utc>::UNIX_EPOCH,
        size,
        e_tag: None,
        version: None,
    }
}

fn descendant_or_self(key: &ObjectPath, prefix: &ObjectPath) -> bool {
    key == prefix || key.prefix_match(prefix).is_some()
}

fn link_next(link: &str, current: &Url) -> Result<Option<Url>> {
    for item in link.split(',') {
        let (target, params) = item
            .trim()
            .split_once('>')
            .context("Invalid Hub pagination link")?;
        if !params
            .split(';')
            .map(str::trim)
            .any(|param| matches!(param, "rel=\"next\"" | "rel=next"))
        {
            continue;
        }
        let next = current.join(
            target
                .strip_prefix('<')
                .context("Invalid Hub pagination URL")?,
        )?;
        ensure!(
            next.origin() == current.origin()
                && next.path() == current.path()
                && next.username().is_empty()
                && next.password().is_none()
                && next.fragment().is_none()
                && next.query_pairs().find(|(key, _)| key == "recursive")
                    == current.query_pairs().find(|(key, _)| key == "recursive")
                && next != *current,
            "Hub pagination changed catalog scope or did not advance"
        );
        return Ok(Some(next));
    }
    Ok(None)
}

impl HubStore {
    fn tree_url(&self, prefix: Option<&ObjectPath>, recursive: bool) -> Result<Url> {
        let prefix = prefix.filter(|p| !p.is_root()).unwrap_or(&self.root);
        ensure!(
            descendant_or_self(prefix, &self.root),
            "Hub listing outside bound catalog"
        );
        let relative = prefix
            .as_ref()
            .strip_prefix(self.root.as_ref())
            .context("Missing Hub key prefix")?;
        self.dataset
            .tree_url(relative.trim_start_matches('/'), recursive)
    }

    fn object_path(&self, path: &str, listing_prefix: &ObjectPath) -> Result<ObjectPath> {
        let relative = ObjectPath::parse(path)?;
        ensure!(
            !relative.is_root() && relative.as_ref() == path,
            "Invalid Hub repository path"
        );
        let location = ObjectPath::parse(format!("{}/{relative}", self.root))?;
        ensure!(
            location != *listing_prefix && descendant_or_self(&location, listing_prefix),
            "Hub returned a path outside the listed directory"
        );
        Ok(location)
    }

    async fn page(&self, url: &Url, first: bool) -> Result<(Option<Url>, Vec<HubEntry>)> {
        let response = self.metadata.get(url.clone()).send().await?;
        if first
            && response.status() == StatusCode::NOT_FOUND
            && response
                .headers()
                .get("x-error-code")
                .and_then(|value| value.to_str().ok())
                == Some("EntryNotFound")
        {
            // A catalog can legitimately omit an optional data family. A missing
            // repository/revision or an unclassified HTTP error is not an empty list.
            return Ok((None, Vec::new()));
        }
        ensure!(
            response.status().is_success(),
            "Hub tree request returned {}",
            response.status()
        );
        let next = response
            .headers()
            .get(header::LINK)
            .map(|value| link_next(value.to_str()?, url))
            .transpose()?
            .flatten();
        Ok((next, response.json().await?))
    }

    fn prefix(&self, prefix: Option<&ObjectPath>) -> ObjectPath {
        prefix
            .filter(|p| !p.is_root())
            .unwrap_or(&self.root)
            .clone()
    }

    fn ready_recursive(
        &self,
        prefix: &ObjectPath,
    ) -> object_store::Result<Option<Arc<ListResult>>> {
        if !descendant_or_self(prefix, &self.root) {
            return Err(hub_error(anyhow::anyhow!(
                "Hub listing outside bound catalog"
            )));
        }
        let listings = self
            .listings
            .lock()
            .map_err(|_| hub_error(anyhow::anyhow!("Hub metadata cache lock is poisoned")))?;
        let mut candidate = prefix.clone();
        loop {
            if let Some(listing) = listings
                .get(&(candidate.clone(), true))
                .and_then(|slot| slot.get())
            {
                return Ok(Some(listing.clone()));
            }
            if candidate == self.root {
                return Ok(None);
            }
            let (parent, _) = candidate
                .as_ref()
                .rsplit_once('/')
                .ok_or_else(|| hub_error(anyhow::anyhow!("Hub listing outside bound catalog")))?;
            candidate = ObjectPath::parse(parent).map_err(|error| hub_error(error.into()))?;
        }
    }

    async fn listing(
        &self,
        prefix: &ObjectPath,
        recursive: bool,
    ) -> object_store::Result<Arc<ListResult>> {
        let slot = self
            .listings
            .lock()
            .map_err(|_| hub_error(anyhow::anyhow!("Hub metadata cache lock is poisoned")))?
            .entry((prefix.clone(), recursive))
            .or_default()
            .clone();
        let listing = slot
            .get_or_try_init(|| async {
                let mut next = Some(self.tree_url(Some(prefix), recursive).map_err(hub_error)?);
                let mut seen = HashSet::new();
                let mut result = ListResult {
                    common_prefixes: Vec::new(),
                    objects: Vec::new(),
                };
                while let Some(url) = next {
                    let first = seen.is_empty();
                    if !seen.insert(url.clone()) {
                        return Err(hub_error(anyhow::anyhow!("Repeated Hub pagination cursor")));
                    }
                    let (following, entries) = self.page(&url, first).await.map_err(hub_error)?;
                    for entry in entries {
                        match entry {
                            HubEntry::File { path, size } => result.objects.push(listed_file(
                                self.object_path(&path, prefix).map_err(hub_error)?,
                                size,
                            )),
                            HubEntry::Directory { path } if !recursive => result
                                .common_prefixes
                                .push(self.object_path(&path, prefix).map_err(hub_error)?),
                            HubEntry::Directory { .. } => {}
                        }
                    }
                    next = following;
                }
                if recursive {
                    result.objects.sort_by(|a, b| a.location.cmp(&b.location));
                }
                Ok(Arc::new(result))
            })
            .await?;
        Ok(listing.clone())
    }
}

#[async_trait]
impl ObjectStore for HubStore {
    async fn get_opts(
        &self,
        location: &ObjectPath,
        options: GetOptions,
    ) -> object_store::Result<GetResult> {
        ensure_in_root(location, &self.root)?;
        self.http.get_opts(location, options).await
    }

    async fn get_ranges(
        &self,
        location: &ObjectPath,
        ranges: &[Range<u64>],
    ) -> object_store::Result<Vec<Bytes>> {
        ensure_in_root(location, &self.root)?;
        self.http.get_ranges(location, ranges).await
    }

    fn list(
        &self,
        prefix: Option<&ObjectPath>,
    ) -> BoxStream<'static, object_store::Result<ObjectMeta>> {
        let prefix = self.prefix(prefix);
        let store = self.clone();
        stream::once(async move {
            let result = match store.ready_recursive(&prefix)? {
                Some(ready) => ready,
                None => store.listing(&prefix, true).await?,
            };
            let range = descendant_range(&result.objects, &prefix);
            Ok::<_, object_store::Error>((result, range))
        })
        .map_ok(|(result, range)| stream::iter(range.map(move |i| Ok(result.objects[i].clone()))))
        .try_flatten()
        .boxed()
    }

    async fn list_with_delimiter(
        &self,
        prefix: Option<&ObjectPath>,
    ) -> object_store::Result<ListResult> {
        let result = self.listing(&self.prefix(prefix), false).await?;
        Ok(ListResult {
            common_prefixes: result.common_prefixes.clone(),
            objects: result.objects.clone(),
        })
    }

    async fn put_opts(
        &self,
        _: &ObjectPath,
        _: PutPayload,
        _: PutOptions,
    ) -> object_store::Result<PutResult> {
        Err(read_only())
    }

    async fn put_multipart_opts(
        &self,
        _: &ObjectPath,
        _: PutMultipartOptions,
    ) -> object_store::Result<Box<dyn MultipartUpload>> {
        Err(read_only())
    }

    fn delete_stream(
        &self,
        locations: BoxStream<'static, object_store::Result<ObjectPath>>,
    ) -> BoxStream<'static, object_store::Result<ObjectPath>> {
        locations
            .map(|location| location.and_then(|_| Err(read_only())))
            .boxed()
    }

    async fn copy_opts(
        &self,
        _: &ObjectPath,
        _: &ObjectPath,
        _: CopyOptions,
    ) -> object_store::Result<()> {
        Err(read_only())
    }
}

// Recursive listings are sorted by ObjectPath. A slash in the search
// key excludes an exact leaf or a sibling sharing only the textual prefix.
fn descendant_range(objects: &[ObjectMeta], prefix: &ObjectPath) -> Range<usize> {
    let prefix = format!("{prefix}/");
    let start = objects.partition_point(|m| m.location.as_ref() < prefix.as_str());
    let count = objects[start..].partition_point(|m| m.location.as_ref().starts_with(&prefix));
    start..start + count
}

fn ensure_in_root(location: &ObjectPath, root: &ObjectPath) -> object_store::Result<()> {
    if descendant_or_self(location, root) {
        Ok(())
    } else {
        Err(hub_error(anyhow::anyhow!("Hub read outside bound catalog")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn public_catalog_requires_explicit_immutable_revision() {
        let root = format!("hf://datasets/owner/repo@{COMMIT}");
        let dataset = HubDataset::parse(&root).unwrap();
        assert!(validate_uri(&root).is_ok());
        assert!(validate_uri(&format!("hf://datasets/O_w.n-er/Repo_1@{COMMIT}")).is_ok());
        assert!(validate_uri(&format!("hf://datasets/_owner/repo_@{COMMIT}")).is_ok());
        assert!(validate_uri("hf://datasets/owner/repo@main").is_err());
        assert!(validate_uri(&format!("{root}/catalog")).is_err());
        assert!(validate_uri(&format!("{root}?token=abc")).is_err());
        assert!(validate_uri(&format!("{root}/")).is_err());
        assert!(validate_uri(&format!("hf://datasets/.owner/repo@{COMMIT}")).is_err());
        assert!(validate_uri(&format!("hf://datasets/owner/repo-@{COMMIT}")).is_err());
        assert!(validate_uri(&format!("hf://datasets/owner/re..po@{COMMIT}")).is_err());
        assert!(validate_uri(&format!("hf://datasets/owner/re--po@{COMMIT}")).is_err());
        assert_eq!(
            dataset.resolve_prefix(),
            format!("datasets/owner/repo/resolve/{COMMIT}")
        );
        assert_eq!(
            dataset
                .tree_url("data/order_book_delta/km:XYZ%5E.HYPERLIQUID", true)
                .unwrap()
                .as_str(),
            format!(
                "{ORIGIN}/api/datasets/owner/repo/tree/{COMMIT}/data/order_book_delta/km:XYZ%255E.HYPERLIQUID?recursive=true"
            )
        );
    }

    #[test]
    fn pagination_cannot_change_dataset_commit_prefix_or_origin() {
        let url = Url::parse(&format!(
            "{ORIGIN}/api/datasets/owner/repo/tree/{COMMIT}/catalog/data?recursive=true"
        ))
        .unwrap();
        assert!(
            link_next("<?recursive=true&cursor=next>; rel=\"next\"", &url)
                .unwrap()
                .is_some()
        );
        assert!(link_next("<https://example.com/>; rel=\"next\"", &url).is_err());
        assert!(
            link_next(
                "</api/datasets/owner/repo/tree/main/catalog/data>; rel=\"next\"",
                &url
            )
            .is_err()
        );
        assert!(link_next("<https://huggingface.co:81/api/datasets/owner/repo/tree/main/catalog>; rel=\"next\"", &url).is_err());
        assert!(link_next("<?recursive=true>; rel=\"next\"", &url).is_err());
    }
    #[test]
    fn recursive_reuse_only_matches_complete_object_path_components() {
        let mut objects = [
            "data/trades/xyz:STRC",
            "data/trades/xyz:STRC-PERP/sibling.parquet",
            "data/trades/xyz:STRC/a.parquet",
            "data/trades/xyz:STRC/nested/b.parquet",
            "data/trades/xyz:STRC0/sibling.parquet",
        ]
        .map(|key| listed_file(ObjectPath::parse(key).unwrap(), 1));
        objects.sort_by(|a, b| a.location.cmp(&b.location));
        let prefix = ObjectPath::parse("data/trades/xyz:STRC").unwrap();
        assert_eq!(
            objects[descendant_range(&objects, &prefix)]
                .iter()
                .map(|item| item.location.as_ref())
                .collect::<Vec<_>>(),
            [
                "data/trades/xyz:STRC/a.parquet",
                "data/trades/xyz:STRC/nested/b.parquet"
            ]
        );
        let absent = ObjectPath::parse("data/trades/xyz:STRCX").unwrap();
        assert!(descendant_range(&objects, &absent).is_empty());
    }

    #[test]
    #[ignore = "explicit anonymous read of an existing public Hugging Face native catalog"]
    fn reads_published_hf_native_book_and_trades() -> Result<()> {
        use nautilus_core::Params;
        use nautilus_model::{
            data::{DataBatch, NautilusDataType},
            enums::RecordFlag,
            instruments::Instrument,
        };
        use nautilus_persistence::catalog::traits::{
            CatalogInstrumentQuery, CatalogQuery, CatalogReader,
        };

        // The real Oct 13 04:00–05:00 UTC catalog includes spot, perp, and HIP-3.
        let uri = std::env::var("QZ_TEST_HF_CATALOG")?;
        let mut catalog = open(&uri)?;
        let ids: Vec<String> = [
            "PURR-USDC-SPOT.HYPERLIQUID",
            "ALT-USD-PERP.HYPERLIQUID",
            "xyz:STRC-USD-PERP.HYPERLIQUID",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let definitions = CatalogReader::instruments(
            &mut catalog,
            &CatalogInstrumentQuery::new().with_instrument_ids(Some(ids.clone())),
        )?;
        for id in &ids {
            assert!(
                definitions.iter().any(|i| i.id().to_string() == *id),
                "{id}"
            );
        }
        const START: u64 = 1_791_864_000_000_000_000;
        const END: u64 = 1_791_867_600_000_000_000;
        let mut totals = [0u64, 0];
        let mut totals_both_clocks = [0u64, 0];
        for id in &ids {
            for (index, kind) in [
                NautilusDataType::OrderBookDelta,
                NautilusDataType::TradeTick,
            ]
            .into_iter()
            .enumerate()
            {
                // Coverage is from exact native file intervals, not a claim about event completeness.
                let intervals = catalog.get_intervals(&kind.clone().into(), Some(id))?;
                assert!(!intervals.is_empty(), "{id} {kind}");
                let mut query = CatalogQuery::new(kind.clone());
                query.identifiers = Some(vec![id.clone()]);
                query.start = Some(START.into());
                query.end = Some((END - 1).into());
                query.params = Some(Params::new(
                    [("optimize_file_loading".to_string(), false.into())].into(),
                ));
                let mut session = catalog.query_batch_session(&query, Some(4096))?;
                let (mut rows, mut both, mut frames, mut last_init) = (0, 0, 0, START);
                let mut saw_last = false;
                let mut observe = |instrument: String, ts_init: u64, ts_event: u64| {
                    assert_eq!(instrument, *id);
                    assert!((START..END).contains(&ts_init) && ts_init >= last_init);
                    last_init = ts_init;
                    rows += 1;
                    both += u64::from((START..END).contains(&ts_event));
                };
                while let Some(batch) = session.next_batch()? {
                    match (index, batch) {
                        (0, DataBatch::BookDelta(values)) => {
                            for row in values.as_ref() {
                                observe(
                                    row.instrument_id.to_string(),
                                    row.ts_init.as_u64(),
                                    row.ts_event.as_u64(),
                                );
                                saw_last = row.flags & RecordFlag::F_LAST as u8 != 0;
                                frames += u64::from(saw_last);
                            }
                        }
                        (1, DataBatch::Trade(values)) => {
                            for row in values.as_ref() {
                                observe(
                                    row.instrument_id.to_string(),
                                    row.ts_init.as_u64(),
                                    row.ts_event.as_u64(),
                                );
                            }
                        }
                        (_, other) => anyhow::bail!("Unexpected {kind} batch: {other:?}"),
                    }
                }
                assert!(rows > 0, "{id} {kind}");
                if index == 0 {
                    assert!(frames > 0 && saw_last, "{id} {kind}");
                }
                totals[index] += rows;
                totals_both_clocks[index] += both;
                eprintln!("{id} {kind}: rows={rows} rows_both_clocks={both} book_frames={frames}");
            }
        }
        eprintln!(
            "Oct13 UTC paired native counts [book, trade]: availability={totals:?} both_clocks={totals_both_clocks:?}"
        );
        Ok(())
    }
}
