use argon2::Argon2;
use curve25519_dalek::{Scalar, RistrettoPoint};
pub use nazgul::blsag::BLSAG_COMPACT;
use rand_core::{OsRng, RngCore};

use chacha20poly1305::{
    aead::{Aead, KeyInit}, AeadCore, ChaCha20Poly1305, Key, Nonce
};

const CIPHER: fn(&Key) -> ChaCha20Poly1305 = ChaCha20Poly1305::new;
pub const KEY_SIZE: usize = 32; // chacha20 uses a 32-byte key
pub const SALT_SIZE: usize = 32; // argon2 uses a 32-byte salt
const IV_SIZE: usize = 12; // chacha20 uses a 12-byte nonce
const TAG_SIZE: usize = 16; // chacha20-poly1305 uses a 16-byte tag

/// The result of an encryption operation.
#[derive(Debug, PartialEq)]
pub struct EncryptionResult {
    pub ciphertext: Vec<u8>,
    pub iv: [u8; IV_SIZE],
}

impl EncryptionResult {
    pub fn encode(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend_from_slice(&self.iv);
        result.extend_from_slice(&self.ciphertext);
        result
    }

    pub fn decode(data: &[u8]) -> Result<EncryptionResult, ()> {
        if data.len() < IV_SIZE + TAG_SIZE {
            return Err(());
        }
        let mut iv = [0u8; IV_SIZE];
        iv.clone_from_slice(&data[0..IV_SIZE]);
        let ciphertext = data[IV_SIZE..].to_vec();
        Ok(EncryptionResult{ ciphertext, iv})
    }
}

/// Generate iv
pub fn generate_iv() -> [u8; IV_SIZE] {
    ChaCha20Poly1305::generate_nonce(&mut OsRng).to_vec().try_into().unwrap()
}

/// Encrypts a message using the chacha20-poly1305 AEAD cipher.
/// Returns the ciphertext, the IV, and the tag.
pub fn encrypt_message(message: &[u8], key: &[u8]) -> Result<EncryptionResult, ()> {
    assert_eq!(key.len(), KEY_SIZE);
    let iv = generate_iv();
    match CIPHER(Key::from_slice(key)).encrypt(Nonce::from_slice(&iv), message) {
        Ok(ciphertext) => {
            Ok(EncryptionResult{ciphertext, iv})
        },
        Err(_) => {
            Err(())
        },
    }
}

pub fn decrypt_message(key: &[u8; KEY_SIZE], encrypted_data: &EncryptionResult) -> Result<Vec<u8>, ()> {
    match CIPHER(Key::from_slice(key)).decrypt(Nonce::from_slice(&encrypted_data.iv), encrypted_data.ciphertext.as_ref()) {
        Ok(plaintext) => Ok(plaintext),
        Err(_) => Err(()),
    }
}

pub fn generate_ephemeral_key() -> [u8; KEY_SIZE] {
    let mut out = [0u8; KEY_SIZE];
    OsRng.fill_bytes(&mut out);
    out
}

pub fn apply_ephemeral_key_part(key: &mut [u8; KEY_SIZE], part: &[u8]) {
    assert_eq!(part.len(), KEY_SIZE);
    // xor the key with the part
    key.iter_mut().zip(part.iter()).for_each(|(a, b)| *a ^= *b);
}

/// Signs a message using the BLSAG signature scheme
pub fn sign_message(private_key: &Scalar, personal_key_insertion_index: usize, ring: &[RistrettoPoint], message: &[u8]) -> BLSAG_COMPACT {
    BLSAG_COMPACT::sign::<sha3::Keccak512, OsRng>(private_key, ring, personal_key_insertion_index, message)
}

/// Verifies a BLSAG signature
pub fn verify_message(signature: &BLSAG_COMPACT, ring: &[RistrettoPoint], message: &[u8]) -> bool {
    BLSAG_COMPACT::verify::<sha3::Keccak512>(signature, ring, message)
}

/// Generate salt
pub fn generate_salt() -> [u8; SALT_SIZE] {
    let mut out = [0u8; SALT_SIZE];
    OsRng.fill_bytes(&mut out);
    out
}

/// Hashes a password using Argon2, returns the hash and the salt
pub fn hash_password(password: &[u8]) -> ([u8; 32], [u8; SALT_SIZE]) {
    let salt = generate_salt();
    let argon = Argon2::default();
    let mut out = [0u8; 32];
    argon.hash_password_into(password, &salt, &mut out).unwrap();
    (out, salt)
}

/// Hashes a password using Argon2 with a given salt
pub fn hash_password_with_salt(password: &[u8], salt: &[u8; SALT_SIZE]) -> [u8; 32] {
    let argon = Argon2::default();
    let mut out = [0u8; 32];
    argon.hash_password_into(password, salt, &mut out).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let key = generate_ephemeral_key();
        let message = b"Hello, world!";
        let mut result = encrypt_message(message, &key).unwrap();
        let plaintext = decrypt_message(&key, &result).unwrap();
        assert_eq!(&message.to_vec(), &plaintext);

        result.ciphertext[0] ^= 0x01; // flip a bit in the tag
        assert!(decrypt_message(&key, &result).is_err());
    }

    #[test]
    fn test_sign_verify_message() {
        let message = "hi".as_bytes().to_vec();
        let mut ring: Vec<RistrettoPoint> = (0..5)
            .map(|_| RistrettoPoint::random(&mut OsRng))
            .collect();
        let key: Scalar = Scalar::random(&mut OsRng);
        let pubkey = key * RISTRETTO_BASEPOINT_POINT;
        ring.push(pubkey);
        let signature = sign_message(&key, ring.len()-1, &ring, &message);
        assert!(verify_message(&signature, &ring, &message));
    }

    #[test]
    fn test_hash_password() {
        let password = "password".as_bytes();
        let (hash, salt) = hash_password(password);
        assert_eq!(hash, hash_password_with_salt(password, &salt));
        assert_ne!(hash, hash_password_with_salt(b"password1", &salt));
    }

    #[test]
    fn test_encode_decode_encryption_result() {
        let iv = generate_iv();
        let ciphertext = b"this is a test ciphertext";
        let expected_encryption_result = EncryptionResult {ciphertext: ciphertext.to_vec(), iv};
        let encoded = expected_encryption_result.encode();
        let decoded = EncryptionResult::decode(&encoded).unwrap();
        assert_eq!(expected_encryption_result, decoded);
    }
}
