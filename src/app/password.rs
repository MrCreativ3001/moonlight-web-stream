use moonlight_common::{crypto::rustcrypto::RustCryptoBackend, http::pair::PairingCryptoBackend};
use pbkdf2::sha2::Sha256;

use crate::app::AppError;

const HASH_ITERATIONS: u32 = 150_000;

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

        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, HASH_ITERATIONS, out);

        Ok(())
    }

    pub fn new(password: &str) -> Result<Self, AppError> {
        let mut salt = [0u8; 16];

        RustCryptoBackend.random_bytes(&mut salt)?;

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
