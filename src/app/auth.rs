use std::fmt;

use hex::FromHexError;
use openssl::rand::rand_bytes;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

use crate::app::AppError;

pub enum UserAuth {
    None,
    UserPassword { username: String, password: String },
    Session(SessionToken),
    ForwardedHeaders { username: String },
}

const SESSION_TOKEN_SIZE: usize = 32;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionToken([u8; SESSION_TOKEN_SIZE]);

impl SessionToken {
    pub fn new() -> Result<Self, AppError> {
        let mut bytes = [0; SESSION_TOKEN_SIZE];

        rand_bytes(&mut bytes)?;

        Ok(Self(bytes))
    }

    pub fn encode<'a>(&self, bytes: &'a mut [u8; SESSION_TOKEN_SIZE * 2]) -> &'a str {
        hex::encode_to_slice(self.0.as_slice(), bytes).expect("failed to hex encode bytes");

        str::from_utf8(bytes).expect("hex encode produces invalid utf-8")
    }

    pub fn decode(str: &str) -> Result<Self, FromHexError> {
        let mut arr = [0u8; SESSION_TOKEN_SIZE];
        hex::decode_to_slice(str.as_bytes(), &mut arr)?;
        Ok(SessionToken(arr))
    }
}

impl Serialize for SessionToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = [0; _];
        serializer.serialize_str(self.encode(&mut bytes))
    }
}

impl<'de> Deserialize<'de> for SessionToken {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SessionTokenVisitor;

        impl<'de> Visitor<'de> for SessionTokenVisitor {
            type Value = SessionToken;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(&format!("a {}-byte hex string", SESSION_TOKEN_SIZE * 2))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                SessionToken::decode(v).map_err(|_| E::custom("failed to decode hex"))
            }
        }

        deserializer.deserialize_str(SessionTokenVisitor)
    }
}
