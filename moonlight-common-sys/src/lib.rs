#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

pub mod limelight {
    include!(concat!(env!("OUT_DIR"), "/limelight.rs"));
}

#[cfg(feature = "crypto")]
mod crypto {
    include!(concat!(env!("OUT_DIR"), "/crypto.rs"));
}
