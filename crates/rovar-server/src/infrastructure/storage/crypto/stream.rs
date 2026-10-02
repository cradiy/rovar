//! Media uses RustCrypto's STREAM construction with an authenticated final
//! empty segment. The envelope binds the stream nonce and format to the object
//! context; STREAM authenticates segment order and the final marker.
use super::*;
use aes_gcm::aead::stream::{DecryptorBE32, EncryptorBE32};

const MAGIC: &[u8; 8] = b"ROVMED01";
pub const HEADER_BYTES: usize = 75;
pub const CHUNK_BYTES: usize = 1024 * 1024;
pub const TAG_BYTES: usize = 16;

pub struct Encryptor {
    stream: EncryptorBE32<Aes256Gcm>,
    aad: Vec<u8>,
}

pub struct Decryptor {
    stream: DecryptorBE32<Aes256Gcm>,
    aad: Vec<u8>,
}

impl StorageKey {
    pub fn media_encryptor(&self, context: &[u8]) -> Result<(Encryptor, Vec<u8>)> {
        let mut key = [0; 32];
        let mut nonce = [0; 7];
        OsRng.fill_bytes(&mut key);
        OsRng.fill_bytes(&mut nonce);
        let wrap_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut header = Vec::with_capacity(HEADER_BYTES);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&wrap_nonce);
        header.extend_from_slice(&nonce);
        let mut aad = context.to_vec();
        aad.extend_from_slice(&header);
        let wrapped = Aes256Gcm::new_from_slice(&self.0)
            .unwrap()
            .encrypt(
                &wrap_nonce,
                Payload {
                    msg: &key,
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow::anyhow!("Media key encryption failed"))?;
        header.extend_from_slice(&wrapped);
        Ok((
            Encryptor {
                stream: EncryptorBE32::new((&key).into(), (&nonce).into()),
                aad,
            },
            header,
        ))
    }

    pub fn media_decryptor(
        &self,
        header: &[u8; HEADER_BYTES],
        context: &[u8],
    ) -> Result<Decryptor> {
        ensure!(&header[..8] == MAGIC, "Invalid encrypted media format");
        let mut aad = context.to_vec();
        aad.extend_from_slice(&header[..27]);
        let key = Aes256Gcm::new_from_slice(&self.0)
            .unwrap()
            .decrypt(
                Nonce::from_slice(&header[8..20]),
                Payload {
                    msg: &header[27..],
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow::anyhow!("Media key authentication failed"))?;
        Ok(Decryptor {
            stream: DecryptorBE32::from_aead(
                Aes256Gcm::new_from_slice(&key).context("Invalid media key")?,
                (&header[20..27]).into(),
            ),
            aad,
        })
    }
}

impl Encryptor {
    pub fn next(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            !bytes.is_empty() && bytes.len() <= CHUNK_BYTES,
            "Invalid media segment length"
        );
        self.stream
            .encrypt_next(Payload {
                msg: bytes,
                aad: &self.aad,
            })
            .map_err(|_| anyhow::anyhow!("Media encryption failed"))
    }
    pub fn finish(self) -> Result<Vec<u8>> {
        self.stream
            .encrypt_last(Payload {
                msg: &[],
                aad: &self.aad,
            })
            .map_err(|_| anyhow::anyhow!("Media encryption failed"))
    }
}

impl Decryptor {
    pub fn next(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        ensure!(
            bytes.len() > TAG_BYTES && bytes.len() <= CHUNK_BYTES + TAG_BYTES,
            "Invalid media segment length"
        );
        self.stream
            .decrypt_next(Payload {
                msg: bytes,
                aad: &self.aad,
            })
            .map_err(|_| anyhow::anyhow!("Media authentication failed"))
    }
    pub fn finish(self, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() == TAG_BYTES, "Invalid media final segment");
        let output = self
            .stream
            .decrypt_last(Payload {
                msg: bytes,
                aad: &self.aad,
            })
            .map_err(|_| anyhow::anyhow!("Media final authentication failed"))?;
        ensure!(output.is_empty(), "Invalid media final segment");
        Ok(())
    }
}
