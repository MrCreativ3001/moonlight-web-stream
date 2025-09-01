use log::warn;
use moonlight_common::moonlight::{stream::Colorspace, video::SupportedVideoFormats};
use serde::{Deserialize, Serialize};

pub mod api_bindings;
pub mod api_bindings_consts;
pub mod config;
pub mod ipc;

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamSettings {
    pub bitrate: u32,
    pub packet_size: u32,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub video_sample_queue_size: u32,
    pub audio_sample_queue_size: u32,
    pub play_audio_local: bool,
    pub video_supported_formats: SupportedVideoFormats,
    pub video_colorspace: Colorspace,
    pub video_color_range_full: bool,
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
