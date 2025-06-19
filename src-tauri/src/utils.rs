use std::collections::HashMap;
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use rand::Rng;

// Parse URL query parameters in the format key=value;key2=value2
pub fn parse_query_params(query: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    
    // Split by semicolon for parameters
    for param in query.split(';') {
        // Split by equals for key-value pairs
        if let Some(idx) = param.find('=') {
            let key = param[0..idx].trim().to_string();
            let value = param[idx+1..].trim().to_string();
            params.insert(key, value);
        }
    }
    
    params
}

// Encryption utilities
pub struct EncryptionManager {
    cipher: Aes256Gcm,
}

impl EncryptionManager {
    pub fn new(key_string: &str) -> Self {
        // Generate a key from the provided string
        let mut key_bytes = [0u8; 32];
        let bytes = key_string.as_bytes();
        
        // Use the key string or pad it if it's too short
        for i in 0..key_bytes.len() {
            key_bytes[i] = if i < bytes.len() {
                bytes[i]
            } else {
                // Pad with repeating pattern if key is too short
                bytes[i % bytes.len()]
            };
        }
        
        let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
        let cipher = Aes256Gcm::new(key);
        
        EncryptionManager { cipher }
    }
    
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        // Generate a random nonce
        let nonce_value = rand::thread_rng().gen::<[u8; 12]>();
        let nonce = Nonce::from_slice(&nonce_value);
        
        // Encrypt the data
        match self.cipher.encrypt(nonce, data) {
            Ok(mut encrypted) => {
                // Prepend the nonce to the encrypted data
                let mut result = nonce_value.to_vec();
                result.append(&mut encrypted);
                Ok(result)
            }
            Err(_) => Err("Encryption failed".to_string()),
        }
    }
    
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        if data.len() < 12 {
            return Err("Data too short to contain nonce".to_string());
        }
        
        // Extract the nonce from the data
        let nonce = Nonce::from_slice(&data[0..12]);
        
        // Decrypt the data
        match self.cipher.decrypt(nonce, &data[12..]) {
            Ok(decrypted) => Ok(decrypted),
            Err(_) => Err("Decryption failed".to_string()),
        }
    }
}

// Compression utilities
pub fn compress_data(data: &[u8], level: i32) -> Result<Vec<u8>, String> {
    zstd::encode_all(data, level).map_err(|e| format!("Compression failed: {}", e))
}

pub fn decompress_data(data: &[u8]) -> Result<Vec<u8>, String> {
    zstd::decode_all(data).map_err(|e| format!("Decompression failed: {}", e))
}
