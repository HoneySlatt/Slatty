//! The Epic Games Store: signing in, the account's library, and installing its games. Epic
//! publishes no API for this; what is known of its services comes from legendary
//! (derrod/legendary, GPL-3.0: `legendary/api/egs.py`, `legendary/core.py`, `legendary/models`,
//! `legendary/downloader`), see docs/epic-integration.md.

pub mod achievements;
pub mod auth;
pub mod chunk;
pub mod cloud;
pub mod download;
pub mod install;
pub mod launch;
pub mod library;
pub mod manifest;

use crate::error::{Error, Result};

/// The user agent of the Epic Games Launcher legendary sends, so that Epic's services answer
/// Slatty as they answer the launcher whose OAuth client it signs in with.
const USER_AGENT: &str =
    "UELauncher/11.0.1-14907503+++Portal+Release-Live Windows/10.0.19041.1.256.64bit";

/// SHA-1, which Epic checks its manifests and files with.
pub(crate) fn sha1(data: &[u8]) -> [u8; 20] {
    let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA1_FOR_LEGACY_USE_ONLY, data);
    digest.as_ref().try_into().expect("SHA-1 is 20 bytes")
}

/// Decrypts what Epic encrypted with AES-GCM: a key given in hexadecimal (128 or 256 bits), a
/// 12-byte nonce, the tag apart.
pub(crate) fn decrypt(
    key_hex: &str,
    nonce: &[u8],
    ciphertext: &[u8],
    tag: &[u8],
) -> Result<Vec<u8>> {
    use aws_lc_rs::aead::{AES_128_GCM, AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
    let refused = || Error::Refused("an Epic key does not open what it should".into());
    let key = unhex(key_hex).ok_or_else(refused)?;
    let algorithm = match key.len() {
        16 => &AES_128_GCM,
        32 => &AES_256_GCM,
        _ => return Err(refused()),
    };
    let key = LessSafeKey::new(UnboundKey::new(algorithm, &key).map_err(|_| refused())?);
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| refused())?;
    let mut data = [ciphertext, tag].concat();
    let plain = key
        .open_in_place(nonce, Aad::empty(), &mut data)
        .map_err(|_| refused())?;
    Ok(plain.to_vec())
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Encrypts as Epic does, for tests.
#[cfg(test)]
pub(crate) fn encrypt(key_hex: &str, nonce: &[u8], plain: &[u8]) -> (Vec<u8>, Vec<u8>) {
    use aws_lc_rs::aead::{AES_128_GCM, AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
    let key = unhex(key_hex).unwrap();
    let algorithm = if key.len() == 16 {
        &AES_128_GCM
    } else {
        &AES_256_GCM
    };
    let key = LessSafeKey::new(UnboundKey::new(algorithm, &key).unwrap());
    let mut data = plain.to_vec();
    let tag = key
        .seal_in_place_separate_tag(
            Nonce::try_assume_unique_for_key(nonce).unwrap(),
            Aad::empty(),
            &mut data,
        )
        .unwrap();
    (data, tag.as_ref().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_is_sha1() {
        assert_eq!(
            crate::fsutil::hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn what_a_key_encrypted_it_decrypts_and_nothing_else() {
        let key = "000102030405060708090a0b0c0d0e0f";
        let nonce = [7u8; 12];
        let (ciphertext, tag) = encrypt(key, &nonce, b"[FAKE] names");
        assert_eq!(
            decrypt(key, &nonce, &ciphertext, &tag).unwrap(),
            b"[FAKE] names"
        );
        let other = "0f0e0d0c0b0a09080706050403020100";
        assert!(decrypt(other, &nonce, &ciphertext, &tag).is_err());
        assert!(decrypt("zz", &nonce, &ciphertext, &tag).is_err());
        assert!(decrypt(key, &[7u8; 8], &ciphertext, &tag).is_err());
    }
}
