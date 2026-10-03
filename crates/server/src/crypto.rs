//! Envelope encryption for provider tokens and secret URLs (AES-256-GCM).
//!
//! The key never lives in the database. It comes from `TENDLY_ENCRYPTION_KEY`
//! (base64, 32 bytes), from `TENDLY_ENCRYPTION_KEY_FILE`, from
//! `TENDLY_ENCRYPTION_KEY_PATH`, or is generated once into
//! `<data_dir>/secrets/encryption.key` with owner-only permissions.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use std::path::Path;

#[derive(Clone)]
pub struct Cipher {
    cipher: Aes256Gcm,
    pub source: String,
}

impl std::fmt::Debug for Cipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Cipher({})", self.source)
    }
}

impl Cipher {
    pub fn from_key_bytes(key: &[u8], source: &str) -> Result<Self> {
        if key.len() != 32 {
            bail!("encryption key must be 32 bytes");
        }
        Ok(Cipher { cipher: Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("bad key"))?, source: source.into() })
    }

    pub fn from_base64(b64: &str, source: &str) -> Result<Self> {
        let key = B64.decode(b64.trim()).context("encryption key is not valid base64")?;
        Self::from_key_bytes(&key, source)
    }

    /// Loads or creates the key according to the documented precedence.
    pub fn load(env_key: Option<&str>, key_path: Option<&Path>, data_dir: &Path) -> Result<Self> {
        if let Some(k) = env_key {
            return Self::from_base64(k, "environment");
        }
        let path = key_path.map(Path::to_path_buf).unwrap_or_else(|| data_dir.join("secrets").join("encryption.key"));
        if path.exists() {
            let k = std::fs::read_to_string(&path).context("reading encryption key file")?;
            return Self::from_base64(&k, "key file");
        }
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_private(&path, B64.encode(key).as_bytes())?;
        tracing::info!("generated a new encryption key file (keep it out of backups shared with others)");
        Self::from_key_bytes(&key, "generated key file")
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ct = self.cipher.encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes()).map_err(|_| anyhow!("encryption failed"))?;
        let mut out = nonce.to_vec();
        out.extend(ct);
        Ok(format!("v1:{}", B64.encode(out)))
    }

    pub fn decrypt(&self, stored: &str) -> Result<String> {
        let raw = stored.strip_prefix("v1:").ok_or_else(|| anyhow!("unknown ciphertext version"))?;
        let bytes = B64.decode(raw).context("ciphertext is not base64")?;
        if bytes.len() < 13 {
            bail!("ciphertext too short");
        }
        let (nonce, ct) = bytes.split_at(12);
        let pt =
            self.cipher.decrypt(Nonce::from_slice(nonce), ct).map_err(|_| anyhow!("decryption failed (wrong key or corrupted data)"))?;
        Ok(String::from_utf8(pt)?)
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        f.write_all(bytes)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

/// URL-safe random token with `bytes` bytes of entropy.
pub fn random_token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Constant-time comparison for secrets.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_tamper() {
        let dir = tempfile::tempdir().unwrap();
        let c = Cipher::load(None, None, dir.path()).unwrap();
        let ct = c.encrypt("refresh-token-value").unwrap();
        assert!(!ct.contains("refresh-token-value"));
        assert_eq!(c.decrypt(&ct).unwrap(), "refresh-token-value");
        // Reloading uses the same key file.
        let c2 = Cipher::load(None, None, dir.path()).unwrap();
        assert_eq!(c2.decrypt(&ct).unwrap(), "refresh-token-value");
        let mut bad = ct.clone();
        bad.pop();
        bad.push('A');
        assert!(c.decrypt(&bad).is_err());
        let other = Cipher::from_key_bytes(&[7u8; 32], "test").unwrap();
        assert!(other.decrypt(&ct).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(dir.path().join("secrets/encryption.key")).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn tokens_are_long_and_unique() {
        let a = random_token(32);
        let b = random_token(32);
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
    }
}
