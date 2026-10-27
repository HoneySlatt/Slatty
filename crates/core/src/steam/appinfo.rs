//! Steam's `appcache/appinfo.vdf`: what the client knows of each app (its name, type, platforms,
//! library images), as binary KeyValues. Layout from SteamDatabase's description
//! (https://github.com/SteamDatabase/SteamAppInfo): versions 40 and 41 are read. Only the apps
//! asked for are decoded; the others are skipped by their size.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::vdf::Vdf;
use crate::error::{Error, Result};

const V40: u32 = 0x0756_4428;
const V41: u32 = 0x0756_4429;
/// The file is a few MB; larger is not Steam's.
const MAX_FILE: u64 = 512 << 20;
const MAX_DEPTH: usize = 32;
/// Bytes between an entry's size and its KeyValues: info state, last change, PICS token, SHA-1,
/// change number and the binary data's SHA-1.
const ENTRY_HEADER: usize = 4 + 4 + 8 + 20 + 4 + 20;

fn bad(what: &str) -> Error {
    Error::parse("Steam's appinfo.vdf", what.to_string())
}

/// The KeyValues of each app of `wanted` the file knows, by app id.
pub fn read(path: &Path, wanted: &HashSet<u32>) -> Result<HashMap<u32, Vdf>> {
    let file = super::open(path).map_err(|e| Error::io("open appinfo.vdf", e))?;
    let mut data = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_FILE), &mut data)
        .map_err(|e| Error::io("read appinfo.vdf", e))?;
    parse(&data, wanted)
}

pub(crate) fn parse(data: &[u8], wanted: &HashSet<u32>) -> Result<HashMap<u32, Vdf>> {
    let mut at = Cursor { data, pos: 0 };
    let magic = at.u32()?;
    at.u32()?; // universe
    let strings = match magic {
        V41 => {
            let offset = usize::try_from(at.u64()?).map_err(|_| bad("string table offset"))?;
            Some(string_table(data, offset)?)
        }
        V40 => None,
        _ => return Err(bad(&format!("unknown version {magic:#x}"))),
    };
    let mut apps = HashMap::new();
    let keys = Keys::new(strings.as_deref());
    loop {
        let app_id = at.u32()?;
        if app_id == 0 {
            return Ok(apps);
        }
        let size = at.u32()? as usize;
        let end = at.pos.checked_add(size).filter(|&e| e <= data.len());
        let end = end.ok_or_else(|| bad("an entry is cut short"))?;
        if wanted.contains(&app_id) {
            let mut entry = Cursor {
                data: &data[..end],
                pos: at.pos + ENTRY_HEADER,
            };
            apps.insert(
                app_id,
                Vdf::Section(section(&mut entry, &keys, 0, Some(shown))?),
            );
        }
        at.pos = end;
    }
}

/// A file of binary KeyValues whose keys are written in place, as the client's stats files are
/// (`appcache/stats`); refused when damaged.
pub(crate) fn keyvalues(data: &[u8]) -> Result<Vdf> {
    let mut at = Cursor { data, pos: 0 };
    let entries = section(&mut at, &Keys::new(None), 0, None)?;
    Ok(Vdf::Section(entries))
}

fn string_table(data: &[u8], offset: usize) -> Result<Vec<String>> {
    let mut at = Cursor { data, pos: offset };
    let count = at.u32()? as usize;
    // Each string takes at least its terminating zero in the file, and a `String` once read, empty
    // or not: bounded like the rest of what the file decodes into.
    if count > data.len() || count > DECODED / std::mem::size_of::<String>() {
        return Err(bad("string table"));
    }
    (0..count)
        .map(|_| at.c_str().map(Cow::into_owned))
        .collect()
}

struct Keys<'a> {
    /// Version 41's string table; version 40 writes each key in place.
    strings: Option<&'a [String]>,
    /// Bytes the decoded entries may still take: a key of the string table can be used any number
    /// of times, so a small file could otherwise ask for gigabytes.
    left: std::cell::Cell<usize>,
}

/// What one file's decoded entries may take, far more than Steam's (some MB).
const DECODED: usize = 256 << 20;
/// What an entry takes besides its text.
const ENTRY: usize = 64;

impl<'a> Keys<'a> {
    fn new(strings: Option<&'a [String]>) -> Keys<'a> {
        Keys {
            strings,
            left: std::cell::Cell::new(DECODED),
        }
    }

    /// Takes `bytes` of what is left to decode.
    fn spend(&self, bytes: usize) -> Result<()> {
        let left = self.left.get().checked_sub(bytes + ENTRY);
        self.left
            .set(left.ok_or_else(|| bad("too large once read"))?);
        Ok(())
    }

    /// The next key, borrowed from the string table or from the file.
    fn key(&self, at: &mut Cursor<'a>) -> Result<Cow<'a, str>> {
        match self.strings {
            Some(strings) => {
                let index = at.u32()? as usize;
                strings
                    .get(index)
                    .map(|key| Cow::Borrowed(key.as_str()))
                    .ok_or_else(|| bad("a key outside the string table"))
            }
            None => at.c_str(),
        }
    }
}

/// Which entries of an app are decoded, by depth and key: what the library shows (`common`'s
/// name, type, systems, images and icon) and the app's id. The rest, most of the file (`config`,
/// `depots`, `extended`, the store's texts in every language), is read past, not decoded: about
/// 60 KB an app otherwise.
fn shown(depth: usize, key: &str) -> bool {
    let is = |name: &str| key.eq_ignore_ascii_case(name);
    match depth {
        0 => is("appinfo"),
        1 => is("appid") || is("common"),
        2 => is("name") || is("type") || is("oslist") || is("library_assets_full") || is("icon"),
        _ => true,
    }
}

/// The entries of a section, those `keep` takes (by depth and key) when given.
fn section<'a>(
    at: &mut Cursor<'a>,
    keys: &Keys<'a>,
    depth: usize,
    keep: Option<fn(usize, &str) -> bool>,
) -> Result<Vec<(String, Vdf)>> {
    if depth > MAX_DEPTH {
        return Err(bad("nested too deep"));
    }
    let mut entries = Vec::new();
    loop {
        let kind = at.u8()?;
        if kind == 0x08 {
            return Ok(entries);
        }
        let key = keys.key(at)?;
        if keep.is_some_and(|keep| !keep(depth, &key)) {
            skip(at, keys, kind, depth + 1)?;
            continue;
        }
        keys.spend(key.len())?;
        let value = match kind {
            0x00 => Vdf::Section(section(at, keys, depth + 1, keep)?),
            0x01 => Vdf::Text(at.c_str()?.into_owned()),
            0x02 | 0x04 | 0x06 => Vdf::Text((at.u32()? as i32).to_string()),
            0x03 => Vdf::Text(f32::from_bits(at.u32()?).to_string()),
            0x07 => Vdf::Text(at.u64()?.to_string()),
            0x0A => Vdf::Text((at.u64()? as i64).to_string()),
            other => return Err(bad(&format!("value type {other:#x}"))),
        };
        if let Vdf::Text(text) = &value {
            keys.spend(text.len())?;
        }
        entries.push((key.into_owned(), value));
    }
}

/// Reads past a value of `kind` without decoding it.
fn skip<'a>(at: &mut Cursor<'a>, keys: &Keys<'a>, kind: u8, depth: usize) -> Result<()> {
    match kind {
        0x00 => {
            if depth > MAX_DEPTH {
                return Err(bad("nested too deep"));
            }
            loop {
                let kind = at.u8()?;
                if kind == 0x08 {
                    return Ok(());
                }
                keys.key(at)?;
                skip(at, keys, kind, depth + 1)?;
            }
        }
        0x01 => at.c_str().map(drop),
        0x02..=0x04 | 0x06 => at.take(4).map(drop),
        0x07 | 0x0A => at.take(8).map(drop),
        other => Err(bad(&format!("value type {other:#x}"))),
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| bad("cut short"))?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("8 bytes"),
        ))
    }

    /// A text ended by a zero, borrowed from the file when it is UTF-8.
    fn c_str(&mut self) -> Result<Cow<'a, str>> {
        let rest = self.data.get(self.pos..).unwrap_or_default();
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| bad("a text is not ended"))?;
        self.pos += len + 1;
        Ok(String::from_utf8_lossy(&rest[..len]))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A string table of millions of empty keys would take two dozen times its size once read:
    /// what it may take is bounded like the rest of what a file decodes into.
    #[test]
    fn a_string_table_of_millions_of_empty_keys_is_refused() {
        let count: u32 = 12_000_000;
        let mut data = count.to_le_bytes().to_vec();
        data.resize(4 + count as usize, 0);
        assert!(string_table(&data, 0).is_err());
    }

    /// Builds an appinfo.vdf of version 41 holding `apps` (app id, name, type, platforms).
    pub(crate) fn fake_appinfo(apps: &[(u32, &str, &str, &str)]) -> Vec<u8> {
        let mut strings: Vec<&str> = Vec::new();
        let mut key = |name: &'static str, out: &mut Vec<u8>| {
            let index = strings.iter().position(|s| *s == name).unwrap_or_else(|| {
                strings.push(name);
                strings.len() - 1
            });
            out.extend((index as u32).to_le_bytes());
        };
        let mut body = Vec::new();
        for &(app_id, name, kind, oslist) in apps {
            let mut kv = Vec::new();
            kv.push(0x00);
            key("appinfo", &mut kv);
            kv.push(0x02);
            key("appid", &mut kv);
            kv.extend(app_id.to_le_bytes());
            kv.push(0x00);
            key("common", &mut kv);
            let icon = format!("{app_id:040x}");
            for (k, v) in [
                ("name", name),
                ("type", kind),
                ("oslist", oslist),
                ("icon", icon.as_str()),
            ] {
                kv.push(0x01);
                key(k, &mut kv);
                kv.extend(v.as_bytes());
                kv.push(0);
            }
            kv.extend([0x08, 0x08, 0x08]);
            body.extend(app_id.to_le_bytes());
            body.extend(((ENTRY_HEADER + kv.len()) as u32).to_le_bytes());
            body.extend([0u8; ENTRY_HEADER]);
            body.extend(kv);
        }
        body.extend(0u32.to_le_bytes());
        let mut file = Vec::new();
        file.extend(V41.to_le_bytes());
        file.extend(1u32.to_le_bytes());
        let offset = 16 + body.len() as u64;
        file.extend(offset.to_le_bytes());
        file.extend(body);
        file.extend((strings.len() as u32).to_le_bytes());
        for s in strings {
            file.extend(s.as_bytes());
            file.push(0);
        }
        file
    }

    /// Version 40 (keys written in place), with what the library does not show beside `common`.
    #[test]
    fn only_what_the_library_shows_is_decoded() {
        let text = |out: &mut Vec<u8>, s: &str| {
            out.extend(s.as_bytes());
            out.push(0);
        };
        let mut kv = vec![0x00];
        text(&mut kv, "appinfo");
        kv.push(0x02);
        text(&mut kv, "appid");
        kv.extend(5u32.to_le_bytes());
        kv.push(0x00);
        text(&mut kv, "common");
        for (k, v) in [
            ("name", "[FAKE] Five"),
            ("type", "game"),
            ("review_score", "9"),
        ] {
            kv.push(0x01);
            text(&mut kv, k);
            text(&mut kv, v);
        }
        kv.push(0x08);
        kv.push(0x00);
        text(&mut kv, "depots");
        kv.push(0x00);
        text(&mut kv, "1");
        kv.push(0x07);
        text(&mut kv, "size");
        kv.extend(7u64.to_le_bytes());
        kv.extend([0x08, 0x08, 0x08, 0x08]);
        let mut data = Vec::new();
        data.extend(V40.to_le_bytes());
        data.extend(1u32.to_le_bytes());
        data.extend(5u32.to_le_bytes());
        data.extend(((ENTRY_HEADER + kv.len()) as u32).to_le_bytes());
        data.extend([0u8; ENTRY_HEADER]);
        data.extend(kv);
        data.extend(0u32.to_le_bytes());

        let apps = parse(&data, &HashSet::from([5])).unwrap();
        let app = apps[&5].get("appinfo").unwrap();
        assert_eq!(app.text("appid"), Some("5"));
        let common = app.get("common").unwrap();
        assert_eq!(common.text("name"), Some("[FAKE] Five"));
        assert_eq!(common.text("review_score"), None, "read past");
        assert!(app.get("depots").is_none(), "read past");
    }

    /// A key of version 41's string table can be used any number of times: what the entries take
    /// once decoded is bounded, whatever the file asks.
    #[test]
    fn keys_used_over_and_over_cannot_take_more_than_their_share() {
        let table = vec!["k".repeat(1000)];
        let mut kv = Vec::new();
        for _ in 0..100 {
            kv.push(0x01);
            kv.extend(0u32.to_le_bytes());
            kv.push(0);
        }
        kv.push(0x08);
        let keys = |left: usize| Keys {
            strings: Some(&table),
            left: std::cell::Cell::new(left),
        };
        let mut at = Cursor { data: &kv, pos: 0 };
        assert!(section(&mut at, &keys(200_000), 0, None).is_ok());
        let mut at = Cursor { data: &kv, pos: 0 };
        let refused = section(&mut at, &keys(50_000), 0, None);
        assert!(refused.is_err(), "100 keys of 1,000 bytes, 50,000 allowed");
    }

    #[test]
    fn the_apps_asked_for_are_read() {
        let data = fake_appinfo(&[
            (440, "[FAKE] Fortress", "Game", "windows,linux"),
            (620, "[FAKE] Portal", "Game", "windows"),
            (228980, "[FAKE] Redistributables", "Tool", ""),
        ]);
        let apps = parse(&data, &HashSet::from([440, 228980, 999])).unwrap();
        assert_eq!(apps.len(), 2);
        let common = apps[&440].get("appinfo").unwrap().get("common").unwrap();
        assert_eq!(common.text("name"), Some("[FAKE] Fortress"));
        assert_eq!(common.text("oslist"), Some("windows,linux"));
        assert_eq!(
            apps[&440].get("appinfo").unwrap().text("appid"),
            Some("440")
        );
    }

    #[test]
    fn a_damaged_file_is_refused_not_trusted() {
        let data = fake_appinfo(&[(440, "[FAKE] Fortress", "Game", "windows")]);
        let all = HashSet::from([440]);
        // Cut anywhere, the file is refused rather than read past its end.
        for cut in [0, 3, 15, 20, 40, data.len() / 2, data.len() - 1] {
            assert!(parse(&data[..cut], &all).is_err(), "cut at {cut}");
        }
        let mut wrong = data.clone();
        wrong[0] = 0x27;
        assert!(parse(&wrong, &all).is_err(), "another version");
        // A size leading past the file's end.
        let mut long = data.clone();
        long[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&long, &all).is_err());
    }
}
