use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("invalid base64 payload")]
    InvalidBase64(#[from] base64::DecodeError),
    #[error("invalid Ed25519 public key length")]
    InvalidPublicKeyLength,
    #[error("invalid Ed25519 signature length")]
    InvalidSignatureLength,
    #[error("invalid Ed25519 public key")]
    InvalidPublicKey,
}

pub fn verify_ed25519(
    public_key_b64: &str,
    payload: &[u8],
    signature_b64: &str,
) -> Result<bool, SecurityError> {
    let public_key = STANDARD.decode(public_key_b64)?;
    let signature = STANDARD.decode(signature_b64)?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| SecurityError::InvalidPublicKeyLength)?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| SecurityError::InvalidSignatureLength)?;
    let key = VerifyingKey::from_bytes(&public_key).map_err(|_| SecurityError::InvalidPublicKey)?;
    let signature = Signature::from_bytes(&signature);
    Ok(key.verify(payload, &signature).is_ok())
}

#[cfg(test)]
mod tests {
    use super::{verify_ed25519, SecurityError};

    #[test]
    fn verifier_rejects_bad_base64() {
        assert!(matches!(
            verify_ed25519("not base64", b"payload", "also bad"),
            Err(SecurityError::InvalidBase64(_))
        ));
    }

    #[test]
    fn verifier_rejects_wrong_key_length() {
        assert!(matches!(
            verify_ed25519("AQID", b"payload", "AQID"),
            Err(SecurityError::InvalidPublicKeyLength)
        ));
    }
}
