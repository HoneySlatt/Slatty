//! GOG's binary patches between two builds: xdelta3 (VCDIFF) deltas of changed files, usually far
//! smaller than the changed chunks. Each patched file is checked against GOG's MD5; any failure
//! leaves the file to the normal chunk download.

use std::collections::HashMap;
use std::io::{BufWriter, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use flate2::read::ZlibDecoder;
use md5::{Digest, Md5};
use oxidelta::compress::decoder::DeltaDecoder;
use oxidelta::vcdiff::decoder::{DecodeError, SourceProvider};
use reqwest::Client;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::galaxy::{self, Chunk, ContentSource};
use crate::http;
use crate::installer::{self, FileSet, InstallRecord};

const CONTENT_SYSTEM: &str = "https://content-system.gog.com";
const CDN: &str = "https://gog-cdn-fastly.gog.com";
const PARTIAL_SUFFIX: &str = ".slatty-patch";

/// One file to rebuild from its previous version and a delta.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FilePatch {
    #[serde(rename = "path_source")]
    pub source: String,
    #[serde(rename = "path_target")]
    pub target: String,
    pub md5_source: String,
    pub md5_target: String,
    /// Delta chunks, from the product's patch store.
    pub chunks: Vec<Chunk>,
    #[serde(skip)]
    pub product_id: String,
}

impl FilePatch {
    pub fn delta_size(&self) -> u64 {
        self.chunks.iter().map(|c| c.compressed_size).sum()
    }
}

#[derive(Deserialize)]
struct PatchLink {
    link: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchData {
    algorithm: String,
    base_product_id: String,
    depots: Vec<PatchDepot>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchDepot {
    product_id: String,
    languages: Vec<String>,
    manifest: String,
}

#[derive(Deserialize)]
struct DiffManifest {
    depot: DiffItems,
}

#[derive(Deserialize)]
struct DiffItems {
    items: Vec<serde_json::Value>,
}

async fn zlib_json<T: serde::de::DeserializeOwned>(
    http: &Client,
    tokens: &Tokens,
    url: &str,
    context: &'static str,
) -> Result<T> {
    let raw = http::bytes(
        http.get(url).bearer_auth(tokens.access_token.expose()),
        context,
    )
    .await?;
    galaxy::decode_zlib_json(&raw, context)
}

/// File patches GOG offers between two builds of a game, for the given language and products
/// (the game and its installed DLC). `None` when GOG has no usable patch.
pub async fn find(
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
    from_build: &str,
    to_build: &str,
    language: &str,
    products: &[String],
) -> Result<Option<Vec<FilePatch>>> {
    let url = format!(
        "{CONTENT_SYSTEM}/products/{game_id}/patches?_version=4&from_build_id={from_build}&to_build_id={to_build}"
    );
    let link: PatchLink = match zlib_json(http, tokens, &url, "looking for a patch").await {
        Ok(l) => l,
        Err(Error::Http { status: 404, .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let Some(link) = link.link else {
        return Ok(None);
    };
    let data: PatchData = zlib_json(http, tokens, &link, "reading a patch").await?;
    select(http, tokens, data, language, products).await
}

async fn select(
    http: &Client,
    tokens: &Tokens,
    data: PatchData,
    language: &str,
    products: &[String],
) -> Result<Option<Vec<FilePatch>>> {
    let Some(depots) = usable_depots(data, language, products) else {
        return Ok(None);
    };
    let mut files = Vec::new();
    for depot in depots {
        let url = format!(
            "{CDN}/content-system/v2/patches/meta/{}",
            galaxy::galaxy_path(&depot.manifest)
        );
        let diffs: DiffManifest = zlib_json(http, tokens, &url, "reading a patch manifest").await?;
        files.extend(parse_diffs(diffs.depot.items, &depot.product_id)?);
    }
    Ok(Some(files))
}

/// Depots of the patch for the installed products and language; `None` when the patch is not
/// an xdelta3 one or covers nothing installed.
fn usable_depots(data: PatchData, language: &str, products: &[String]) -> Option<Vec<PatchDepot>> {
    if data.algorithm != "xdelta3" {
        return None;
    }
    let depots: Vec<PatchDepot> = data
        .depots
        .into_iter()
        .filter(|d| d.product_id == data.base_product_id || products.contains(&d.product_id))
        .filter(|d| {
            d.languages
                .iter()
                .any(|l| l == "*" || l.eq_ignore_ascii_case(language))
        })
        .collect();
    (!depots.is_empty()).then_some(depots)
}

fn parse_diffs(items: Vec<serde_json::Value>, product_id: &str) -> Result<Vec<FilePatch>> {
    items
        .into_iter()
        .map(|item| {
            if item["type"] != "DepotDiff" {
                return Err(Error::Unsupported(format!(
                    "patch item of type {}",
                    item["type"]
                )));
            }
            let mut patch: FilePatch = serde_json::from_value(item)
                .map_err(|e| Error::parse("reading a patch manifest", e))?;
            patch.product_id = product_id.to_string();
            Ok(patch)
        })
        .collect()
}

/// Whole-file MD5, as GOG states it for patch sources and targets.
pub fn file_md5(path: &Path) -> Result<Option<String>> {
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::io(format!("open {}", path.display()), e)),
    };
    let mut hasher = Md5::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(Some(fsutil::hex(&hasher.finalize())))
}

/// Old file read on demand, so a delta of any size is applied in constant memory.
struct FileSource {
    file: std::fs::File,
    len: u64,
}

impl SourceProvider for FileSource {
    fn read_source(
        &mut self,
        offset: u64,
        buf: &mut [u8],
    ) -> std::result::Result<usize, DecodeError> {
        let wanted = buf.len().min(self.len.saturating_sub(offset) as usize);
        self.file
            .read_exact_at(&mut buf[..wanted], offset)
            .map_err(DecodeError::Io)?;
        Ok(wanted)
    }

    fn source_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

/// Writes `target` from `source` and the delta, if the result has GOG's MD5. The target is
/// replaced atomically; nothing changes on any failure.
pub fn apply(source: &Path, delta: &Path, target: &Path, md5_target: &str) -> Result<()> {
    let delta = std::fs::File::open(delta)
        .map_err(|e| Error::io(format!("open {}", delta.display()), e))?;
    let file = std::fs::File::open(source)
        .map_err(|e| Error::io(format!("open {}", source.display()), e))?;
    let len = file
        .metadata()
        .map_err(|e| Error::io(format!("stat {}", source.display()), e))?
        .len();
    let partial = partial_path(target);
    let result = (|| {
        let out = std::fs::File::create(&partial)
            .map_err(|e| Error::io(format!("create {}", partial.display()), e))?;
        let mut writer = HashingWriter {
            inner: BufWriter::new(out),
            hasher: Md5::new(),
        };
        DeltaDecoder::new(std::io::BufReader::new(delta))
            .decode_to(&mut FileSource { file, len }, &mut writer)
            .map_err(|e| Error::Refused(format!("patch of {} failed: {e}", target.display())))?;
        let out = writer
            .inner
            .into_inner()
            .map_err(|e| Error::io(format!("write {}", partial.display()), e.into_error()))?;
        if fsutil::hex(&writer.hasher.finalize()) != md5_target {
            return Err(Error::Refused(format!(
                "patched {} does not match GOG's checksum",
                target.display()
            )));
        }
        out.sync_all()
            .map_err(|e| Error::io(format!("sync {}", partial.display()), e))?;
        if let Ok(meta) = std::fs::metadata(source) {
            let _ = std::fs::set_permissions(&partial, meta.permissions());
        }
        std::fs::rename(&partial, target)
            .map_err(|e| Error::io(format!("rename {}", partial.display()), e))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

fn partial_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(PARTIAL_SUFFIX);
    target.with_file_name(name)
}

struct HashingWriter<W: Write> {
    inner: W,
    hasher: Md5,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Downloads the delta of a file patch from the patch store into `dest`, checking every chunk.
pub async fn download_delta<S: ContentSource>(
    source: &S,
    patch: &FilePatch,
    dest: &Path,
) -> Result<()> {
    let store = galaxy::patch_store(&patch.product_id);
    let mut out = std::fs::File::create(dest)
        .map_err(|e| Error::io(format!("create {}", dest.display()), e))?;
    for c in &patch.chunks {
        let raw = source.chunk(&store, &c.compressed_md5).await?;
        let c = c.clone();
        let data = tokio::task::spawn_blocking(move || unpack(&raw, &c))
            .await
            .expect("chunk task panicked")?;
        out.write_all(&data)
            .map_err(|e| Error::io(format!("write {}", dest.display()), e))?;
    }
    Ok(())
}

fn unpack(raw: &[u8], c: &Chunk) -> Result<Vec<u8>> {
    let mut data = Vec::with_capacity(c.size as usize);
    if fsutil::hex(&Md5::digest(raw)) != c.compressed_md5
        || ZlibDecoder::new(raw).read_to_end(&mut data).is_err()
        || fsutil::hex(&Md5::digest(&data)) != c.md5
    {
        return Err(Error::Refused(format!(
            "patch chunk {} is corrupted",
            c.compressed_md5
        )));
    }
    Ok(data)
}

#[cfg(test)]
mod tests;

/// Files rebuilt from deltas during an update.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Patched {
    pub files: Vec<PathBuf>,
    pub delta_bytes: u64,
}

/// Applies every patch whose source is on disk exactly as GOG expects and whose target belongs to
/// the new build. Other files, and any patch that fails, are left for the normal download.
pub async fn apply_all<S: ContentSource>(
    source: &S,
    patches: &[FilePatch],
    set: &FileSet,
    old: &InstallRecord,
    dir: &Path,
    cancel: &CancellationToken,
) -> Result<Patched> {
    let lower = |p: &Path| p.to_string_lossy().to_lowercase();
    let targets: HashMap<String, &PathBuf> = set.files.iter().map(|(p, _)| (lower(p), p)).collect();
    let mut sources = HashMap::new();
    for f in &old.files {
        let rel = installer::safe_relative(&f.path)?;
        sources.insert(lower(&rel), rel);
    }
    let mut done = Patched::default();
    for patch in patches {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let (Ok(src), Ok(dst)) = (
            installer::safe_relative(&patch.source),
            installer::safe_relative(&patch.target),
        ) else {
            continue;
        };
        let (Some(src), Some(dst)) = (sources.get(&lower(&src)), targets.get(&lower(&dst))) else {
            continue;
        };
        let (src, dst) = (dir.join(src), dir.join(dst));
        let md5 = tokio::task::spawn_blocking({
            let src = src.clone();
            move || file_md5(&src)
        })
        .await
        .expect("hash task panicked")?;
        if md5.as_deref() != Some(patch.md5_source.as_str()) {
            continue;
        }
        let delta = partial_path(&dst).with_extension("slatty-delta");
        let result = async {
            download_delta(source, patch, &delta).await?;
            let (src, dst, delta, md5) =
                (src, dst.clone(), delta.clone(), patch.md5_target.clone());
            tokio::task::spawn_blocking(move || apply(&src, &delta, &dst, &md5))
                .await
                .expect("patch task panicked")
        }
        .await;
        let _ = std::fs::remove_file(&delta);
        match result {
            Ok(()) => {
                done.files
                    .push(dst.strip_prefix(dir).unwrap_or(&dst).to_path_buf());
                done.delta_bytes += patch.delta_size();
            }
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(e) => tracing::warn!("patch not applied, the file will be downloaded: {e}"),
        }
    }
    done.files.sort();
    Ok(done)
}
