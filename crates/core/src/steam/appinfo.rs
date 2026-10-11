//! Steam's `appcache/appinfo.vdf`: what the client knows of each app (its name, type, platforms,
//! library images), as binary KeyValues. Layout from SteamDatabase's description
//! (https://github.com/SteamDatabase/SteamAppInfo): versions 40 and 41 are read. Only the apps
//! asked for are decoded; the others are skipped by their size.

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
    let file = std::fs::File::open(path).map_err(|e| Error::io("open appinfo.vdf", e))?;
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
            let keys = Keys {
                strings: strings.as_deref(),
            };
            apps.insert(app_id, Vdf::Section(section(&mut entry, &keys, 0)?));
        }
        at.pos = end;
    }
}

/// A file of binary KeyValues whose keys are written in place, as the client's stats files are
/// (`appcache/stats`); refused when damaged.
pub(crate) fn keyvalues(data: &[u8]) -> Result<Vdf> {
    let mut at = Cursor { data, pos: 0 };
    let entries = section(&mut at, &Keys { strings: None }, 0)?;
    Ok(Vdf::Section(entries))
}

fn string_table(data: &[u8], offset: usize) -> Result<Vec<String>> {
    let mut at = Cursor { data, pos: offset };
    let count = at.u32()? as usize;
    // Each string takes at least its terminating zero.
    if count > data.len() {
        return Err(bad("string table"));
    }
    (0..count).map(|_| at.c_string()).collect()
}

struct Keys<'a> {
    /// Version 41's string table; version 40 writes each key in place.
    strings: Option<&'a [String]>,
}

impl Keys<'_> {
    fn read(&self, at: &mut Cursor<'_>) -> Result<String> {
        match self.strings {
            Some(strings) => {
                let index = at.u32()? as usize;
                strings
                    .get(index)
                    .cloned()
                    .ok_or_else(|| bad("a key outside the string table"))
            }
            None => at.c_string(),
        }
    }
}

fn section(at: &mut Cursor<'_>, keys: &Keys<'_>, depth: usize) -> Result<Vec<(String, Vdf)>> {
    if depth > MAX_DEPTH {
        return Err(bad("nested too deep"));
    }
    let mut entries = Vec::new();
    loop {
        let kind = at.u8()?;
        if kind == 0x08 {
            return Ok(entries);
        }
        let key = keys.read(at)?;
        let value = match kind {
            0x00 => Vdf::Section(section(at, keys, depth + 1)?),
            0x01 => Vdf::Text(at.c_string()?),
            0x02 | 0x04 | 0x06 => Vdf::Text((at.u32()? as i32).to_string()),
            0x03 => Vdf::Text(f32::from_bits(at.u32()?).to_string()),
            0x07 => Vdf::Text(at.u64()?.to_string()),
            0x0A => Vdf::Text((at.u64()? as i64).to_string()),
            other => return Err(bad(&format!("value type {other:#x}"))),
        };
        entries.push((key, value));
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
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

    fn c_string(&mut self) -> Result<String> {
        let rest = self.data.get(self.pos..).unwrap_or_default();
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| bad("a text is not ended"))?;
        let text = String::from_utf8_lossy(&rest[..len]).into_owned();
        self.pos += len + 1;
        Ok(text)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

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
            for (k, v) in [("name", name), ("type", kind), ("oslist", oslist)] {
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
