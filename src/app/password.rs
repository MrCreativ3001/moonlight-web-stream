use openssl::{hash::MessageDigest, pkcs5, rand::rand_bytes};

use crate::app::AppError;

const HASH_ITERATIONS: usize = 150_000;

#[derive(Clone)]
pub struct StoragePassword {
    pub salt: [u8; 16],
    pub hash: [u8; 32],
}

impl StoragePassword {
    fn hash(salt: &[u8; 16], password: &str, out: &mut [u8; 32]) -> Result<(), AppError> {
        if password.is_empty() {
            return Err(AppError::PasswordEmpty);
        }

        pkcs5::pbkdf2_hmac(
            password.as_bytes(),
            salt,
            HASH_ITERATIONS,
            MessageDigest::sha256(),
            out,
        )?;

        Ok(())
    }

    pub fn new(password: &str) -> Result<Self, AppError> {
        let mut salt = [0u8; 16];

        rand_bytes(&mut salt)?;

        let mut hash = [0u8; 32];

        Self::hash(&salt, password, &mut hash)?;

        Ok(Self { salt, hash })
    }

    pub fn verify(&self, password: &str) -> Result<bool, AppError> {
        let mut hash = [0u8; 32];
        Self::hash(&self.salt, password, &mut hash)?;

        Ok(self.hash == hash)
    }
}
