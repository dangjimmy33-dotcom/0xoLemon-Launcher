use std::env;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Transport encryption used for new depot pack chunks.
///
/// The launcher still accepts old chunks where `ChunkRef.encryption == None`.
pub const DEPOT_ENCRYPTION_ALGORITHM: &str = "XCHACHA20-POLY1305-ZSTD-V1";
pub const DEPOT_KEY_ENV: &str = "OXO_DEPOT_KEY";

// IMPORTANT: change this before public releases. Prefer setting OXO_DEPOT_KEY
// at build/run time instead of relying on this fallback.
const FALLBACK_KEY_MATERIAL: &str = "7FHeVBQLBRmfWOCO5PYkILtZtRmj98ZKkBCgjb71zUA=";

#[derive(Debug, Error)]
pub enum DepotCryptoError {
    #[error("unsupported depot encryption algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("invalid depot encryption nonce")]
    InvalidNonce,
    #[error("depot decrypt failed; wrong key or corrupted chunk")]
    DecryptFailed,
    #[error("depot encrypt failed")]
    EncryptFailed,
}

pub fn resolve_key_material(override_key: Option<&str>) -> String {
    override_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            env::var(DEPOT_KEY_ENV)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            option_env!("OXO_DEPOT_KEY")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| FALLBACK_KEY_MATERIAL.to_string())
}

pub fn key_id_from_material(material: &str) -> String {
    let key = derive_key(material);
    let digest = Sha256::digest(key);
    hex::encode(&digest[..8])
}

pub fn encrypt_compressed_chunk(
    compressed: &[u8],
    chunk_hash: &str,
    plaintext_compressed_sha256: &str,
    key_material: &str,
) -> Result<(Vec<u8>, String), DepotCryptoError> {
    let key = derive_key(key_material);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let mut nonce_bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let aad = chunk_aad(chunk_hash, plaintext_compressed_sha256);
    let encrypted = cipher
        .encrypt(
            nonce,
            Payload {
                msg: compressed,
                aad: &aad,
            },
        )
        .map_err(|_| DepotCryptoError::EncryptFailed)?;
    Ok((encrypted, B64.encode(nonce_bytes)))
}

pub fn decrypt_compressed_chunk(
    encrypted: &[u8],
    chunk_hash: &str,
    plaintext_compressed_sha256: &str,
    nonce_b64: &str,
    key_material: &str,
    algorithm: &str,
) -> Result<Vec<u8>, DepotCryptoError> {
    if algorithm != DEPOT_ENCRYPTION_ALGORITHM {
        return Err(DepotCryptoError::UnsupportedAlgorithm(
            algorithm.to_string(),
        ));
    }
    let nonce_vec = B64
        .decode(nonce_b64.as_bytes())
        .map_err(|_| DepotCryptoError::InvalidNonce)?;
    if nonce_vec.len() != 24 {
        return Err(DepotCryptoError::InvalidNonce);
    }
    let key = derive_key(key_material);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let nonce = XNonce::from_slice(&nonce_vec);
    let aad = chunk_aad(chunk_hash, plaintext_compressed_sha256);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: encrypted,
                aad: &aad,
            },
        )
        .map_err(|_| DepotCryptoError::DecryptFailed)
}

fn chunk_aad(chunk_hash: &str, plaintext_compressed_sha256: &str) -> Vec<u8> {
    format!("0xo-depot-v1\nchunk={chunk_hash}\ncompressedSha256={plaintext_compressed_sha256}")
        .into_bytes()
}

fn derive_key(material: &str) -> [u8; 32] {
    let trimmed = material.trim();
    if let Ok(bytes) = B64.decode(trimmed.as_bytes()) {
        if bytes.len() == 32 {
            let mut out = [0_u8; 32];
            out.copy_from_slice(&bytes);
            return out;
        }
    }
    if trimmed.len() == 64 {
        if let Ok(bytes) = hex::decode(trimmed) {
            if bytes.len() == 32 {
                let mut out = [0_u8; 32];
                out.copy_from_slice(&bytes);
                return out;
            }
        }
    }
    let digest = Sha256::digest(trimmed.as_bytes());
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}
