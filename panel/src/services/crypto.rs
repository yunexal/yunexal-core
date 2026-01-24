use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    XChaCha20Poly1305, XNonce
};
use std::env;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub struct EncryptionService {
    cipher: XChaCha20Poly1305,
}

impl EncryptionService {
    pub fn new() -> Self {
        let key_str = env::var("APP_KEY").expect("APP_KEY must be set");
        let key_bytes = if key_str.len() == 32 {
            key_str.into_bytes()
        } else {
             // Try base64 decode if not raw 32 chars
             match BASE64.decode(&key_str) {
                 Ok(b) if b.len() == 32 => b,
                 _ => panic!("APP_KEY must be 32 bytes (raw string or base64)"),
             }
        };

        let cipher = XChaCha20Poly1305::new_from_slice(&key_bytes).expect("Invalid key length");
        Self { cipher }
    }

    pub fn encrypt(&self, data: &[u8]) -> Result<String, String> {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng); // 24-bytes; unique
        let ciphertext = self.cipher.encrypt(&nonce, data)
            .map_err(|e| format!("Encryption failure: {}", e))?;
        
        // Return: nonce + ciphertext (base64)
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);
        
        Ok(BASE64.encode(combined))
    }

    pub fn decrypt(&self, encrypted_data: &str) -> Result<Vec<u8>, String> {
        let combined = BASE64.decode(encrypted_data)
            .map_err(|e| format!("Base64 decode failed: {}", e))?;
            
        if combined.len() < 24 {
            return Err("Data too short".to_string());
        }

        let (nonce, ciphertext) = combined.split_at(24);
        let nonce = XNonce::from_slice(nonce);

        self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| format!("Decryption failure: {}", e))
    }
}
