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

        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, out);

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
#[allow(clippy::unwrap_used)]
mod test {
    use crate::app::password::StoragePassword;

    #[test]
    fn test_iterations_600_000() {
        let password = "test";
        let storage = StoragePassword {
            salt: hex::decode("847040fce304a9bef9e7727f50f40d06")
                .unwrap()
                .try_into()
                .unwrap(),
            hash: hex::decode("a0d31b674cb2f97eaa3ac1366553db59e1a7b8c9d47cf210194a15fdd268c597")
                .unwrap()
                .try_into()
                .unwrap(),
            iterations: 600_000,
        };

        assert!(storage.verify(password).unwrap());
    }
}
