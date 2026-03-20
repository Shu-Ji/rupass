use anyhow::{Context, Result, anyhow, bail};
use argon2::Argon2;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

pub(crate) fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut output = [0_u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut output)
        .map_err(|err| anyhow!("failed to derive key: {err}"))?;
    Ok(output)
}

pub(crate) fn password_verifier(key: &[u8; 32]) -> Vec<u8> {
    Sha256::digest(key).to_vec()
}

pub(crate) fn encrypt_text(cipher_key: &[u8; 32], plaintext: &str) -> Result<(String, String)> {
    let nonce = random_bytes::<24>();
    let cipher = XChaCha20Poly1305::new_from_slice(cipher_key)
        .map_err(|_| anyhow!("failed to initialize cipher"))?;
    let encrypted = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| anyhow!("failed to encrypt data"))?;

    Ok((STANDARD.encode(encrypted), STANDARD.encode(nonce)))
}

pub(crate) fn decrypt_text(cipher_key: &[u8; 32], ciphertext: &str, nonce: &str) -> Result<String> {
    let cipher = XChaCha20Poly1305::new_from_slice(cipher_key)
        .map_err(|_| anyhow!("failed to initialize cipher"))?;
    let nonce_bytes = STANDARD.decode(nonce).context("invalid nonce")?;
    if nonce_bytes.len() != 24 {
        bail!("invalid nonce length");
    }

    let payload = STANDARD
        .decode(ciphertext)
        .context("invalid encrypted payload")?;
    let decrypted = cipher
        .decrypt(XNonce::from_slice(&nonce_bytes), payload.as_ref())
        .map_err(|_| anyhow!("failed to decrypt data"))?;

    String::from_utf8(decrypted).context("decrypted data is not valid utf-8")
}

pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0_u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

pub(crate) fn read_existing_password(team: &str) -> Result<String> {
    let password = rpassword::prompt_password(format!("password for {team}: "))
        .context("failed to read password")?;
    if password.is_empty() {
        bail!("password cannot be empty");
    }

    Ok(password)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_round_trip() {
        let salt = random_bytes::<16>();
        let key = derive_key("secret", &salt).expect("derive key");
        let (encrypted, nonce) = encrypt_text(&key, "value").expect("encrypt");
        let decrypted = decrypt_text(&key, &encrypted, &nonce).expect("decrypt");
        assert_eq!(decrypted, "value");
    }
}
