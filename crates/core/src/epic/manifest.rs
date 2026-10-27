//! Epic's build manifests: which files a build has, each made of parts of chunks, as legendary reads
//! them (`legendary/models/manifest.py`). Binary manifests only; Epic still serves an older JSON
//! form for a few old builds, which is refused.

use std::collections::HashMap;
use std::io::Read;

use flate2::read::ZlibDecoder;

use crate::error::{Error, Result};

const MAGIC: u32 = 0x44BE_C00C;
/// The largest manifest body read, once inflated: far above the largest known (about 100 MiB for a
/// game of 500,000 files).
const MAX_BODY: u64 = 512 << 20;
/// The largest chunk accepted. Epic cuts files into chunks of 1 MiB; a manifest saying otherwise
/// is not trusted with an allocation of the size it gives.
pub const MAX_CHUNK: u32 = 64 << 20;

const COMPRESSED: u8 = 0x1;
const ENCRYPTED: u8 = 0x2;
/// A file flag: executable (meaningful for Mac builds).
const EXECUTABLE: u8 = 0x4;

pub type Guid = [u32; 4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The version the manifest was written for: it decides the chunks' folder and address.
    pub feature_level: u32,
    pub app_name: String,
    pub build_version: String,
    /// The program a launch starts, relative to the game folder, and its arguments.
    pub launch_exe: String,
    pub launch_command: String,
    /// What the game asks to be installed first (a redistributable), when anything.
    pub prerequisites: Option<Prerequisites>,
    pub chunks: HashMap<Guid, ChunkInfo>,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prerequisites {
    pub name: String,
    pub path: String,
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkInfo {
    pub guid: Guid,
    pub hash: u64,
    pub group: u8,
    /// Its size once unpacked.
    pub window_size: u32,
    /// Its size as downloaded.
    pub file_size: u64,
    /// The key it is encrypted with, by id; none for a plain chunk.
    pub secret: Option<Guid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Its path in the game folder, as the manifest writes it (`/` between folders).
    pub name: String,
    /// Not empty for a symbolic link, which is never made.
    pub symlink_target: String,
    pub sha1: [u8; 20],
    pub flags: u8,
    pub parts: Vec<ChunkPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPart {
    pub guid: Guid,
    /// Where the part starts in its chunk, unpacked, and its length.
    pub offset: u32,
    pub size: u32,
}

impl FileEntry {
    pub fn size(&self) -> u64 {
        self.parts.iter().map(|p| p.size as u64).sum()
    }

    pub fn is_executable(&self) -> bool {
        self.flags & EXECUTABLE != 0
    }
}

impl ChunkInfo {
    /// Its address under the build's base address (`ChunkInfo.path`).
    pub fn path(&self, feature_level: u32) -> String {
        let dir = match feature_level {
            22.. => "ChunksV5",
            15.. => "ChunksV4",
            6.. => "ChunksV3",
            3.. => "ChunksV2",
            _ => "Chunks",
        };
        if feature_level >= 22 {
            let secret = match self.secret {
                Some(s) if s != [0; 4] => base64_url(&guid_bytes(&s)),
                _ => "plain".into(),
            };
            return format!(
                "{dir}/{secret}/{:02}/{}_{}.chunk",
                self.group,
                base64_url(&self.hash.to_le_bytes()),
                base64_url(&guid_bytes(&self.guid))
            );
        }
        format!(
            "{dir}/{:02}/{:016X}_{}.chunk",
            self.group,
            self.hash,
            guid_hex(&self.guid)
        )
    }
}

/// A guid as Epic names it among the keys of a build: its four numbers in upper-case hexadecimal.
pub fn guid_hex(guid: &Guid) -> String {
    guid.iter().map(|g| format!("{g:08X}")).collect()
}

fn guid_bytes(guid: &Guid) -> Vec<u8> {
    guid.iter().flat_map(|g| g.to_le_bytes()).collect()
}

/// Base64 for addresses (`-` and `_`), without padding.
fn base64_url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for group in bytes.chunks(3) {
        let n = group
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=group.len() {
            out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// Reads bounded values from a manifest, refusing to read past its end.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, at: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.data.len());
        let end = end.ok_or_else(truncated)?;
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().expect("N bytes"))
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn guid(&mut self) -> Result<Guid> {
        Ok([self.u32()?, self.u32()?, self.u32()?, self.u32()?])
    }

    /// A string as Unreal writes one: its length with its terminating zero, in bytes for ASCII,
    /// negative and in UTF-16 units for anything else.
    fn string(&mut self) -> Result<String> {
        let len = self.i32()?;
        if len == 0 {
            return Ok(String::new());
        }
        if len > 0 {
            let raw = self.bytes(len as usize)?;
            let text = &raw[..raw.len() - 1];
            return String::from_utf8(text.to_vec()).map_err(|_| bad("a string is not text"));
        }
        let units = (len.unsigned_abs() as usize)
            .checked_mul(2)
            .ok_or_else(truncated)?;
        let raw = self.bytes(units)?;
        let utf16: Vec<u16> = raw[..raw.len() - 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16(&utf16).map_err(|_| bad("a string is not text"))
    }

    /// A count of elements at least `each` bytes long: refused when they cannot all fit in what
    /// is left, so that no allocation follows a count the data cannot hold.
    fn count(&mut self, each: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n.saturating_mul(each) > self.data.len() - self.at {
            return Err(truncated());
        }
        Ok(n)
    }

    /// Goes to `start + size`, the end of a section, over what this reader does not know.
    fn end_section(&mut self, start: usize, size: u32) -> Result<()> {
        let end = start.checked_add(size as usize).ok_or_else(truncated)?;
        if end < self.at || end > self.data.len() {
            return Err(bad("a section is not the size it says"));
        }
        self.at = end;
        Ok(())
    }
}

fn truncated() -> Error {
    bad("it ends too early")
}

fn bad(detail: &str) -> Error {
    Error::parse("an Epic manifest", detail)
}

/// Reads a manifest, decrypting it with the build's keys (`secrets`, hexadecimal, by key id) when
/// it is encrypted.
pub fn parse(raw: &[u8], secrets: &HashMap<String, String>) -> Result<Manifest> {
    if raw.first() == Some(&b'{') {
        return Err(Error::Unsupported(
            "this build has an older kind of Epic manifest (JSON), which Slatty does not read yet"
                .into(),
        ));
    }
    let mut r = Reader::new(raw);
    if r.u32()? != MAGIC {
        return Err(bad("not a manifest"));
    }
    let header_size = r.u32()?;
    let size_uncompressed = r.u32()?;
    let _size_compressed = r.u32()?;
    let sha1: [u8; 20] = r.array()?;
    let stored_as = r.u8()?;
    let version = r.u32()?;
    let (secret, tag) = if version >= 22 {
        (Some(r.guid()?), Some(r.array::<16>()?))
    } else {
        (None, None)
    };
    r.end_section(0, header_size)?;
    let rest = &raw[r.at..];
    let body = if stored_as & COMPRESSED != 0 {
        let body = inflate(rest, size_uncompressed as u64)?;
        if super::sha1(&body) != sha1 {
            return Err(bad("its checksum does not match"));
        }
        body
    } else {
        rest.to_vec()
    };
    let mut r = Reader::new(&body);

    // Metadata.
    let start = r.at;
    let meta_size = r.u32()?;
    let data_version = r.u8()?;
    let feature_level = r.u32()?;
    let _is_file_data = r.u8()?;
    let _app_id = r.u32()?;
    let app_name = r.string()?;
    let build_version = r.string()?;
    let mut launch_exe = r.string()?;
    let mut launch_command = r.string()?;
    let prereq_ids = r.count(4)?;
    for _ in 0..prereq_ids {
        r.string()?;
    }
    let mut prereq = [r.string()?, r.string()?, r.string()?];
    if data_version >= 1 {
        let _build_id = r.string()?;
    }
    if data_version >= 2 {
        let _uninstall = (r.string()?, r.string()?);
    }
    r.end_section(start, meta_size)?;

    // Chunks.
    let start = r.at;
    let size = r.u32()?;
    let _version = r.u8()?;
    let count = r.count(57)?;
    let mut chunks: Vec<ChunkInfo> = (0..count)
        .map(|_| {
            Ok(ChunkInfo {
                guid: r.guid()?,
                hash: 0,
                group: 0,
                window_size: 0,
                file_size: 0,
                secret: None,
            })
        })
        .collect::<Result<_>>()?;
    for c in &mut chunks {
        c.hash = r.u64()?;
    }
    for _ in &chunks {
        r.bytes(20)?;
    }
    for c in &mut chunks {
        c.group = r.u8()?;
    }
    for c in &mut chunks {
        c.window_size = r.u32()?;
    }
    for c in &mut chunks {
        c.file_size = u64::try_from(i64::from_le_bytes(r.array()?))
            .map_err(|_| bad("a chunk has a negative size"))?;
    }
    if feature_level >= 22 {
        for c in &mut chunks {
            c.secret = Some(r.guid()?).filter(|s| *s != [0; 4]);
        }
        for _ in &chunks {
            r.u32()?;
        }
        for _ in &chunks {
            r.bytes(16)?;
        }
    }
    r.end_section(start, size)?;

    // Files.
    let start = r.at;
    let size = r.u32()?;
    let version = r.u8()?;
    let count = r.count(37)?;
    let mut files: Vec<FileEntry> = (0..count)
        .map(|_| {
            Ok(FileEntry {
                name: r.string()?,
                symlink_target: String::new(),
                sha1: [0; 20],
                flags: 0,
                parts: Vec::new(),
            })
        })
        .collect::<Result<_>>()?;
    for f in &mut files {
        f.symlink_target = r.string()?;
    }
    for f in &mut files {
        f.sha1 = r.array()?;
    }
    for f in &mut files {
        f.flags = r.u8()?;
    }
    for _ in &files {
        let tags = r.count(4)?;
        for _ in 0..tags {
            r.string()?;
        }
    }
    for f in &mut files {
        let parts = r.count(28)?;
        f.parts.reserve(parts);
        for _ in 0..parts {
            let part_start = r.at;
            let part_size = r.u32()?;
            f.parts.push(ChunkPart {
                guid: r.guid()?,
                offset: r.u32()?,
                size: r.u32()?,
            });
            r.end_section(part_start, part_size)?;
        }
    }
    if version >= 1 {
        for _ in &files {
            if r.u32()? != 0 {
                r.bytes(16)?;
            }
        }
        for _ in &files {
            r.string()?;
        }
    }
    if version >= 2 {
        for _ in &files {
            r.bytes(32)?;
        }
    }
    r.end_section(start, size)?;

    // Custom fields: nothing Slatty uses.
    let start = r.at;
    let size = r.u32()?;
    r.end_section(start, size)?;

    if stored_as & ENCRYPTED != 0 {
        if feature_level < 24 {
            return Err(Error::Unsupported(
                "this build's Epic manifest is encrypted in a way Slatty does not read".into(),
            ));
        }
        let secret = secret.ok_or_else(|| bad("an encrypted manifest names no key"))?;
        let key = secrets.get(&guid_hex(&secret)).ok_or_else(|| {
            Error::Unsupported(
                "Epic did not give the key to this build's manifest (a preload?)".into(),
            )
        })?;
        let plain = decrypt_names(&mut r, key, &tag.expect("read with the key's id"))?;
        let mut p = Reader::new(&plain);
        launch_exe = p.string()?;
        launch_command = p.string()?;
        let ids = p.count(4)?;
        for _ in 0..ids {
            p.string()?;
        }
        prereq = [p.string()?, p.string()?, p.string()?];
        let _uninstall = (p.string()?, p.string()?);
        for f in &mut files {
            f.name = p.string()?;
            f.symlink_target = p.string()?;
        }
    }

    let chunks: HashMap<Guid, ChunkInfo> = chunks.into_iter().map(|c| (c.guid, c)).collect();
    check(&chunks, &files)?;
    let [name, path, args] = prereq;
    Ok(Manifest {
        feature_level,
        app_name,
        build_version,
        launch_exe,
        launch_command,
        prerequisites: (!path.is_empty()).then_some(Prerequisites { name, path, args }),
        chunks,
        files,
    })
}

/// The encrypted part of a manifest (feature level 24): its launch command, prerequisites and
/// file names, as legendary reads them (`Manifest.decrypt`).
fn decrypt_names(r: &mut Reader, key: &str, tag: &[u8; 16]) -> Result<Vec<u8>> {
    let start = r.at;
    let size = r.u32()?;
    let _version = r.u8()?;
    let cipher_size = r.u32()? as usize;
    let mut inner = Reader::new(r.bytes(cipher_size)?);
    let header_start = inner.at;
    let header_size = inner.u32()?;
    let _version = inner.u32()?;
    let stored_as = inner.u8()?;
    let uncompressed = inner.u32()?;
    let _compressed = inner.u32()?;
    let iv_len = inner.u32()? as usize;
    let iv = inner.bytes(iv_len)?.to_vec();
    inner.end_section(header_start, header_size)?;
    let ciphertext_len = inner.u32()? as usize;
    let ciphertext = inner.bytes(ciphertext_len)?;
    r.end_section(start, size)?;
    let plain = super::decrypt(key, &iv, ciphertext, tag)?;
    if stored_as & COMPRESSED != 0 {
        return inflate(&plain, uncompressed as u64);
    }
    Ok(plain)
}

/// Inflates zlib data of the size given, refusing more.
pub(crate) fn inflate(data: &[u8], size: u64) -> Result<Vec<u8>> {
    if size > MAX_BODY {
        return Err(bad("it is larger than any Epic makes"));
    }
    let mut out = Vec::with_capacity(size as usize);
    ZlibDecoder::new(data)
        .take(size + 1)
        .read_to_end(&mut out)
        .map_err(|_| bad("its compressed data is damaged"))?;
    if out.len() as u64 != size {
        return Err(bad("its compressed data is not the size it says"));
    }
    Ok(out)
}

/// Refuses chunks no real one can be, and parts outside their chunk or of a chunk not listed.
fn check(chunks: &HashMap<Guid, ChunkInfo>, files: &[FileEntry]) -> Result<()> {
    if chunks
        .values()
        .any(|c| c.window_size > MAX_CHUNK || c.file_size > MAX_CHUNK as u64)
    {
        return Err(Error::Refused(
            "the manifest lists a chunk larger than any Epic makes".into(),
        ));
    }
    for p in files.iter().flat_map(|f| &f.parts) {
        let chunk = chunks
            .get(&p.guid)
            .ok_or_else(|| bad("a file is made of a chunk it does not list"))?;
        if p.offset as u64 + p.size as u64 > chunk.window_size as u64 {
            return Err(bad("a file takes a part beyond the end of its chunk"));
        }
    }
    Ok(())
}

/// Writes manifests as Epic does, for tests (`Manifest.write`).
#[cfg(test)]
pub(crate) mod write {
    use super::*;

    fn string(out: &mut Vec<u8>, s: &str) {
        if s.is_empty() {
            out.extend(0i32.to_le_bytes());
        } else if s.is_ascii() {
            out.extend((s.len() as i32 + 1).to_le_bytes());
            out.extend(s.as_bytes());
            out.push(0);
        } else {
            let units: Vec<u16> = s.encode_utf16().collect();
            out.extend((-(units.len() as i32 + 1)).to_le_bytes());
            out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
            out.extend([0, 0]);
        }
    }

    fn section(out: &mut Vec<u8>, body: Vec<u8>) {
        out.extend((body.len() as u32 + 4).to_le_bytes());
        out.extend(body);
    }

    fn guid(out: &mut Vec<u8>, g: &Guid) {
        out.extend(guid_bytes(g));
    }

    /// A compressed binary manifest of `feature_level` (below 22: no encryption fields).
    pub(crate) fn manifest(m: &Manifest, chunk_shas: &HashMap<Guid, [u8; 20]>) -> Vec<u8> {
        let mut body = Vec::new();
        let mut meta = vec![0u8];
        meta.extend(m.feature_level.to_le_bytes());
        meta.push(0);
        meta.extend(0u32.to_le_bytes());
        string(&mut meta, &m.app_name);
        string(&mut meta, &m.build_version);
        string(&mut meta, &m.launch_exe);
        string(&mut meta, &m.launch_command);
        meta.extend(0u32.to_le_bytes());
        let p = m.prerequisites.clone().unwrap_or(Prerequisites {
            name: String::new(),
            path: String::new(),
            args: String::new(),
        });
        for s in [&p.name, &p.path, &p.args] {
            string(&mut meta, s);
        }
        section(&mut body, meta);

        let mut chunks: Vec<&ChunkInfo> = m.chunks.values().collect();
        chunks.sort_by_key(|c| c.guid);
        let mut cdl = vec![0u8];
        cdl.extend((chunks.len() as u32).to_le_bytes());
        chunks.iter().for_each(|c| guid(&mut cdl, &c.guid));
        chunks.iter().for_each(|c| cdl.extend(c.hash.to_le_bytes()));
        chunks
            .iter()
            .for_each(|c| cdl.extend(chunk_shas.get(&c.guid).copied().unwrap_or([0; 20])));
        chunks.iter().for_each(|c| cdl.push(c.group));
        chunks
            .iter()
            .for_each(|c| cdl.extend(c.window_size.to_le_bytes()));
        chunks
            .iter()
            .for_each(|c| cdl.extend((c.file_size as i64).to_le_bytes()));
        if m.feature_level >= 22 {
            chunks
                .iter()
                .for_each(|c| guid(&mut cdl, &c.secret.unwrap_or([0; 4])));
            chunks.iter().for_each(|_| cdl.extend(0u32.to_le_bytes()));
            chunks.iter().for_each(|_| cdl.extend([0u8; 16]));
        }
        section(&mut body, cdl);

        let mut fml = vec![0u8];
        fml.extend((m.files.len() as u32).to_le_bytes());
        m.files.iter().for_each(|f| string(&mut fml, &f.name));
        m.files
            .iter()
            .for_each(|f| string(&mut fml, &f.symlink_target));
        m.files.iter().for_each(|f| fml.extend(f.sha1));
        m.files.iter().for_each(|f| fml.push(f.flags));
        m.files.iter().for_each(|_| fml.extend(0u32.to_le_bytes()));
        for f in &m.files {
            fml.extend((f.parts.len() as u32).to_le_bytes());
            for p in &f.parts {
                fml.extend(28u32.to_le_bytes());
                guid(&mut fml, &p.guid);
                fml.extend(p.offset.to_le_bytes());
                fml.extend(p.size.to_le_bytes());
            }
        }
        section(&mut body, fml);
        section(&mut body, {
            let mut cf = vec![0u8];
            cf.extend(0u32.to_le_bytes());
            cf
        });

        let mut compressed = Vec::new();
        {
            use std::io::Write;
            let mut z =
                flate2::write::ZlibEncoder::new(&mut compressed, flate2::Compression::fast());
            z.write_all(&body).unwrap();
            z.finish().unwrap();
        }
        let mut out = Vec::new();
        out.extend(MAGIC.to_le_bytes());
        let header_size: u32 = if m.feature_level >= 22 { 73 } else { 41 };
        out.extend(header_size.to_le_bytes());
        out.extend((body.len() as u32).to_le_bytes());
        out.extend((compressed.len() as u32).to_le_bytes());
        out.extend(crate::epic::sha1(&body));
        out.push(COMPRESSED);
        out.extend(m.feature_level.to_le_bytes());
        if m.feature_level >= 22 {
            out.extend([0u8; 32]);
        }
        out.extend(compressed);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(feature_level: u32) -> Manifest {
        let a = [1, 2, 3, 4];
        let b = [5, 6, 7, 0xAB];
        Manifest {
            feature_level,
            app_name: "Fake".into(),
            build_version: "1.0.0-[FAKE]".into(),
            launch_exe: "Bin/Game.exe".into(),
            launch_command: "-fake".into(),
            prerequisites: Some(Prerequisites {
                name: "[FAKE] runtime".into(),
                path: "Redist/setup.exe".into(),
                args: "/quiet".into(),
            }),
            chunks: [a, b]
                .into_iter()
                .map(|g| {
                    (
                        g,
                        ChunkInfo {
                            guid: g,
                            hash: 0x0123_4567_89AB_CDEF,
                            group: 7,
                            window_size: 1 << 20,
                            file_size: 1000,
                            secret: None,
                        },
                    )
                })
                .collect(),
            files: vec![
                FileEntry {
                    name: "Bin/Game.exe".into(),
                    symlink_target: String::new(),
                    sha1: [9; 20],
                    flags: 0,
                    parts: vec![
                        ChunkPart {
                            guid: a,
                            offset: 0,
                            size: 100,
                        },
                        ChunkPart {
                            guid: b,
                            offset: 50,
                            size: 20,
                        },
                    ],
                },
                FileEntry {
                    name: "Données/é.txt".into(),
                    symlink_target: String::new(),
                    sha1: [3; 20],
                    flags: EXECUTABLE,
                    parts: vec![ChunkPart {
                        guid: a,
                        offset: 100,
                        size: 5,
                    }],
                },
            ],
        }
    }

    #[test]
    fn a_manifest_reads_back_as_written() {
        for level in [18, 21, 22] {
            let m = sample(level);
            let read = parse(&write::manifest(&m, &HashMap::new()), &HashMap::new()).unwrap();
            assert_eq!(read, m, "feature level {level}");
            assert_eq!(read.files[0].size(), 120);
            assert!(read.files[1].is_executable());
        }
    }

    /// Chunks are found where legendary looks for them (`ChunkInfo.path`).
    #[test]
    fn chunks_are_named_as_epic_names_them() {
        let c = ChunkInfo {
            guid: [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_FF00],
            hash: 0x0123_4567_89AB_CDEF,
            group: 7,
            window_size: 1 << 20,
            file_size: 1000,
            secret: None,
        };
        assert_eq!(
            c.path(18),
            "ChunksV4/07/0123456789ABCDEF_112233445566778899AABBCCDDEEFF00.chunk"
        );
        assert_eq!(
            c.path(22),
            "ChunksV5/plain/07/782riWdFIwE_RDMiEYh3ZlXMu6qZAP_u3Q.chunk"
        );
        assert!(c.path(12).starts_with("ChunksV3/"));
    }

    #[test]
    fn urlsafe_base64_has_no_padding() {
        assert_eq!(base64_url(b"f"), "Zg");
        assert_eq!(base64_url(b"fo"), "Zm8");
        assert_eq!(base64_url(b"foo"), "Zm9v");
        assert_eq!(base64_url(&[0xfb, 0xff]), "-_8");
    }

    /// A damaged or hostile manifest is an error, never a panic or an allocation of the size it
    /// claims.
    #[test]
    fn damaged_manifests_are_errors() {
        let good = write::manifest(&sample(18), &HashMap::new());
        for cut in [0, 3, 10, 40, 41, 60, good.len() - 1] {
            assert!(
                parse(&good[..cut], &HashMap::new()).is_err(),
                "cut at {cut}"
            );
        }
        let mut flipped = good.clone();
        let last = flipped.len() - 3;
        flipped[last] ^= 0xFF;
        assert!(parse(&flipped, &HashMap::new()).is_err());
        assert!(matches!(
            parse(b"{\"ManifestFileVersion\": \"013\"}", &HashMap::new()),
            Err(Error::Unsupported(_))
        ));

        let mut outside = sample(18);
        outside.files[0].parts[0].offset = (1 << 20) - 10;
        let raw = write::manifest(&outside, &HashMap::new());
        assert!(
            parse(&raw, &HashMap::new()).is_err(),
            "a part past its chunk"
        );

        let mut huge = sample(18);
        huge.chunks.values_mut().next().unwrap().window_size = MAX_CHUNK + 1;
        let raw = write::manifest(&huge, &HashMap::new());
        assert!(matches!(
            parse(&raw, &HashMap::new()),
            Err(Error::Refused(_))
        ));
    }
}
