use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, AeadCore, OsRng, Payload, rand_core::RngCore},
};
use anyhow::{Context, Result, ensure};
use std::{io::Write, path::Path};

const MAGIC: &[u8; 8] = b"ROVENC01";

#[derive(Clone)]
pub struct StorageKey([u8; 32]);

impl StorageKey {
    pub fn load(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let path = root.join("master.key");
        if !path.exists() {
            // Missing keys must never silently replace keys for existing content.
            ensure!(
                !root.join("blobs").exists(),
                "master.key is missing; restore it from backup"
            );
            let mut key = [0; 32];
            OsRng.fill_bytes(&mut key);
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path)?;
            file.write_all(&key)?;
            file.sync_all()?;
        }
        let key = std::fs::read(&path)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid master.key"))?;
        Ok(Self(key))
    }

    pub fn encrypt(&self, bytes: &[u8], context: &[u8]) -> Result<Vec<u8>> {
        let mut data_key = [0; 32];
        OsRng.fill_bytes(&mut data_key);
        let key_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let data_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let wrapped = Aes256Gcm::new_from_slice(&self.0)
            .unwrap()
            .encrypt(
                &key_nonce,
                Payload {
                    msg: &data_key,
                    aad: context,
                },
            )
            .map_err(|_| anyhow::anyhow!("Key encryption failed"))?;
        let encrypted = Aes256Gcm::new_from_slice(&data_key)
            .unwrap()
            .encrypt(
                &data_nonce,
                Payload {
                    msg: bytes,
                    aad: context,
                },
            )
            .map_err(|_| anyhow::anyhow!("Content encryption failed"))?;
        let mut output = Vec::with_capacity(80 + encrypted.len());
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&key_nonce);
        output.extend_from_slice(&data_nonce);
        output.extend_from_slice(&wrapped);
        output.extend_from_slice(&encrypted);
        Ok(output)
    }

    pub fn decrypt(&self, bytes: &[u8], context: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            bytes.len() >= 96 && &bytes[..8] == MAGIC,
            "Invalid encrypted object"
        );
        let key = Aes256Gcm::new_from_slice(&self.0)
            .unwrap()
            .decrypt(
                Nonce::from_slice(&bytes[8..20]),
                Payload {
                    msg: &bytes[32..80],
                    aad: context,
                },
            )
            .map_err(|_| anyhow::anyhow!("Key authentication failed"))?;
        Aes256Gcm::new_from_slice(&key)
            .context("Invalid data key")?
            .decrypt(
                Nonce::from_slice(&bytes[20..32]),
                Payload {
                    msg: &bytes[80..],
                    aad: context,
                },
            )
            .map_err(|_| anyhow::anyhow!("Content authentication failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_content_rejects_tampering_and_other_object_contexts() {
        let root = tempfile::tempdir().unwrap();
        let key = StorageKey::load(root.path()).unwrap();
        let encrypted = key.encrypt(b"private design", b"owner/object/1").unwrap();
        let reopened = StorageKey::load(root.path()).unwrap();
        assert_eq!(
            reopened.decrypt(&encrypted, b"owner/object/1").unwrap(),
            b"private design"
        );
        assert!(reopened.decrypt(&encrypted, b"other/object/1").is_err());
        let mut changed = encrypted.clone();
        *changed.last_mut().unwrap() ^= 1;
        assert!(reopened.decrypt(&changed, b"owner/object/1").is_err());
        assert!(
            reopened
                .decrypt(&encrypted[..encrypted.len() - 1], b"owner/object/1")
                .is_err()
        );
        std::fs::create_dir(root.path().join("blobs")).unwrap();
        std::fs::remove_file(root.path().join("master.key")).unwrap();
        assert!(StorageKey::load(root.path()).is_err());
    }
}
