//! Optional passphrase protection of the sync file. Argon2id turns the passphrase into a key and
//! XChaCha20-Poly1305 encrypts and authenticates the whole `.ami` JSON. The sealed file is still
//! JSON (`{"format":"ami-calculator-sealed", ...}`), so it is easy to tell apart from a plain one.
//!
//! shortcut: the passphrase is an ordinary `String` that is not wiped from memory afterwards, and
//! the key is derived again on every sync (about 0.1 s). Upgrade if the threat grows beyond
//! "someone reads the synced folder".

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

pub const FORMAT_ID: &str = "ami-calculator-sealed";
const VERSION: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
// OWASP's minimum for Argon2id (19 MiB, 2 passes): fine on a phone, slow enough to resist guessing.
const M_KIB: u32 = 19 * 1024;
const T_COST: u32 = 2;
const P_COST: u32 = 1;
// What a file may ask of this device: a hostile file must not be able to exhaust memory or time.
const MAX_M_KIB: u32 = 64 * 1024;
const MAX_T_COST: u32 = 4;
const MAX_P_COST: u32 = 2;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sealed {
    format: String,
    version: u32,
    kdf: String,
    m: u32,
    t: u32,
    p: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}

/// What `open` returns: the text, and the salt to reuse so the next write skips nothing but randomness.
pub struct Opened {
    pub plain: String,
    pub salt: Vec<u8>,
}

pub fn is_sealed(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("format")?.as_str().map(|f| f == FORMAT_ID))
        .unwrap_or(false)
}

fn key(passphrase: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<[u8; 32]> {
    let params =
        Params::new(m, t, p, Some(32)).map_err(|e| CoreError::Validation(e.to_string()))?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| CoreError::Validation(e.to_string()))?;
    Ok(out)
}

/// Header values are authenticated too, so they cannot be swapped without detection.
fn aad(version: u32, m: u32, t: u32, p: u32) -> Vec<u8> {
    format!("{FORMAT_ID}|{version}|argon2id|{m}|{t}|{p}").into_bytes()
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b)
        .map_err(|e| CoreError::Validation(format!("no randomness: {e}")))?;
    Ok(b)
}

/// Encrypts `plain`. Pass the salt returned by `open` to keep it, or `None` for a fresh one.
pub fn seal(plain: &str, passphrase: &str, salt: Option<&[u8]>) -> Result<String> {
    let salt: Vec<u8> = match salt {
        Some(s) => s.to_vec(),
        None => random::<SALT_LEN>()?.to_vec(),
    };
    let nonce = random::<NONCE_LEN>()?; // fresh every write: required for XChaCha20
    let k = key(passphrase, &salt, M_KIB, T_COST, P_COST)?;
    let ciphertext = XChaCha20Poly1305::new((&k).into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain.as_bytes(),
                aad: &aad(VERSION, M_KIB, T_COST, P_COST),
            },
        )
        .map_err(|_| CoreError::WrongPassphrase)?;
    Ok(serde_json::to_string_pretty(&Sealed {
        format: FORMAT_ID.into(),
        version: VERSION,
        kdf: "argon2id".into(),
        m: M_KIB,
        t: T_COST,
        p: P_COST,
        salt: B64.encode(&salt),
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ciphertext),
    })?)
}

pub fn open(text: &str, passphrase: &str) -> Result<Opened> {
    let s: Sealed = serde_json::from_str(text).map_err(|_| CoreError::NotAnExport)?;
    if s.format != FORMAT_ID || s.kdf != "argon2id" {
        return Err(CoreError::NotAnExport);
    }
    if s.version > VERSION {
        return Err(CoreError::UnsupportedVersion(s.version));
    }
    if s.m > MAX_M_KIB || s.t > MAX_T_COST || s.p > MAX_P_COST {
        return Err(CoreError::Validation(
            "protected file asks for too much work".into(),
        ));
    }
    let decode = |v: &str| B64.decode(v).map_err(|_| CoreError::NotAnExport);
    let (salt, nonce, ciphertext) = (decode(&s.salt)?, decode(&s.nonce)?, decode(&s.ciphertext)?);
    if nonce.len() != NONCE_LEN || salt.len() < 8 {
        return Err(CoreError::NotAnExport);
    }
    let k = key(passphrase, &salt, s.m, s.t, s.p)?;
    let plain = XChaCha20Poly1305::new((&k).into())
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad(s.version, s.m, s.t, s.p),
            },
        )
        .map_err(|_| CoreError::WrongPassphrase)?;
    Ok(Opened {
        plain: String::from_utf8(plain).map_err(|_| CoreError::NotAnExport)?,
        salt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(text: &str, f: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
        f(&mut v);
        v.to_string()
    }

    #[test]
    fn round_trip_hides_the_text_and_can_keep_the_salt() {
        let sealed = seal("secret portfolio IKE", "correct horse", None).unwrap();
        assert!(is_sealed(&sealed));
        assert!(!sealed.contains("IKE"));
        let opened = open(&sealed, "correct horse").unwrap();
        assert_eq!(opened.plain, "secret portfolio IKE");

        let again = seal("changed", "correct horse", Some(&opened.salt)).unwrap();
        assert_eq!(open(&again, "correct horse").unwrap().salt, opened.salt);
        assert_ne!(
            sealed,
            seal("secret portfolio IKE", "correct horse", Some(&opened.salt)).unwrap(),
            "nonce is fresh"
        );
    }

    #[test]
    fn a_wrong_passphrase_is_refused() {
        let sealed = seal("data", "right", None).unwrap();
        assert!(matches!(
            open(&sealed, "wrong"),
            Err(CoreError::WrongPassphrase)
        ));
        assert!(matches!(open(&sealed, ""), Err(CoreError::WrongPassphrase)));
    }

    #[test]
    fn any_tampering_is_detected() {
        let sealed = seal("data", "pw", None).unwrap();
        let flip = |field: &'static str| {
            edit(&sealed, |v| {
                let s = v[field].as_str().unwrap().to_string();
                let mut raw = B64.decode(s).unwrap();
                raw[0] ^= 1;
                v[field] = B64.encode(raw).into();
            })
        };
        for field in ["ciphertext", "nonce"] {
            assert!(
                matches!(open(&flip(field), "pw"), Err(CoreError::WrongPassphrase)),
                "{field}"
            );
        }
        let weaker = edit(&sealed, |v| v["t"] = 1.into()); // cost lowered by an attacker
        assert!(matches!(
            open(&weaker, "pw"),
            Err(CoreError::WrongPassphrase)
        ));
    }

    #[test]
    fn a_file_cannot_demand_unreasonable_work_or_pose_as_something_else() {
        let sealed = seal("data", "pw", None).unwrap();
        let greedy = edit(&sealed, |v| v["m"] = 4_000_000.into());
        assert!(matches!(open(&greedy, "pw"), Err(CoreError::Validation(_))));
        let newer = edit(&sealed, |v| v["version"] = 99.into());
        assert!(matches!(
            open(&newer, "pw"),
            Err(CoreError::UnsupportedVersion(99))
        ));
        assert!(!is_sealed(
            r#"{"format":"ami-calculator-export","version":1}"#
        ));
        assert!(!is_sealed("not json"));
    }

    #[test]
    fn short_or_empty_ciphertext_is_an_error_not_a_panic() {
        let sealed = seal("data", "pw", None).unwrap();
        for ct in ["", "AA=="] {
            let cut = edit(&sealed, |v| v["ciphertext"] = ct.into());
            assert!(
                matches!(open(&cut, "pw"), Err(CoreError::WrongPassphrase)),
                "{ct:?}"
            );
        }
        assert_eq!(
            open(&seal("", "pw", None).unwrap(), "pw").unwrap().plain,
            "",
            "empty text round-trips"
        );
    }
}
