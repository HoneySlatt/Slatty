//! Reading the zip inside GOG's Linux installers without downloading it: its directory from the
//! end of the file, then each file on its own. The installer is a shell script followed by the
//! zip, so offsets written in the zip are shifted by the length of the script.

use crate::error::{Error, Result};

const EOCD: u32 = 0x0605_4b50;
const ZIP64_LOCATOR: u32 = 0x0706_4b50;
const ZIP64_EOCD: u32 = 0x0606_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

/// How much of the end of the file to read to find the directory's position: the end record, a
/// comment of at most 64 KiB and the ZIP64 records.
pub const TAIL: u64 = 22 + 0xffff + 20 + 56;

pub const STORED: u16 = 0;
pub const DEFLATED: u16 = 8;

/// One entry of the zip's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    pub name: String,
    pub method: u16,
    pub crc: u32,
    pub compressed: u64,
    pub size: u64,
    /// Unix mode, when the zip was made on Unix.
    pub mode: Option<u32>,
    /// Where its local header starts in the installer.
    pub header: u64,
}

impl ZipEntry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }

    pub fn is_symlink(&self) -> bool {
        self.mode.is_some_and(|m| m & 0o170000 == 0o120000)
    }

    /// Bytes to read from `header` to be sure to cover the local header and the data. The local
    /// header repeats the name; its extra field is not always the directory's, hence the margin.
    pub fn span(&self) -> u64 {
        30 + self.name.len() as u64 + 1024 + self.compressed
    }
}

/// Where the directory is, read from the last `tail.len()` bytes of a file of `size` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Directory {
    pub start: u64,
    pub len: u64,
    pub entries: u64,
    /// Added to every offset written in the zip.
    pub shift: u64,
}

pub fn directory(tail: &[u8], size: u64) -> Result<Directory> {
    let tail_start = size - tail.len() as u64;
    let at = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| u32_at(tail, i) == EOCD)
        .ok_or_else(|| bad("no end of central directory"))?;
    let (mut entries, mut len, mut recorded) = (
        u16_at(tail, at + 10) as u64,
        u32_at(tail, at + 12) as u64,
        u32_at(tail, at + 16) as u64,
    );
    let mut end = tail_start + at as u64;
    let zip64 = at >= 20 && u32_at(tail, at - 20) == ZIP64_LOCATOR;
    if zip64 {
        let at64 = (at - 20)
            .checked_sub(56)
            .filter(|&i| u32_at(tail, i) == ZIP64_EOCD)
            .ok_or_else(|| bad("no ZIP64 end of central directory"))?;
        entries = u64_at(tail, at64 + 32);
        len = u64_at(tail, at64 + 40);
        recorded = u64_at(tail, at64 + 48);
        end = tail_start + at64 as u64;
    }
    let start = end
        .checked_sub(len)
        .ok_or_else(|| bad("directory larger than the file"))?;
    let shift = start
        .checked_sub(recorded)
        .ok_or_else(|| bad("directory before its recorded offset"))?;
    Ok(Directory {
        start,
        len,
        entries,
        shift,
    })
}

pub fn entries(dir: &Directory, raw: &[u8]) -> Result<Vec<ZipEntry>> {
    let mut out = Vec::with_capacity(dir.entries as usize);
    let mut at = 0;
    while at + 46 <= raw.len() && u32_at(raw, at) == CENTRAL {
        let made_on_unix = raw[at + 5] == 3;
        let method = u16_at(raw, at + 10);
        let crc = u32_at(raw, at + 16);
        let mut compressed = u32_at(raw, at + 20) as u64;
        let mut size = u32_at(raw, at + 24) as u64;
        let name_len = u16_at(raw, at + 28) as usize;
        let extra_len = u16_at(raw, at + 30) as usize;
        let comment_len = u16_at(raw, at + 32) as usize;
        let attributes = u32_at(raw, at + 38);
        let mut header = u32_at(raw, at + 42) as u64;
        let name_at = at + 46;
        let extra_at = name_at + name_len;
        let next = extra_at + extra_len + comment_len;
        if next > raw.len() {
            return Err(bad("truncated directory entry"));
        }
        let name = String::from_utf8_lossy(&raw[name_at..extra_at]).into_owned();
        // ZIP64 sizes and offset, present only for the fields that did not fit.
        let mut e = extra_at;
        while e + 4 <= extra_at + extra_len {
            let (id, n) = (u16_at(raw, e), u16_at(raw, e + 2) as usize);
            if id == 1 {
                let mut f = e + 4;
                for field in [&mut size, &mut compressed, &mut header] {
                    if *field == 0xffff_ffff && f + 8 <= e + 4 + n {
                        *field = u64_at(raw, f);
                        f += 8;
                    }
                }
            }
            e += 4 + n;
        }
        out.push(ZipEntry {
            name,
            method,
            crc,
            compressed,
            size,
            mode: made_on_unix.then_some(attributes >> 16),
            header: header + dir.shift,
        });
        at = next;
    }
    if out.len() as u64 != dir.entries {
        return Err(bad("directory entries do not match its count"));
    }
    Ok(out)
}

/// Length of a local header, from its first 30 bytes.
pub fn local_header_len(head: &[u8]) -> Result<usize> {
    if head.len() < 30 || u32_at(head, 0) != LOCAL {
        return Err(bad("no local file header where the directory points"));
    }
    Ok(30 + u16_at(head, 26) as usize + u16_at(head, 28) as usize)
}

fn bad(detail: &str) -> Error {
    Error::Parse {
        context: "reading a Linux installer",
        detail: detail.into(),
    }
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4)
        .map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()))
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}

/// Builds installers for tests: a script, then a zip whose offsets ignore the script, as
/// MojoSetup's are.
#[cfg(test)]
pub mod build {
    use std::io::Write;

    pub struct File<'a> {
        pub name: &'a str,
        pub data: &'a [u8],
        pub mode: u32,
        pub deflate: bool,
    }

    pub fn installer(script: &[u8], files: &[File], zip64: bool) -> Vec<u8> {
        let mut zip = Vec::new();
        let mut central = Vec::new();
        for f in files {
            let offset = zip.len() as u32;
            let mut crc = flate2::Crc::new();
            crc.update(f.data);
            let data = if f.deflate {
                let mut e =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                e.write_all(f.data).unwrap();
                e.finish().unwrap()
            } else {
                f.data.to_vec()
            };
            let method: u16 = if f.deflate { 8 } else { 0 };
            let common = |out: &mut Vec<u8>| {
                out.extend(20u16.to_le_bytes());
                out.extend(0u16.to_le_bytes());
                out.extend(method.to_le_bytes());
                out.extend([0; 4]);
                out.extend(crc.sum().to_le_bytes());
                out.extend((data.len() as u32).to_le_bytes());
                out.extend((f.data.len() as u32).to_le_bytes());
                out.extend((f.name.len() as u16).to_le_bytes());
            };
            zip.extend(super::LOCAL.to_le_bytes());
            common(&mut zip);
            // A local extra field the directory does not have.
            zip.extend(4u16.to_le_bytes());
            zip.extend(f.name.as_bytes());
            zip.extend([0x55, 0x54, 0, 0]);
            zip.extend(&data);

            central.extend(super::CENTRAL.to_le_bytes());
            central.extend((3u16 << 8 | 20).to_le_bytes());
            common(&mut central);
            central.extend(0u16.to_le_bytes());
            central.extend([0; 6]);
            central.extend((f.mode << 16).to_le_bytes());
            central.extend(offset.to_le_bytes());
            central.extend(f.name.as_bytes());
        }
        let cd_offset = zip.len() as u64;
        zip.extend(&central);
        if zip64 {
            let at = zip.len() as u64;
            zip.extend(super::ZIP64_EOCD.to_le_bytes());
            zip.extend(44u64.to_le_bytes());
            zip.extend([0; 12]);
            zip.extend((files.len() as u64).to_le_bytes());
            zip.extend((files.len() as u64).to_le_bytes());
            zip.extend((central.len() as u64).to_le_bytes());
            zip.extend(cd_offset.to_le_bytes());
            zip.extend(super::ZIP64_LOCATOR.to_le_bytes());
            zip.extend(0u32.to_le_bytes());
            zip.extend(at.to_le_bytes());
            zip.extend(1u32.to_le_bytes());
        }
        zip.extend(super::EOCD.to_le_bytes());
        zip.extend([0; 4]);
        let count = if zip64 { 0xffff } else { files.len() as u16 };
        zip.extend(count.to_le_bytes());
        zip.extend(count.to_le_bytes());
        let (len, offset) = if zip64 {
            (0xffff_ffff, 0xffff_ffff)
        } else {
            (central.len() as u32, cd_offset as u32)
        };
        zip.extend(len.to_le_bytes());
        zip.extend(offset.to_le_bytes());
        zip.extend(0u16.to_le_bytes());
        let mut out = script.to_vec();
        out.extend(zip);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::build::{File, installer};
    use super::*;

    fn read_all(bytes: &[u8]) -> Vec<ZipEntry> {
        let size = bytes.len() as u64;
        let tail = &bytes[bytes.len().saturating_sub(TAIL as usize)..];
        let dir = directory(tail, size).unwrap();
        entries(
            &dir,
            &bytes[dir.start as usize..(dir.start + dir.len) as usize],
        )
        .unwrap()
    }

    fn data_of(bytes: &[u8], e: &ZipEntry) -> Vec<u8> {
        let h = e.header as usize;
        let start = h + local_header_len(&bytes[h..]).unwrap();
        let raw = &bytes[start..start + e.compressed as usize];
        if e.method == DEFLATED {
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::DeflateDecoder::new(raw), &mut out)
                .unwrap();
            out
        } else {
            raw.to_vec()
        }
    }

    #[test]
    fn finds_files_after_the_script_with_and_without_zip64() {
        let files = [
            File {
                name: "data/noarch/start.sh",
                data: b"#!/bin/sh\necho hi\n",
                mode: 0o100755,
                deflate: false,
            },
            File {
                name: "data/noarch/game/data.bin",
                data: &[7u8; 5000],
                mode: 0o100644,
                deflate: true,
            },
            File {
                name: "data/noarch/lib/libfoo.so",
                data: b"libfoo.so.1",
                mode: 0o120777,
                deflate: false,
            },
        ];
        for zip64 in [false, true] {
            let bytes = installer(b"#!/bin/sh\nexit 0\n", &files, zip64);
            let found = read_all(&bytes);
            assert_eq!(found.len(), 3);
            assert_eq!(found[0].mode, Some(0o100755));
            assert!(found[2].is_symlink() && !found[0].is_symlink());
            for (e, f) in found.iter().zip(&files) {
                assert_eq!(e.name, f.name);
                assert_eq!(e.size, f.data.len() as u64);
                assert_eq!(data_of(&bytes, e), f.data, "{zip64}: {}", e.name);
            }
        }
    }

    #[test]
    fn a_file_without_a_zip_is_refused() {
        assert!(directory(&[0u8; 100], 100).is_err());
    }
}
