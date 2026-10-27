//! Epic's chunks, as downloaded: a header, then their data, compressed and maybe encrypted, as
//! legendary reads them (`legendary/models/chunk.py`).

use std::collections::HashMap;

use super::manifest::{ChunkInfo, Guid, guid_hex};
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

/// The data a chunk of a cloud save holds once unpacked: every one is padded to it.
pub(crate) const SAVE_CHUNK: usize = 1 << 20;

/// A chunk of a cloud save as legendary packs one (`SaveGameHelper.finalize_chunk`,
/// `Chunk.write`): `data` padded with zeros to `SAVE_CHUNK`, its rolling hash and SHA-1 taken over
/// that, compressed, under a header of version 3. Its description, its SHA-1 and its bytes.
pub(crate) fn pack_save(guid: Guid, data: &[u8]) -> (ChunkInfo, [u8; 20], Vec<u8>) {
    use std::io::Write;
    let mut padded = data.to_vec();
    padded.resize(SAVE_CHUNK, 0);
    let sha = super::sha1(&padded);
    let hash = rolling_hash(&padded);
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&padded).expect("writes to memory");
    let body = z.finish().expect("writes to memory");
    let mut out = Vec::with_capacity(66 + body.len());
    out.extend(MAGIC.to_le_bytes());
    out.extend(3u32.to_le_bytes());
    out.extend(66u32.to_le_bytes());
    out.extend((body.len() as u32).to_le_bytes());
    guid.iter().for_each(|g| out.extend(g.to_le_bytes()));
    out.extend(hash.to_le_bytes());
    out.push(COMPRESSED);
    out.extend(sha);
    // Both hashes given.
    out.push(3);
    out.extend((SAVE_CHUNK as u32).to_le_bytes());
    out.extend(body);
    let info = ChunkInfo {
        guid,
        hash,
        group: group(&guid),
        window_size: SAVE_CHUNK as u32,
        file_size: out.len() as u64,
        secret: None,
    };
    (info, sha, out)
}

/// The folder number of a chunk's address (`ChunkInfo.group_num`): the CRC-32 of its guid, as
/// little-endian words, modulo 100.
fn group(guid: &Guid) -> u8 {
    let mut crc = flate2::Crc::new();
    guid.iter().for_each(|g| crc.update(&g.to_le_bytes()));
    (crc.sum() % 100) as u8
}

/// Epic's rolling hash of a chunk (`legendary/utils/rolling_hash.py`).
pub(crate) fn rolling_hash(data: &[u8]) -> u64 {
    const POLY: u64 = 0xC96C_5795_D787_0F42;
    const TABLE: [u64; 256] = {
        let mut table = [0u64; 256];
        let mut i = 0;
        while i < 256 {
            let mut v = i as u64;
            let mut bit = 0;
            while bit < 8 {
                v = if v & 1 == 1 { (v >> 1) ^ POLY } else { v >> 1 };
                bit += 1;
            }
            table[i] = v;
            i += 1;
        }
        table
    };
    data.iter()
        .fold(0u64, |h, &b| h.rotate_left(1) ^ TABLE[b as usize])
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

    /// A cloud save's chunk reads back as its data padded to 1 MiB, its hashes taken over that.
    #[test]
    fn a_save_chunk_reads_back_padded() {
        let (info, sha, raw) = pack_save([1, 2, 3, 4], b"[FAKE] save");
        let data = unpack(&raw, &info, &HashMap::new()).unwrap();
        assert_eq!(data.len(), SAVE_CHUNK);
        assert!(data.starts_with(b"[FAKE] save") && data[11..].iter().all(|b| *b == 0));
        assert_eq!(sha, super::super::sha1(&data));
        assert_eq!(info.hash, rolling_hash(&data));
        assert!(info.group < 100);
    }

    /// Its table is CRC-64/XZ's (the same polynomial, reflected), whose entries are known.
    #[test]
    fn the_rolling_hash_is_epics() {
        assert_eq!(rolling_hash(b""), 0);
        assert_eq!(rolling_hash(&[0x80]), 0xC96C_5795_D787_0F42);
        assert_eq!(rolling_hash(&[0x01]), 0xB32E_4CBE_03A7_5F6F);
        assert_eq!(
            rolling_hash(&[0x01, 0x00]),
            0xB32E_4CBE_03A7_5F6F_u64.rotate_left(1)
        );
    }

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
