//! Encrypt / decrypt / fingerprint. All key material is zeroized on drop.
//!
//! - Argon2id (m = 64 MiB, t = 3, p = 1) turns the master password into 64 bytes:
//!   `enc_key` (XChaCha20-Poly1305) and `fp_key` (HMAC-SHA256 fingerprints).
//! - Every value gets a fresh random 24-byte nonce. The AAD binds a ciphertext to its row.

use crate::error::{LokiError, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, TryRngCore};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const SALT_LEN: usize = 16;

const VERIFIER_PLAINTEXT: &[u8] = b"LOKI-VAULT-VERIFIER-v1";
const VERIFIER_AAD: &[u8] = b"loki-verifier";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KdfParams {
    pub m_cost: u32, // KiB
    pub t_cost: u32,
    pub p_cost: u32,
}

impl KdfParams {
    pub const DEFAULT: KdfParams = KdfParams {
        m_cost: 64 * 1024,
        t_cost: 3,
        p_cost: 1,
    };
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct VaultKeys {
    enc_key: [u8; KEY_LEN],
    fp_key: [u8; KEY_LEN],
}

// Deliberately not derived: never print key bytes.
impl std::fmt::Debug for VaultKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VaultKeys(..)")
    }
}

pub struct Sealed {
    pub nonce: [u8; NONCE_LEN],
    pub ct: Vec<u8>,
}

pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    OsRng.try_fill_bytes(&mut buf).map_err(|_| LokiError::Crypto)?;
    Ok(buf)
}

/// Uniform random number in `0..n` (rejection sampling, no modulo bias).
pub fn random_index(n: u32) -> Result<u32> {
    if n == 0 {
        return Err(LokiError::Crypto);
    }
    let limit = u32::MAX - (u32::MAX % n);
    loop {
        let x = u32::from_le_bytes(random_bytes::<4>()?);
        if x < limit {
            return Ok(x % n);
        }
    }
}

pub fn derive_keys(password: &[u8], salt: &[u8], p: KdfParams) -> Result<VaultKeys> {
    let params = Params::new(p.m_cost, p.t_cost, p.p_cost, Some(64)).map_err(|_| LokiError::Crypto)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 64]);
    argon
        .hash_password_into(password, salt, &mut out[..])
        .map_err(|_| LokiError::Crypto)?;
    let mut enc_key = [0u8; KEY_LEN];
    let mut fp_key = [0u8; KEY_LEN];
    enc_key.copy_from_slice(&out[..KEY_LEN]);
    fp_key.copy_from_slice(&out[KEY_LEN..]);
    Ok(VaultKeys { enc_key, fp_key })
}

impl VaultKeys {
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<Sealed> {
        let nonce = random_bytes::<NONCE_LEN>()?;
        let cipher = XChaCha20Poly1305::new(&Key::from(self.enc_key));
        let ct = cipher
            .encrypt(&XNonce::from(nonce), Payload { msg: plaintext, aad })
            .map_err(|_| LokiError::Crypto)?;
        Ok(Sealed { nonce, ct })
    }

    pub fn open(&self, aad: &[u8], nonce: &[u8], ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| LokiError::Crypto)?;
        let cipher = XChaCha20Poly1305::new(&Key::from(self.enc_key));
        let pt = cipher
            .decrypt(&XNonce::from(nonce), Payload { msg: ct, aad })
            .map_err(|_| LokiError::Crypto)?;
        Ok(Zeroizing::new(pt))
    }

    /// HMAC-SHA256 of a value. Equal fingerprint means equal value, within one vault.
    pub fn fingerprint(&self, value: &[u8]) -> Result<[u8; 32]> {
        let mut mac =
            <Hmac<Sha256> as Mac>::new_from_slice(&self.fp_key).map_err(|_| LokiError::Crypto)?;
        mac.update(value);
        Ok(mac.finalize().into_bytes().into())
    }

    pub fn make_verifier(&self) -> Result<Sealed> {
        self.seal(VERIFIER_AAD, VERIFIER_PLAINTEXT)
    }

    pub fn check_verifier(&self, nonce: &[u8], ct: &[u8]) -> bool {
        match self.open(VERIFIER_AAD, nonce, ct) {
            Ok(pt) => pt.as_slice() == VERIFIER_PLAINTEXT,
            Err(_) => false,
        }
    }
}

pub fn value_aad(snapshot_id: i64, key: &str) -> Vec<u8> {
    format!("{snapshot_id}:{key}").into_bytes()
}

pub fn comment_aad(snapshot_id: i64, key: &str) -> Vec<u8> {
    format!("{snapshot_id}:{key}:comment").into_bytes()
}

pub fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

#[cfg(test)]
pub const TEST_PARAMS: KdfParams = KdfParams {
    m_cost: 64,
    t_cost: 1,
    p_cost: 1,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(password: &str, salt: &[u8]) -> VaultKeys {
        derive_keys(password.as_bytes(), salt, TEST_PARAMS).unwrap()
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let k = keys("fake_password_1", &[1u8; SALT_LEN]);
        let aad = value_aad(7, "API_KEY");
        let sealed = k.seal(&aad, b"fake_value_1").unwrap();
        assert_ne!(sealed.ct, b"fake_value_1");
        let pt = k.open(&aad, &sealed.nonce, &sealed.ct).unwrap();
        assert_eq!(pt.as_slice(), b"fake_value_1");
    }

    #[test]
    fn nonces_are_fresh() {
        let k = keys("fake_password_1", &[1u8; SALT_LEN]);
        let a = k.seal(b"aad", b"fake_value_1").unwrap();
        let b = k.seal(b"aad", b"fake_value_1").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.ct, b.ct);
    }

    #[test]
    fn wrong_key_fails() {
        let k1 = keys("fake_password_1", &[1u8; SALT_LEN]);
        let k2 = keys("fake_password_2", &[1u8; SALT_LEN]);
        let sealed = k1.seal(b"aad", b"fake_value_1").unwrap();
        assert!(k2.open(b"aad", &sealed.nonce, &sealed.ct).is_err());
    }

    #[test]
    fn swapped_aad_fails() {
        let k = keys("fake_password_1", &[1u8; SALT_LEN]);
        let sealed = k.seal(&value_aad(1, "A_KEY"), b"fake_value_1").unwrap();
        // Same ciphertext moved to another key or another snapshot must not open.
        assert!(k.open(&value_aad(1, "B_KEY"), &sealed.nonce, &sealed.ct).is_err());
        assert!(k.open(&value_aad(2, "A_KEY"), &sealed.nonce, &sealed.ct).is_err());
        assert!(k.open(&comment_aad(1, "A_KEY"), &sealed.nonce, &sealed.ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let k = keys("fake_password_1", &[1u8; SALT_LEN]);
        let mut sealed = k.seal(b"aad", b"fake_value_1").unwrap();
        sealed.ct[0] ^= 1;
        assert!(k.open(b"aad", &sealed.nonce, &sealed.ct).is_err());
    }

    #[test]
    fn fingerprints_equal_for_equal_values_and_differ_across_vaults() {
        let v1 = keys("fake_password_1", &[1u8; SALT_LEN]);
        let v1_again = keys("fake_password_1", &[1u8; SALT_LEN]);
        let v2 = keys("fake_password_1", &[2u8; SALT_LEN]); // different salt = different vault
        let fp = v1.fingerprint(b"fake_value_1").unwrap();
        assert_eq!(fp, v1_again.fingerprint(b"fake_value_1").unwrap());
        assert_ne!(fp, v1.fingerprint(b"fake_value_2").unwrap());
        assert_ne!(fp, v2.fingerprint(b"fake_value_1").unwrap());
    }

    #[test]
    fn verifier_accepts_right_key_only() {
        let k1 = keys("fake_password_1", &[1u8; SALT_LEN]);
        let k2 = keys("fake_password_2", &[1u8; SALT_LEN]);
        let v = k1.make_verifier().unwrap();
        assert!(k1.check_verifier(&v.nonce, &v.ct));
        assert!(!k2.check_verifier(&v.nonce, &v.ct));
    }

    #[test]
    fn random_index_stays_in_range_and_covers_it() {
        let mut seen = [false; 7];
        for _ in 0..2000 {
            let i = random_index(7).unwrap() as usize;
            seen[i] = true;
        }
        assert!(seen.iter().all(|s| *s));
        assert!(random_index(0).is_err());
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(to_hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}
