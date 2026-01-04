use std::fmt::{self, Display, Formatter};

use log::warn;
use moonlight_common::stream::bindings::{Colorspace, SupportedVideoFormats};
use serde::{Deserialize, Serialize};

pub mod api_bindings;
pub mod api_bindings_ext;
pub mod config;
pub mod ipc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSettings {
    pub bitrate: u32,
    pub packet_size: u32,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub play_audio_local: bool,
    pub video_supported_formats: SupportedVideoFormats,
    pub video_colorspace: Colorspace,
    pub video_color_range_full: bool,
    pub hdr: bool,
}

impl Display for StreamSettings {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} with {}x{}x{}",
            self.video_supported_formats, self.width, self.height, self.fps
        )
    }
}

pub fn serialize_json<T>(message: &T) -> Option<String>
where
    T: Serialize,
{
    let Ok(json) = serde_json::to_string(&message) else {
        warn!("[Stream]: failed to serialize to json");
        return None;
    };

    Some(json)
}
