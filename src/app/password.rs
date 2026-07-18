use moonlight_common::{crypto::rustcrypto::RustCryptoBackend, http::pair::PairingCryptoBackend};
use pbkdf2::sha2::Sha256;

use crate::app::AppError;

pub const HASH_ITERATIONS: u32 = 600_000;

#[derive(Clone)]
pub struct StoragePassword {
    pub salt: [u8; 16],
    pub hash: [u8; 32],
    pub iterations: u32,
}

impl StoragePassword {
    fn hash(
        salt: &[u8; 16],
        iterations: u32,
        password: &str,
        out: &mut [u8; 32],
    ) -> Result<(), AppError> {
        if password.is_empty() {
            return Err(AppError::PasswordEmpty);
        }

        // TODO
        // pkcs5::pbkdf2_hmac(
        //     password.as_bytes(),
        //     salt,
        //     iterations as usize,
        //     MessageDigest::sha256(),
        //     out,
        // )?;
        todo!();

        Ok(())
    }

    pub fn new(password: &str) -> Result<Self, AppError> {
        let mut salt = [0u8; 16];

        RustCryptoBackend.random_bytes(&mut salt)?;

        let mut hash = [0u8; 32];

        Self::hash(&salt, HASH_ITERATIONS, password, &mut hash)?;

        Ok(Self {
            salt,
            hash,
            iterations: HASH_ITERATIONS,
        })
    }

    pub fn verify(&self, password: &str) -> Result<bool, AppError> {
        let mut hash = [0u8; 32];
        Self::hash(&self.salt, self.iterations, password, &mut hash)?;

        Ok(self.hash == hash)
    }

    pub fn needs_rehash(&self) -> bool {
        self.iterations < HASH_ITERATIONS
    }
}

#[cfg(test)]
mod test {
    use crate::app::password::StoragePassword;

    #[test]
    fn test_iterations_150_000() {
        let password = "test";
        // TODO
        // let storage = StoragePassword {
        // };
        // "password": {
        //   "salt": "c689708ce8e8ec2c0940d25a8c4ae2fb",
        //   "hash": "467d82bff427486dfe08b48024093558b69fcf66480c83282009f9b2d67e4be9"
        // },
    }

    #[test]
    fn test_iterations_600_000() {
        todo!()
    }
}
