use std::fmt::{Display, Formatter};

use moonlight_common::{
    ServerState,
    stream::bindings::{
        Colorspace, ControllerButtons, ControllerCapabilities, KeyModifiers, MouseButton,
        SupportedVideoFormats,
    },
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{api_bindings_ext::TsAny, ts_consts};

const EXPORT_PATH: &str = "../../web-server/web/api_bindings.ts";

#[derive(Serialize, Deserialize, Debug, TS, Clone)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct ConfigJs {
    pub path_prefix: String,
    pub default_settings: Option<TsAny>,
}

#[derive(Serialize, Deserialize, Debug, TS, Clone)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostLoginRequest {
    pub name: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug, TS, Clone, Copy)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum HostState {
    Free,
    Busy,
}

impl From<ServerState> for HostState {
    fn from(value: ServerState) -> Self {
        match value {
            ServerState::Free => Self::Free,
            ServerState::Busy => Self::Busy,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum PairStatus {
    NotPaired,
    Paired,
}

impl From<moonlight_common::PairStatus> for PairStatus {
    fn from(value: moonlight_common::PairStatus) -> Self {
        use moonlight_common::PairStatus as MlPairStatus;
        match value {
            MlPairStatus::NotPaired => Self::NotPaired,
            MlPairStatus::Paired => Self::Paired,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum HostOwner {
    ThisUser,
    Global,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct UndetailedHost {
    pub host_id: u32,
    pub owner: HostOwner,
    pub name: String,
    pub paired: PairStatus,
    /// None if offline else the state
    pub server_state: Option<HostState>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct DetailedHost {
    pub host_id: u32,
    pub owner: HostOwner,
    pub name: String,
    pub paired: PairStatus,
    pub server_state: Option<HostState>,
    pub address: String,
    pub http_port: u16,
    pub https_port: u16,
    pub external_port: u16,
    pub version: String,
    pub gfe_version: String,
    pub unique_id: String,
    pub mac: Option<String>,
    pub local_ip: String,
    pub current_game: u32,
    pub max_luma_pixels_hevc: u32,
    pub server_codec_mode_support: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct App {
    pub app_id: u32,
    pub title: String,
    pub is_hdr_supported: bool,
}

impl From<moonlight_common::network::App> for App {
    fn from(value: moonlight_common::network::App) -> Self {
        Self {
            app_id: value.id,
            title: value.title,
            is_hdr_supported: value.is_hdr_supported,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetHostsResponse {
    pub hosts: Vec<UndetailedHost>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetHostQuery {
    pub host_id: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetHostResponse {
    pub host: DetailedHost,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostHostRequest {
    pub address: String,
    pub http_port: Option<u16>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostHostResponse {
    pub host: DetailedHost,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PatchHostRequest {
    /// The host id of the host to change
    pub host_id: u32,
    /// Option<Option<u32>> are not supported
    pub change_owner: bool,
    pub owner: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct DeleteHostQuery {
    pub host_id: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostPairRequest {
    pub host_id: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum PostPairResponse1 {
    InternalServerError,
    PairError,
    Pin(String),
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum PostPairResponse2 {
    PairError,
    Paired(DetailedHost),
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostWakeUpRequest {
    pub host_id: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetAppsQuery {
    pub host_id: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetAppsResponse {
    pub apps: Vec<App>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetAppImageQuery {
    pub host_id: u32,
    pub app_id: u32,
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostCancelRequest {
    pub host_id: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostCancelResponse {
    pub success: bool,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum UserRole {
    User,
    Admin,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetUserQuery {
    pub name: Option<String>,
    pub user_id: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct DetailedUser {
    pub id: u32,
    pub is_default_user: bool,
    pub name: String,
    pub role: UserRole,
    pub client_unique_id: String,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PostUserRequest {
    pub name: String,
    pub password: String,
    pub role: UserRole,
    pub client_unique_id: String,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct PatchUserRequest {
    /// The user id of the user to change
    pub id: u32,
    pub password: Option<String>,
    pub role: Option<UserRole>,
    pub client_unique_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct DeleteUserRequest {
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct GetUsersResponse {
    pub users: Vec<DetailedUser>,
}

// -- Stream

#[derive(Serialize, Deserialize, Debug, TS, Clone, Copy, PartialEq, Eq)]
#[ts(export, export_to = EXPORT_PATH)]
#[serde(rename_all = "lowercase")]
pub enum TransportChannelMethod {
    WebRTC,
    WebSocket,
}

ts_consts!(
    pub TransportChannelId(export_bindings_transport_channel_id: EXPORT_PATH) as u8:

    pub const GENERAL: u8 = 0;
    pub const STATS: u8 = 1;
    pub const HOST_VIDEO: u8 = 2;
    pub const HOST_AUDIO: u8 = 3;
    pub const MOUSE_RELIABLE: u8 = 4;
    pub const MOUSE_ABSOLUTE: u8 = 5;
    pub const MOUSE_RELATIVE: u8 = 6;
    pub const KEYBOARD: u8 = 7;
    pub const TOUCH: u8 = 8;
    pub const CONTROLLERS: u8 = 9;
    pub const CONTROLLER0: u8 = 10;
    pub const CONTROLLER1: u8 = 11;
    pub const CONTROLLER2: u8 = 12;
    pub const CONTROLLER3: u8 = 13;
    pub const CONTROLLER4: u8 = 14;
    pub const CONTROLLER5: u8 = 15;
    pub const CONTROLLER6: u8 = 16;
    pub const CONTROLLER7: u8 = 17;
    pub const CONTROLLER8: u8 = 18;
    pub const CONTROLLER9: u8 = 19;
    pub const CONTROLLER10: u8 = 20;
    pub const CONTROLLER11: u8 = 21;
    pub const CONTROLLER12: u8 = 22;
    pub const CONTROLLER13: u8 = 23;
    pub const CONTROLLER14: u8 = 24;
    pub const CONTROLLER15: u8 = 25;
);

#[derive(Serialize, Deserialize, Debug, TS, Clone, Copy, PartialEq, Eq)]
#[ts(export, export_to = EXPORT_PATH)]
#[serde(rename_all = "lowercase")]
pub enum RtcSdpType {
    Offer,
    Answer,
    Pranswer,
    Rollback,
    Unspecified,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct RtcSessionDescription {
    pub ty: RtcSdpType,
    pub sdp: String,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct RtcIceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum StreamSignalingMessage {
    Description(RtcSessionDescription),
    AddIceCandidate(RtcIceCandidate),
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum TransportType {
    WebRTC,
    WebSocket,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum StreamClientMessage {
    Init {
        host_id: u32,
        app_id: u32,
        video_frame_queue_size: usize,
        audio_sample_queue_size: usize,
    },
    WebRtc(StreamSignalingMessage),
    SetTransport(TransportType),
    StartStream {
        bitrate: u32,
        packet_size: u32,
        fps: u32,
        width: u32,
        height: u32,
        play_audio_local: bool,
        video_supported_formats: u32,
        video_colorspace: StreamColorspace,
        video_color_range_full: bool,
        hdr: bool,
    },
}

#[derive(Serialize, Deserialize, Debug, TS, Clone, Default)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct RtcIceServer {
    #[serde(skip)]
    pub is_default: bool,
    pub urls: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub credential: String,
}

impl Display for RtcIceServer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "urls=[{}], username=\"{}\", credential=\"{}\"",
            self.urls.join(", "),
            self.username,
            self.credential,
        )
    }
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct StreamCapabilities {
    pub touch: bool,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum LogMessageType {
    Fatal,
    FatalDescription,
    Recover,
    InformError,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum StreamServerMessage {
    Setup {
        ice_servers: Vec<RtcIceServer>,
    },
    WebRtc(StreamSignalingMessage),
    // Optional Info
    UpdateApp {
        app: App,
    },
    DebugLog {
        message: String,
        ty: Option<LogMessageType>,
    },
    ConnectionComplete {
        capabilities: StreamCapabilities,
        /// Use VideoSupportedCodec to figure this out
        format: u32,
        width: u32,
        height: u32,
        fps: u32,
        audio_sample_rate: u32,
        audio_channel_count: u32,
        audio_streams: u32,
        audio_coupled_streams: u32,
        audio_samples_per_frame: u32,
        audio_mapping: [u8; 8],
    },
    ConnectionTerminated {
        error_code: i32,
    },
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum GeneralServerMessage {
    ConnectionStatusUpdate { status: ConnectionStatus },
    HdrModeUpdate { enabled: bool },
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum GeneralClientMessage {}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum ConnectionStatus {
    Ok,
    Poor,
}

impl From<moonlight_common::stream::bindings::ConnectionStatus> for ConnectionStatus {
    fn from(value: moonlight_common::stream::bindings::ConnectionStatus) -> Self {
        use moonlight_common::stream::bindings::ConnectionStatus;
        match value {
            ConnectionStatus::Ok => Self::Ok,
            ConnectionStatus::Poor => Self::Poor,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub struct StatsHostProcessingLatency {
    pub min_host_processing_latency_ms: f64,
    pub max_host_processing_latency_ms: f64,
    pub avg_host_processing_latency_ms: f64,
}

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum StreamerStatsUpdate {
    Rtt {
        rtt_ms: f64,
        rtt_variance_ms: f64,
    },
    Video {
        host_processing_latency: Option<StatsHostProcessingLatency>,
        min_streamer_processing_time_ms: f64,
        max_streamer_processing_time_ms: f64,
        avg_streamer_processing_time_ms: f64,
    },
}

// Virtual-Key Codes
// https://github.com/awakecoding/Win32Keyboard/blob/master/vkcodes.h
ts_consts!(
    pub StreamKeys(export_bindings_keys: EXPORT_PATH) as u16:

    /* Mouse buttons */

    // Left mouse button
    pub const VK_LBUTTON: u16 = 0x01;
    // Right mouse button
    pub const VK_RBUTTON: u16 = 0x02;
    // Control-break processing
    pub const VK_CANCEL: u16 = 0x03;
    // Middle mouse button (three-button mouse)
    pub const VK_MBUTTON: u16 = 0x04;
    // Windows 2000/XP: X1 mouse button
    pub const VK_XBUTTON1: u16 = 0x05;
    // Windows 2000/XP: X2 mouse button
    pub const VK_XBUTTON2: u16 = 0x06;

    /* 0x07 is undefined */

    // BACKSPACE key
    pub const VK_BACK: u16 = 0x08;
    // TAB key
    pub const VK_TAB: u16 = 0x09;

    /* 0x0A to 0x0B are reserved */

    // CLEAR key
    pub const VK_CLEAR: u16 = 0x0C;
    // ENTER key
    pub const VK_RETURN: u16 = 0x0D;

    /* 0x0E to 0x0F are undefined */

    // SHIFT key
    pub const VK_SHIFT: u16 = 0x10;
    // CTRL key
    pub const VK_CONTROL: u16 = 0x11;
    // ALT key
    pub const VK_MENU: u16 = 0x12;
    // PAUSE key
    pub const VK_PAUSE: u16 = 0x13;
    // CAPS LOCK key
    pub const VK_CAPITAL: u16 = 0x14;
    // Input Method Editor (IME) Kana mode
    pub const VK_KANA: u16 = 0x15;
    // IME Hanguel mode (maintained for compatibility; use #define VK_HANGUL)
    pub const VK_HANGUEL: u16 = 0x15;
    // IME Hangul mode
    pub const VK_HANGUL: u16 = 0x15;

    /* 0x16 is undefined */

    // IME Junja mode
    pub const VK_JUNJA: u16 = 0x17;
    // IME final mode
    pub const VK_FINAL: u16 = 0x18;
    // IME Hanja mode
    pub const VK_HANJA: u16 = 0x19;
    // IME Kanji mode
    pub const VK_KANJI: u16 = 0x19;

    /* 0x1A is undefined */

    // ESC key
    pub const VK_ESCAPE: u16 = 0x1B;
    // IME convert
    pub const VK_CONVERT: u16 = 0x1C;
    // IME nonconvert
    pub const VK_NONCONVERT: u16 = 0x1D;
    // IME accept
    pub const VK_ACCEPT: u16 = 0x1E;
    // IME mode change request
    pub const VK_MODECHANGE: u16 = 0x1F;

    // SPACEBAR
    pub const VK_SPACE: u16 = 0x20;
    // PAGE UP key
    pub const VK_PRIOR: u16 = 0x21;
    // PAGE DOWN key
    pub const VK_NEXT: u16 = 0x22;
    // END key
    pub const VK_END: u16 = 0x23;
    // HOME key
    pub const VK_HOME: u16 = 0x24;
    // LEFT ARROW key
    pub const VK_LEFT: u16 = 0x25;
    // UP ARROW key
    pub const VK_UP: u16 = 0x26;
    // RIGHT ARROW key
    pub const VK_RIGHT: u16 = 0x27;
    // DOWN ARROW key
    pub const VK_DOWN: u16 = 0x28;
    // SELECT key
    pub const VK_SELECT: u16 = 0x29;
    // PRINT key
    pub const VK_PRINT: u16 = 0x2A;
    // EXECUTE key
    pub const VK_EXECUTE: u16 = 0x2B;
    // PRINT SCREEN key
    pub const VK_SNAPSHOT: u16 = 0x2C;
    // INS key
    pub const VK_INSERT: u16 = 0x2D;
    // DEL key
    pub const VK_DELETE: u16 = 0x2E;
    // HELP key
    pub const VK_HELP: u16 = 0x2F;

    /* Digits, the last 4 bits of the code represent the corresponding digit */

    // '0' key
    pub const VK_KEY_0: u16 = 0x30;
    // '1' key
    pub const VK_KEY_1: u16 = 0x31;
    // '2' key
    pub const VK_KEY_2: u16 = 0x32;
    // '3' key
    pub const VK_KEY_3: u16 = 0x33;
    // '4' key
    pub const VK_KEY_4: u16 = 0x34;
    // '5' key
    pub const VK_KEY_5: u16 = 0x35;
    // '6' key
    pub const VK_KEY_6: u16 = 0x36;
    // '7' key
    pub const VK_KEY_7: u16 = 0x37;
    // '8' key
    pub const VK_KEY_8: u16 = 0x38;
    // '9' key
    pub const VK_KEY_9: u16 = 0x39;

    /* 0x3A to 0x40 are undefined */

    /* The alphabet, the code corresponds to the capitalized letter in the ASCII code */

    // 'A' key
    pub const VK_KEY_A: u16 = 0x41;
    // 'B' key
    pub const VK_KEY_B: u16 = 0x42;
    // 'C' key
    pub const VK_KEY_C: u16 = 0x43;
    // 'D' key
    pub const VK_KEY_D: u16 = 0x44;
    // 'E' key
    pub const VK_KEY_E: u16 = 0x45;
    // 'F' key
    pub const VK_KEY_F: u16 = 0x46;
    // 'G' key
    pub const VK_KEY_G: u16 = 0x47;
    // 'H' key
    pub const VK_KEY_H: u16 = 0x48;
    // 'I' key
    pub const VK_KEY_I: u16 = 0x49;
    // 'J' key
    pub const VK_KEY_J: u16 = 0x4A;
    // 'K' key
    pub const VK_KEY_K: u16 = 0x4B;
    // 'L' key
    pub const VK_KEY_L: u16 = 0x4C;
    // 'M' key
    pub const VK_KEY_M: u16 = 0x4D;
    // 'N' key
    pub const VK_KEY_N: u16 = 0x4E;
    // 'O' key
    pub const VK_KEY_O: u16 = 0x4F;
    // 'P' key
    pub const VK_KEY_P: u16 = 0x50;
    // 'Q' key
    pub const VK_KEY_Q: u16 = 0x51;
    // 'R' key
    pub const VK_KEY_R: u16 = 0x52;
    // 'S' key
    pub const VK_KEY_S: u16 = 0x53;
    // 'T' key
    pub const VK_KEY_T: u16 = 0x54;
    // 'U' key
    pub const VK_KEY_U: u16 = 0x55;
    // 'V' key
    pub const VK_KEY_V: u16 = 0x56;
    // 'W' key
    pub const VK_KEY_W: u16 = 0x57;
    // 'X' key
    pub const VK_KEY_X: u16 = 0x58;
    // 'Y' key
    pub const VK_KEY_Y: u16 = 0x59;
    // 'Z' key
    pub const VK_KEY_Z: u16 = 0x5A;

    // Left Windows key (Microsoft Natural keyboard)
    pub const VK_LWIN: u16 = 0x5B;
    // Right Windows key (Natural keyboard)
    pub const VK_RWIN: u16 = 0x5C;
    // Applications key (Natural keyboard)
    pub const VK_APPS: u16 = 0x5D;

    /* 0x5E is reserved */

    // Computer Sleep key
    pub const VK_SLEEP: u16 = 0x5F;

    /* Numeric keypad digits, the last four bits of the code represent the corresponding digit */

    // Numeric keypad '0' key
    pub const VK_NUMPAD0: u16 = 0x60;
    // Numeric keypad '1' key
    pub const VK_NUMPAD1: u16 = 0x61;
    // Numeric keypad '2' key
    pub const VK_NUMPAD2: u16 = 0x62;
    // Numeric keypad '3' key
    pub const VK_NUMPAD3: u16 = 0x63;
    // Numeric keypad '4' key
    pub const VK_NUMPAD4: u16 = 0x64;
    // Numeric keypad '5' key
    pub const VK_NUMPAD5: u16 = 0x65;
    // Numeric keypad '6' key
    pub const VK_NUMPAD6: u16 = 0x66;
    // Numeric keypad '7' key
    pub const VK_NUMPAD7: u16 = 0x67;
    // Numeric keypad '8' key
    pub const VK_NUMPAD8: u16 = 0x68;
    // Numeric keypad '9' key
    pub const VK_NUMPAD9: u16 = 0x69;

    /* Numeric keypad operators and special keys */

    // Multiply key
    pub const VK_MULTIPLY: u16 = 0x6A;
    // Add key
    pub const VK_ADD: u16 = 0x6B;
    // Separator key
    pub const VK_SEPARATOR: u16 = 0x6C;
    // Subtract key
    pub const VK_SUBTRACT: u16 = 0x6D;
    // Decimal key
    pub const VK_DECIMAL: u16 = 0x6E;
    // Divide key
    pub const VK_DIVIDE: u16 = 0x6F;

    /* Function keys, from F1 to F24 */

    // F1 key
    pub const VK_F1: u16 = 0x70;
    // F2 key
    pub const VK_F2: u16 = 0x71;
    // F3 key
    pub const VK_F3: u16 = 0x72;
    // F4 key
    pub const VK_F4: u16 = 0x73;
    // F5 key
    pub const VK_F5: u16 = 0x74;
    // F6 key
    pub const VK_F6: u16 = 0x75;
    // F7 key
    pub const VK_F7: u16 = 0x76;
    // F8 key
    pub const VK_F8: u16 = 0x77;
    // F9 key
    pub const VK_F9: u16 = 0x78;
    // F10 key
    pub const VK_F10: u16 = 0x79;
    // F11 key
    pub const VK_F11: u16 = 0x7A;
    // F12 key
    pub const VK_F12: u16 = 0x7B;
    // F13 key
    pub const VK_F13: u16 = 0x7C;
    // F14 key
    pub const VK_F14: u16 = 0x7D;
    // F15 key
    pub const VK_F15: u16 = 0x7E;
    // F16 key
    pub const VK_F16: u16 = 0x7F;
    // F17 key
    pub const VK_F17: u16 = 0x80;
    // F18 key
    pub const VK_F18: u16 = 0x81;
    // F19 key
    pub const VK_F19: u16 = 0x82;
    // F20 key
    pub const VK_F20: u16 = 0x83;
    // F21 key
    pub const VK_F21: u16 = 0x84;
    // F22 key
    pub const VK_F22: u16 = 0x85;
    // F23 key
    pub const VK_F23: u16 = 0x86;
    // F24 key
    pub const VK_F24: u16 = 0x87;

    /* 0x88 to 0x8F are unassigned */

    // NUM LOCK key
    pub const VK_NUMLOCK: u16 = 0x90;
    // SCROLL LOCK key
    pub const VK_SCROLL: u16 = 0x91;

    /* 0x92 to 0x96 are OEM specific */
    /* 0x97 to 0x9F are unassigned */

    /* Modifier keys */

    // Left SHIFT key
    pub const VK_LSHIFT: u16 = 0xA0;
    // Right SHIFT key
    pub const VK_RSHIFT: u16 = 0xA1;
    // Left CONTROL key
    pub const VK_LCONTROL: u16 = 0xA2;
    // Right CONTROL key
    pub const VK_RCONTROL: u16 = 0xA3;
    // Left MENU key
    pub const VK_LMENU: u16 = 0xA4;
    // Right MENU key
    pub const VK_RMENU: u16 = 0xA5;

    /* Browser related keys */

    // Windows 2000/XP: Browser Back key
    pub const VK_BROWSER_BACK: u16 = 0xA6;
    // Windows 2000/XP: Browser Forward key
    pub const VK_BROWSER_FORWARD: u16 = 0xA7;
    // Windows 2000/XP: Browser Refresh key
    pub const VK_BROWSER_REFRESH: u16 = 0xA8;
    // Windows 2000/XP: Browser Stop key
    pub const VK_BROWSER_STOP: u16 = 0xA9;
    // Windows 2000/XP: Browser Search key
    pub const VK_BROWSER_SEARCH: u16 = 0xAA;
    // Windows 2000/XP: Browser Favorites key
    pub const VK_BROWSER_FAVORITES: u16 = 0xAB;
    // Windows 2000/XP: Browser Start and Home key
    pub const VK_BROWSER_HOME: u16 = 0xAC;

    /* Volume related keys */

    // Windows 2000/XP: Volume Mute key
    pub const VK_VOLUME_MUTE: u16 = 0xAD;
    // Windows 2000/XP: Volume Down key
    pub const VK_VOLUME_DOWN: u16 = 0xAE;
    // Windows 2000/XP: Volume Up key
    pub const VK_VOLUME_UP: u16 = 0xAF;

    /* Media player related keys */

    // Windows 2000/XP: Next Track key
    pub const VK_MEDIA_NEXT_TRACK: u16 = 0xB0;
    // Windows 2000/XP: Previous Track key
    pub const VK_MEDIA_PREV_TRACK: u16 = 0xB1;
    // Windows 2000/XP: Stop Media key
    pub const VK_MEDIA_STOP: u16 = 0xB2;
    // Windows 2000/XP: Play/Pause Media key
    pub const VK_MEDIA_PLAY_PAUSE: u16 = 0xB3;

    /* Application launcher keys */

    // Windows 2000/XP: Start Mail key
    pub const VK_LAUNCH_MAIL: u16 = 0xB4;
    // Windows 2000/XP: Select Media key
    pub const VK_MEDIA_SELECT: u16 = 0xB5;
    // Windows 2000/XP: Start Application 1 key
    pub const VK_LAUNCH_APP1: u16 = 0xB6;
    // Windows 2000/XP: Start Application 2 key
    pub const VK_LAUNCH_APP2: u16 = 0xB7;

    /* 0xB8 and 0xB9 are reserved */

    /* OEM keys */

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_1: u16 = 0xBA;
    /* Windows 2000/XP: For the US standard keyboard, the ';:' key */

    // Windows 2000/XP: For any country/region, the '+' key
    pub const VK_OEM_PLUS: u16 = 0xBB;
    // Windows 2000/XP: For any country/region, the ',' key
    pub const VK_OEM_COMMA: u16 = 0xBC;
    // Windows 2000/XP: For any country/region, the '-' key
    pub const VK_OEM_MINUS: u16 = 0xBD;
    // Windows 2000/XP: For any country/region, the '.' key
    pub const VK_OEM_PERIOD: u16 = 0xBE;

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_2: u16 = 0xBF;
    /* Windows 2000/XP: For the US standard keyboard, the '/?' key */

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_3: u16 = 0xC0;
    /* Windows 2000/XP: For the US standard keyboard, the '`~' key */

    /* 0xC1 to 0xD7 are reserved */
    // Brazilian (ABNT) Keyboard
    pub const VK_ABNT_C1: u16 = 0xC1;
    // Brazilian (ABNT) Keyboard
    pub const VK_ABNT_C2: u16 = 0xC2;

    /* 0xD8 to 0xDA are unassigned */

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_4: u16 = 0xDB;
    /* Windows 2000/XP: For the US standard keyboard, the '[{' key */

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_5: u16 = 0xDC;
    /* Windows 2000/XP: For the US standard keyboard, the '\|' key */

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_6: u16 = 0xDD;
    /* Windows 2000/XP: For the US standard keyboard, the ']}' key */

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_7: u16 = 0xDE;
    /* Windows 2000/XP: For the US standard keyboard, the 'single-quote/double-quote' key */

    // Used for miscellaneous characters; it can vary by keyboard.
    pub const VK_OEM_8: u16 = 0xDF;

    /* 0xE0 is reserved */
    /* 0xE1 is OEM specific */

    // Windows 2000/XP: Either the angle bracket key or
    pub const VK_OEM_102: u16 = 0xE2;
    /* the backslash key on the RT 102-key keyboard */

    /* 0xE3 and 0xE4 are OEM specific */

    // Windows 95/98/Me, Windows NT 4.0, Windows 2000/XP: IME PROCESS key
    pub const VK_PROCESSKEY: u16 = 0xE5;

    /* 0xE6 is OEM specific */

    // Windows 2000/XP: Used to pass Unicode characters as if they were keystrokes.
    pub const VK_PACKET: u16 = 0xE7;
    /* The #define VK_PACKET key is the low word of a 32-bit Virtual Key value used */
    /* for non-keyboard input methods. For more information, */
    /* see Remark in KEYBDINPUT, SendInput, WM_KEYDOWN, and WM_KEYUP */

    /* 0xE8 is unassigned */
    /* 0xE9 to 0xF5 are OEM specific */

    // Attn key
    pub const VK_ATTN: u16 = 0xF6;
    // CrSel key
    pub const VK_CRSEL: u16 = 0xF7;
    // ExSel key
    pub const VK_EXSEL: u16 = 0xF8;
    // Erase EOF key
    pub const VK_EREOF: u16 = 0xF9;
    // Play key
    pub const VK_PLAY: u16 = 0xFA;
    // Zoom key
    pub const VK_ZOOM: u16 = 0xFB;
    // Reserved
    pub const VK_NONAME: u16 = 0xFC;
    // PA1 key
    pub const VK_PA1: u16 = 0xFD;
    // Clear key
    pub const VK_OEM_CLEAR: u16 = 0xFE;
);

// Key Modifiers
ts_consts!(
    pub StreamKeyModifiers(export_bindings_key_modifiers: EXPORT_PATH):

    pub const MASK_SHIFT: i8 = KeyModifiers::SHIFT.bits();
    pub const MASK_CTRL: i8 = KeyModifiers::CTRL.bits();
    pub const MASK_ALT: i8 = KeyModifiers::ALT.bits();
    pub const MASK_META: i8 = KeyModifiers::META.bits();
);

// Mouse Buttons
ts_consts!(
    pub StreamMouseButton(export_bindings_mouse_buttons: EXPORT_PATH):

    pub const LEFT: i32 = MouseButton::Left as i32;
    pub const MIDDLE: i32 = MouseButton::Middle as i32;
    pub const RIGHT: i32 = MouseButton::Right as i32;
    pub const X1: i32 = MouseButton::X1 as i32;
    pub const X2: i32 = MouseButton::X2 as i32;
);

// Controller Buttons
ts_consts!(
    pub StreamControllerButton(export_bindings_controller_buttons: EXPORT_PATH):

    pub const BUTTON_A: u32       = ControllerButtons::A.bits();
    pub const BUTTON_B: u32       = ControllerButtons::B.bits();
    pub const BUTTON_X: u32       = ControllerButtons::X.bits();
    pub const BUTTON_Y: u32       = ControllerButtons::Y.bits();
    pub const BUTTON_UP: u32      = ControllerButtons::UP.bits();
    pub const BUTTON_DOWN: u32    = ControllerButtons::DOWN.bits();
    pub const BUTTON_LEFT: u32    = ControllerButtons::LEFT.bits();
    pub const BUTTON_RIGHT: u32   = ControllerButtons::RIGHT.bits();
    pub const BUTTON_LB: u32      = ControllerButtons::LB.bits();
    pub const BUTTON_RB: u32      = ControllerButtons::RB.bits();
    pub const BUTTON_PLAY: u32    = ControllerButtons::PLAY.bits();
    pub const BUTTON_BACK: u32    = ControllerButtons::BACK.bits();
    pub const BUTTON_LS_CLK: u32  = ControllerButtons::LS_CLK.bits();
    pub const BUTTON_RS_CLK: u32  = ControllerButtons::RS_CLK.bits();
    pub const BUTTON_SPECIAL: u32 = ControllerButtons::SPECIAL.bits();
    pub const BUTTON_PADDLE1: u32 = ControllerButtons::PADDLE1.bits();
    pub const BUTTON_PADDLE2: u32 = ControllerButtons::PADDLE2.bits();
    pub const BUTTON_PADDLE3: u32 = ControllerButtons::PADDLE3.bits();
    pub const BUTTON_PADDLE4: u32 = ControllerButtons::PADDLE4.bits();
    pub const BUTTON_TOUCHPAD: u32 =ControllerButtons::TOUCHPAD.bits();
    pub const BUTTON_MISC: u32     =ControllerButtons::MISC.bits();
);

// Controller Buttons
ts_consts!(
    pub StreamControllerCapabilities(export_bindings_controller_capabilities: EXPORT_PATH):

    pub const CAPABILITY_RUMBLE: u16 = ControllerCapabilities::RUMBLE.bits();
    pub const CAPABILITY_TRIGGER_RUMBLE: u16 = ControllerCapabilities::TRIGGER_RUMBLE.bits();
);

#[derive(Serialize, Deserialize, Debug, TS)]
#[ts(export, export_to = EXPORT_PATH)]
pub enum StreamColorspace {
    Rec601,
    Rec709,
    Rec2020,
}

impl From<StreamColorspace> for Colorspace {
    fn from(value: StreamColorspace) -> Self {
        match value {
            StreamColorspace::Rec601 => Colorspace::Rec601,
            StreamColorspace::Rec709 => Colorspace::Rec709,
            StreamColorspace::Rec2020 => Colorspace::Rec2020,
        }
    }
}

// Video Supported Codec
ts_consts!(
    pub StreamSupportedVideoCodecs(export_bindings_supported_video_codecs: EXPORT_PATH):

    pub const H264: u32 = SupportedVideoFormats::H264.bits();
    pub const H264_HIGH8_444: u32 = SupportedVideoFormats::H264_HIGH8_444.bits();
    pub const H265: u32 = SupportedVideoFormats::H265.bits();
    pub const H265_MAIN10: u32 = SupportedVideoFormats::H265_MAIN10.bits();
    pub const H265_REXT8_444: u32 = SupportedVideoFormats::H265_REXT8_444.bits();
    pub const H265_REXT10_444: u32 = SupportedVideoFormats::H265_REXT10_444.bits();
    pub const AV1_MAIN8: u32 = SupportedVideoFormats::AV1_MAIN8.bits();
    pub const AV1_MAIN10: u32 = SupportedVideoFormats::AV1_MAIN10.bits();
    pub const AV1_HIGH8_444: u32 = SupportedVideoFormats::AV1_HIGH8_444.bits();
    pub const AV1_HIGH10_444: u32 = SupportedVideoFormats::AV1_HIGH10_444.bits();
);
