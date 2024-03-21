use openssl::symm::{encrypt_aead, Cipher, decrypt_aead};

const CIPHER: fn() -> Cipher = Cipher::chacha20_poly1305;
const KEY_SIZE: usize = 32; // chacha20 uses a 32-byte key
const IV_SIZE: usize = 12; // chacha20 uses a 12-byte nonce
const TAG_SIZE: usize = 16; // chacha20-poly1305 uses a 16-byte tag

/// The result of an encryption operation.
pub struct EncryptionResult {
    pub ciphertext: Vec<u8>,
    pub iv: Vec<u8>,
    pub tag: Vec<u8>,
}

/// Encrypts a message using the chacha20-poly1305 AEAD cipher.
/// Returns the ciphertext, the IV, and the tag.
pub fn encrypt_message(message: &[u8], key: &[u8]) -> Result<EncryptionResult, ()> {
    assert_eq!(key.len(), KEY_SIZE);
    let mut iv: [u8; IV_SIZE] = [0; IV_SIZE];
    openssl::rand::rand_bytes(&mut iv).unwrap();
    let mut tag: [u8; TAG_SIZE] = [0; TAG_SIZE];
    match encrypt_aead(CIPHER(), key, Some(&iv), &[], message, &mut tag) {
        Ok(ciphertext) => Ok(EncryptionResult{ ciphertext, iv: iv.to_vec(), tag: tag.to_vec() }),
        Err(_) => Err(()),
    }
}

pub fn decrypt_message(ciphertext: &[u8], key: &[u8], iv: &[u8], tag: &[u8]) -> Result<Vec<u8>, ()> {
    assert_eq!(key.len(), KEY_SIZE);
    assert_eq!(iv.len(), IV_SIZE);
    assert_eq!(tag.len(), TAG_SIZE);
    match decrypt_aead(CIPHER(), key, Some(iv), &[], ciphertext, tag) {
        Ok(plaintext) => Ok(plaintext),
        Err(_) => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key: [u8; 32] = [0; 32];
        let message = b"Hello, world!";
        let mut result = encrypt_message(message, &key).unwrap();
        let plaintext = decrypt_message(&result.ciphertext, &key, &result.iv, &result.tag).unwrap();
        assert_eq!(&message.to_vec(), &plaintext);

        result.tag[0] ^= 0x01; // flip a bit in the tag
        assert!(decrypt_message(&result.ciphertext, &key, &result.iv, &result.tag).is_err());
    }
}
