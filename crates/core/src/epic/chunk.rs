//! Epic's chunks, as downloaded: a header, then their data, compressed and maybe encrypted, as
//! legendary reads them (`legendary/models/chunk.py`).

use std::collections::HashMap;

use super::manifest::{ChunkInfo, guid_hex};
use crate::error::{Error, Result};

const MAGIC: u32 = 0xB1FE_3AA2;
const COMPRESSED: u8 = 0x1;
const ENCRYPTED: u8 = 0x2;

/// The data of a downloaded chunk, unpacked and checked to be the chunk the manifest describes, of
/// its size. Its content is checked with the files made of it.
pub fn unpack(raw: &[u8], info: &ChunkInfo, secrets: &HashMap<String, String>) -> Result<Vec<u8>> {
    let bad = |detail: &str| Error::parse("an Epic chunk", detail);
    let field = |at: usize, n: usize| raw.get(at..at + n).ok_or_else(|| bad("it ends too early"));
    let u32_at = |at| field(at, 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    if u32_at(0)? != MAGIC {
        return Err(bad("not a chunk"));
    }
    let version = u32_at(4)?;
    let header_size = u32_at(8)? as usize;
    let compressed_size = u32_at(12)? as usize;
    let guid = [u32_at(16)?, u32_at(20)?, u32_at(24)?, u32_at(28)?];
    let stored_as = field(40, 1)?[0];
    let sha = if version >= 2 {
        Some(field(41, 20)?)
    } else {
        None
    };
    let (secret, tag) = if version >= 4 {
        let secret = [u32_at(66)?, u32_at(70)?, u32_at(74)?, u32_at(78)?];
        (Some(secret), Some(field(82, 16)?))
    } else {
        (None, None)
    };
    if guid != info.guid {
        return Err(Error::Refused(
            "Epic's server sent another chunk than the one asked for".into(),
        ));
    }
    let data = raw
        .get(header_size..)
        .ok_or_else(|| bad("it ends too early"))?;
    let data = &data[..compressed_size.min(data.len())];
    let decrypted;
    let data = if stored_as & ENCRYPTED != 0 {
        let (Some(secret), Some(tag), Some(sha)) = (secret, tag, sha) else {
            return Err(bad("an encrypted chunk names no key"));
        };
        let key = secrets.get(&guid_hex(&secret)).ok_or_else(|| {
            Error::Unsupported("Epic did not give the key to this build's chunks".into())
        })?;
        decrypted = super::decrypt(key, &sha[..12], data, tag)?;
        &decrypted[..]
    } else {
        data
    };
    let size = info.window_size as usize;
    if stored_as & COMPRESSED != 0 {
        return super::manifest::inflate(data, size as u64);
    }
    if data.len() < size {
        return Err(bad("it is smaller than the manifest says"));
    }
    Ok(data[..size].to_vec())
}

/// A chunk as Epic writes one (header version 3, or 4 when `secret` is given), for tests.
#[cfg(test)]
pub(crate) fn pack(info: &ChunkInfo, data: &[u8], secret: Option<(&str, [u32; 4])>) -> Vec<u8> {
    use std::io::Write;
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(data).unwrap();
    let mut body = z.finish().unwrap();
    let sha = super::sha1(data);
    let mut stored_as = COMPRESSED;
    let mut tag = vec![0u8; 16];
    if let Some((key, _)) = secret {
        let (ciphertext, t) = super::encrypt(key, &sha[..12], &body);
        body = ciphertext;
        tag = t;
        stored_as |= ENCRYPTED;
    }
    let version: u32 = if secret.is_some() { 4 } else { 3 };
    let header_size: u32 = if secret.is_some() { 98 } else { 66 };
    let mut out = Vec::new();
    out.extend(MAGIC.to_le_bytes());
    out.extend(version.to_le_bytes());
    out.extend(header_size.to_le_bytes());
    out.extend((body.len() as u32).to_le_bytes());
    info.guid.iter().for_each(|g| out.extend(g.to_le_bytes()));
    out.extend(info.hash.to_le_bytes());
    out.push(stored_as);
    out.extend(sha);
    out.push(3);
    out.extend((data.len() as u32).to_le_bytes());
    if let Some((_, id)) = secret {
        id.iter().for_each(|g| out.extend(g.to_le_bytes()));
        out.extend(tag);
    }
    out.extend(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> ChunkInfo {
        ChunkInfo {
            guid: [1, 2, 3, 4],
            hash: 5,
            group: 6,
            window_size: 1000,
            file_size: 0,
            secret: None,
        }
    }

    #[test]
    fn a_chunk_unpacks_to_its_data() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let raw = pack(&info(), &data, None);
        assert_eq!(unpack(&raw, &info(), &HashMap::new()).unwrap(), data);
    }

    #[test]
    fn an_encrypted_chunk_unpacks_with_its_key_alone() {
        let data = vec![42u8; 1000];
        let key = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let id = [9, 9, 9, 9];
        let raw = pack(&info(), &data, Some((key, id)));
        let secrets = HashMap::from([(guid_hex(&id), key.to_string())]);
        assert_eq!(unpack(&raw, &info(), &secrets).unwrap(), data);
        assert!(matches!(
            unpack(&raw, &info(), &HashMap::new()),
            Err(Error::Unsupported(_))
        ));
    }

    /// Another chunk, a damaged or truncated one, or one of another size is an error, never a
    /// panic.
    #[test]
    fn a_wrong_chunk_is_refused() {
        let data = vec![1u8; 1000];
        let raw = pack(&info(), &data, None);
        let mut other = info();
        other.guid = [4, 3, 2, 1];
        assert!(unpack(&raw, &other, &HashMap::new()).is_err());
        let mut bigger = info();
        bigger.window_size = 2000;
        assert!(unpack(&raw, &bigger, &HashMap::new()).is_err());
        for cut in [0, 10, 40, 65, 70, raw.len() - 2] {
            assert!(
                unpack(&raw[..cut], &info(), &HashMap::new()).is_err(),
                "{cut}"
            );
        }
    }
}
