use std::{
    ffi::CStr,
    fmt::{self, Debug, Display, Formatter},
    time::Duration,
};

use bitflags::bitflags;
use moonlight_common_sys::limelight::{
    A_FLAG, AUDIO_CONFIGURATION_MAX_CHANNEL_COUNT, B_FLAG, BACK_FLAG, BUFFER_TYPE_PICDATA,
    BUFFER_TYPE_PPS, BUFFER_TYPE_SPS, BUFFER_TYPE_VPS, BUTTON_ACTION_PRESS, BUTTON_ACTION_RELEASE,
    BUTTON_LEFT, BUTTON_MIDDLE, BUTTON_RIGHT, BUTTON_X1, BUTTON_X2, CAPABILITY_DIRECT_SUBMIT,
    CAPABILITY_PULL_RENDERER, CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1,
    CAPABILITY_REFERENCE_FRAME_INVALIDATION_AVC, CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC,
    CAPABILITY_SLOW_OPUS_DECODER, CAPABILITY_SUPPORTS_ARBITRARY_AUDIO_DURATION, COLOR_RANGE_FULL,
    COLOR_RANGE_LIMITED, COLORSPACE_REC_601, COLORSPACE_REC_709, COLORSPACE_REC_2020,
    CONN_STATUS_OKAY, CONN_STATUS_POOR, DOWN_FLAG, DR_NEED_IDR, DR_OK, DS_EFFECT_LEFT_TRIGGER,
    DS_EFFECT_PAYLOAD_SIZE, DS_EFFECT_RIGHT_TRIGGER, ENCFLG_ALL, ENCFLG_AUDIO, ENCFLG_NONE,
    ENCFLG_VIDEO, FRAME_TYPE_IDR, FRAME_TYPE_PFRAME, KEY_ACTION_DOWN, KEY_ACTION_UP, LB_FLAG,
    LEFT_FLAG, LI_BATTERY_STATE_CHARGING, LI_BATTERY_STATE_DISCHARGING, LI_BATTERY_STATE_FULL,
    LI_BATTERY_STATE_NOT_CHARGING, LI_BATTERY_STATE_NOT_PRESENT, LI_BATTERY_STATE_UNKNOWN,
    LI_CCAP_ACCEL, LI_CCAP_ANALOG_TRIGGERS, LI_CCAP_BATTERY_STATE, LI_CCAP_GYRO, LI_CCAP_RGB_LED,
    LI_CCAP_RUMBLE, LI_CCAP_TOUCHPAD, LI_CCAP_TRIGGER_RUMBLE, LI_CTYPE_NINTENDO, LI_CTYPE_PS,
    LI_CTYPE_UNKNOWN, LI_CTYPE_XBOX, LI_FF_CONTROLLER_TOUCH_EVENTS, LI_FF_PEN_TOUCH_EVENTS,
    LI_MOTION_TYPE_ACCEL, LI_MOTION_TYPE_GYRO, LI_TOUCH_EVENT_BUTTON_ONLY, LI_TOUCH_EVENT_CANCEL,
    LI_TOUCH_EVENT_CANCEL_ALL, LI_TOUCH_EVENT_DOWN, LI_TOUCH_EVENT_HOVER,
    LI_TOUCH_EVENT_HOVER_LEAVE, LI_TOUCH_EVENT_MOVE, LI_TOUCH_EVENT_UP, LS_CLK_FLAG,
    LiGetStageName, MISC_FLAG, ML_ERROR_FRAME_CONVERSION, ML_ERROR_GRACEFUL_TERMINATION,
    ML_ERROR_NO_VIDEO_FRAME, ML_ERROR_NO_VIDEO_TRAFFIC, ML_ERROR_PROTECTED_CONTENT,
    ML_ERROR_UNEXPECTED_EARLY_TERMINATION, MODIFIER_ALT, MODIFIER_CTRL, MODIFIER_META,
    MODIFIER_SHIFT, PADDLE1_FLAG, PADDLE2_FLAG, PADDLE3_FLAG, PADDLE4_FLAG, PLAY_FLAG, RB_FLAG,
    RIGHT_FLAG, RS_CLK_FLAG, SCM_AV1_HIGH8_444, SCM_AV1_HIGH10_444, SCM_AV1_MAIN8, SCM_AV1_MAIN10,
    SCM_H264, SCM_H264_HIGH8_444, SCM_HEVC, SCM_HEVC_MAIN10, SCM_HEVC_REXT8_444,
    SCM_HEVC_REXT10_444, SPECIAL_FLAG, SS_KBE_FLAG_NON_NORMALIZED, STAGE_AUDIO_STREAM_INIT,
    STAGE_AUDIO_STREAM_START, STAGE_CONTROL_STREAM_INIT, STAGE_CONTROL_STREAM_START,
    STAGE_INPUT_STREAM_INIT, STAGE_INPUT_STREAM_START, STAGE_MAX, STAGE_NAME_RESOLUTION,
    STAGE_NONE, STAGE_PLATFORM_INIT, STAGE_RTSP_HANDSHAKE, STAGE_VIDEO_STREAM_INIT,
    STAGE_VIDEO_STREAM_START, STREAM_CFG_AUTO, STREAM_CFG_LOCAL, STREAM_CFG_REMOTE, TOUCHPAD_FLAG,
    UP_FLAG, VIDEO_FORMAT_AV1_HIGH8_444, VIDEO_FORMAT_AV1_HIGH10_444, VIDEO_FORMAT_AV1_MAIN8,
    VIDEO_FORMAT_AV1_MAIN10, VIDEO_FORMAT_H264, VIDEO_FORMAT_H264_HIGH8_444, VIDEO_FORMAT_H265,
    VIDEO_FORMAT_H265_MAIN10, VIDEO_FORMAT_H265_REXT8_444, VIDEO_FORMAT_H265_REXT10_444,
    VIDEO_FORMAT_MASK_10BIT, VIDEO_FORMAT_MASK_AV1, VIDEO_FORMAT_MASK_H264, VIDEO_FORMAT_MASK_H265,
    VIDEO_FORMAT_MASK_YUV444, X_FLAG, Y_FLAG,
};
use num_derive::FromPrimitive;
use thiserror::Error;

// --------------- Stream ---------------
bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct EncryptionFlags: u32 {
        const AUDIO = ENCFLG_AUDIO;
        const VIDEO  = ENCFLG_VIDEO;

        const NONE = ENCFLG_NONE;
        const ALL = ENCFLG_ALL;
    }
}

pub struct StreamConfiguration {
    /// Dimensions in pixels of the desired video stream
    pub width: i32,
    /// Dimensions in pixels of the desired video stream
    pub height: i32,
    /// FPS of the desired video stream
    pub fps: i32,
    /// Bitrate of the desired video stream (audio adds another ~1 Mbps). This
    /// includes error correction data, so the actual encoder bitrate will be
    /// about 20% lower when using the standard 20% FEC configuration.
    pub bitrate: i32,
    /// Max video packet size in bytes (use 1024 if unsure). If STREAM_CFG_AUTO
    /// determines the stream is remote (see below), it will cap this value at
    /// 1024 to avoid MTU-related issues like packet loss and fragmentation.
    pub packet_size: i32,
    /// Determines whether to enable remote (over the Internet)
    /// streaming optimizations. If unsure, set to STREAM_CFG_AUTO.
    /// STREAM_CFG_AUTO uses a heuristic (whether the target address is
    /// in the RFC 1918 address blocks) to decide whether the stream
    /// is remote or not.
    pub streaming_remotely: StreamingConfig,
    /// Specifies the channel configuration of the audio stream.
    /// See AUDIO_CONFIGURATION constants and MAKE_AUDIO_CONFIGURATION() below.
    pub audio_configuration: i32,
    /// Specifies the mask of supported video formats.
    /// See VIDEO_FORMAT constants below.
    pub supported_video_formats: SupportedVideoFormats,
    /// If specified, the client's display refresh rate x 100. For example,
    /// 59.94 Hz would be specified as 5994. This is used by recent versions
    /// of GFE for enhanced frame pacing.
    pub client_refresh_rate_x100: i32,
    /// If specified, sets the encoder colorspace to the provided COLORSPACE_*
    /// option (listed above). If not set, the encoder will default to Rec 601.
    pub color_space: Colorspace,
    /// If specified, sets the encoder color range to the provided COLOR_RANGE_*
    /// option (listed above). If not set, the encoder will default to Limited.
    pub color_range: ColorRange,
    /// Specifies the data streams where encryption may be enabled if supported
    /// by the host PC. Ideally, you would pass ENCFLG_ALL to encrypt everything
    /// that we support encrypting. However, lower performance hardware may not
    /// be able to support encrypting heavy stuff like video or audio data, so
    /// that encryption may be disabled here. Remote input encryption is always
    /// enabled.
    pub encryption_flags: EncryptionFlags,
    /// AES encryption data for the remote input stream. This must be
    /// the same as what was passed as rikey and rikeyid
    /// in /launch and /resume requests.
    pub remote_input_aes_key: [u8; 16],
    /// AES encryption data for the remote input stream. This must be
    /// the same as what was passed as rikey and rikeyid
    /// in /launch and /resume requests.
    pub remote_input_aes_iv: u32,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Default)]
    pub struct Capabilities: u32 {
        const DIRECT_SUBMIT = CAPABILITY_DIRECT_SUBMIT;
        const REFERENCE_FRAME_INVALIDATION_AV1 = CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1;
        const REFERENCE_FRAME_INVALIDATION_HEVC = CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC;
        const REFERENCE_FRAME_INVALIDATION_AVC = CAPABILITY_REFERENCE_FRAME_INVALIDATION_AVC;
        const SUPPORTS_ARBITRARY_SOUND_DURATION = CAPABILITY_SUPPORTS_ARBITRARY_AUDIO_DURATION;
        const PULL_RENDERER = CAPABILITY_PULL_RENDERER;
        const SLOW_OPUS_DECODER = CAPABILITY_SLOW_OPUS_DECODER;
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum Stage {
    None = STAGE_NONE,
    PlatformInit = STAGE_PLATFORM_INIT,
    NameResolution = STAGE_NAME_RESOLUTION,
    AudioStreamInit = STAGE_AUDIO_STREAM_INIT,
    RtspHandshake = STAGE_RTSP_HANDSHAKE,
    ControlStreamInit = STAGE_CONTROL_STREAM_INIT,
    VideoStreamInit = STAGE_VIDEO_STREAM_INIT,
    InputStreamInit = STAGE_INPUT_STREAM_INIT,
    ControlStreamStart = STAGE_CONTROL_STREAM_START,
    VideoStreamStart = STAGE_VIDEO_STREAM_START,
    AudioStreamStart = STAGE_AUDIO_STREAM_START,
    InputStreamStart = STAGE_INPUT_STREAM_START,
    Max = STAGE_MAX,
}

impl Stage {
    pub fn name(&self) -> &str {
        unsafe {
            let raw_c_str = LiGetStageName(*self as i32);
            let c_str = CStr::from_ptr(raw_c_str);
            c_str.to_str().expect("convert stage name into utf8")
        }
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum ConnectionStatus {
    Ok = CONN_STATUS_OKAY,
    Poor = CONN_STATUS_POOR,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct DualSenseEffect: u32 {
        const PAYLOAD_SIZE = DS_EFFECT_PAYLOAD_SIZE;
        const RIGHT_TRIGGER = DS_EFFECT_RIGHT_TRIGGER;
        const LEFT_TRIGGER = DS_EFFECT_LEFT_TRIGGER;
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum TerminationError {
    Graceful = ML_ERROR_GRACEFUL_TERMINATION as i32,
    NoVideoTraffic = ML_ERROR_NO_VIDEO_TRAFFIC,
    NoVideoFrame = ML_ERROR_NO_VIDEO_FRAME,
    UnexpectedEarlyTermination = ML_ERROR_UNEXPECTED_EARLY_TERMINATION,
    ProtectedContent = ML_ERROR_PROTECTED_CONTENT,
    FrameConversion = ML_ERROR_FRAME_CONVERSION,
}

// --------------- Video ---------------

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ServerCodeModeSupport: u32 {
        const H264            = SCM_H264;
        const HEVC            = SCM_HEVC;
        const HEVC_MAIN10     = SCM_HEVC_MAIN10;
        const AV1_MAIN8       = SCM_AV1_MAIN8;
        const AV1_MAIN10      = SCM_AV1_MAIN10;
        const H264_HIGH8_444  = SCM_H264_HIGH8_444;
        const HEVC_REXT8_444  = SCM_HEVC_REXT8_444;
        const HEVC_REXT10_444 = SCM_HEVC_REXT10_444;
        const AV1_HIGH8_444   = SCM_AV1_HIGH8_444;
        const AV1_HIGH10_444  = SCM_AV1_HIGH10_444;
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum StreamingConfig {
    Local = STREAM_CFG_LOCAL,
    Remote = STREAM_CFG_REMOTE,
    Auto = STREAM_CFG_AUTO,
}

#[repr(u32)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum Colorspace {
    Rec601 = COLORSPACE_REC_601,
    Rec709 = COLORSPACE_REC_709,
    Rec2020 = COLORSPACE_REC_2020,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum ColorRange {
    Limited = COLOR_RANGE_LIMITED,
    Full = COLOR_RANGE_FULL,
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, Default)]
pub struct SupportedVideoFormats(u32);

bitflags! {
    impl SupportedVideoFormats: u32 {
        const H264 = VIDEO_FORMAT_H264;          // H.264 High Profile
        const H264_HIGH8_444 = VIDEO_FORMAT_H264_HIGH8_444;   // H.264 High 4:4:4 8-bit Profile
        const H265 = VIDEO_FORMAT_H265;                       // HEVC Main Profile
        const H265_MAIN10 = VIDEO_FORMAT_H265_MAIN10;         // HEVC Main10 Profile
        const H265_REXT8_444 = VIDEO_FORMAT_H265_REXT8_444;   // HEVC RExt 4:4:4 8-bit Profile
        const H265_REXT10_444 = VIDEO_FORMAT_H265_REXT10_444; // HEVC RExt 4:4:4 10-bit Profile
        const AV1_MAIN8 = VIDEO_FORMAT_AV1_MAIN8;             // AV1 Main 8-bit profile
        const AV1_MAIN10 = VIDEO_FORMAT_AV1_MAIN10;           // AV1 Main 10-bit profile
        const AV1_HIGH8_444 = VIDEO_FORMAT_AV1_HIGH8_444;     // AV1 High 4:4:4 8-bit profile
        const AV1_HIGH10_444 = VIDEO_FORMAT_AV1_HIGH10_444;   // AV1 High 4:4:4 10-bit profile

        // Preconfigured
        const MASK_H264 = VIDEO_FORMAT_MASK_H264;
        const MASK_H265 = VIDEO_FORMAT_MASK_H265;
        const MASK_AV1 = VIDEO_FORMAT_MASK_AV1;
        const MASK_10BIT = VIDEO_FORMAT_MASK_10BIT;
        const MASK_YUV444 = VIDEO_FORMAT_MASK_YUV444;
    }
}

impl Display for SupportedVideoFormats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;

        let mut first = true;
        for (name, _) in self.iter_names() {
            if !first {
                write!(f, ",")?;
            }
            write!(f, "{}", name)?;

            first = false;
        }
        write!(f, "]")?;
        Ok(())
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum VideoFormat {
    H264 = VIDEO_FORMAT_H264,                      // H.264 High Profile
    H264High8_444 = VIDEO_FORMAT_H264_HIGH8_444,   // H.264 High 4:4:4 8-bit Profile
    H265 = VIDEO_FORMAT_H265,                      // HEVC Main Profile
    H265Main10 = VIDEO_FORMAT_H265_MAIN10,         // HEVC Main10 Profile
    H265Rext8_444 = VIDEO_FORMAT_H265_REXT8_444,   // HEVC RExt 4:4:4 8-bit Profile
    H265Rext10_444 = VIDEO_FORMAT_H265_REXT10_444, // HEVC RExt 4:4:4 10-bit Profile
    Av1Main8 = VIDEO_FORMAT_AV1_MAIN8,             // AV1 Main 8-bit profile
    Av1Main10 = VIDEO_FORMAT_AV1_MAIN10,           // AV1 Main 10-bit profile
    Av1High8_444 = VIDEO_FORMAT_AV1_HIGH8_444,     // AV1 High 4:4:4 8-bit profile
    Av1High10_444 = VIDEO_FORMAT_AV1_HIGH10_444,   // AV1 High 4:4:4 10-bit profile
}

impl VideoFormat {
    pub fn all() -> [Self; 10] {
        [
            VideoFormat::H264,
            VideoFormat::H264High8_444,
            VideoFormat::H265,
            VideoFormat::H265Main10,
            VideoFormat::H265Rext8_444,
            VideoFormat::H265Rext10_444,
            VideoFormat::Av1Main8,
            VideoFormat::Av1Main10,
            VideoFormat::Av1High8_444,
            VideoFormat::Av1High10_444,
        ]
    }

    pub fn contained_in(&self, supported_video_formats: SupportedVideoFormats) -> bool {
        let Some(single_format) = SupportedVideoFormats::from_bits(*self as u32) else {
            return false;
        };

        supported_video_formats.contains(single_format)
    }
}

/// These identify codec configuration data in the buffer lists
/// of frames identified as IDR frames for H.264 and HEVC formats.
/// For other codecs, all data is marked as BUFFER_TYPE_PICDATA.
#[repr(u32)]
#[derive(Debug, Clone, Copy, FromPrimitive, PartialEq, Eq)]
pub enum BufferType {
    PicData = BUFFER_TYPE_PICDATA,
    Sps = BUFFER_TYPE_SPS,
    Pps = BUFFER_TYPE_PPS,
    Vps = BUFFER_TYPE_VPS,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum FrameType {
    /// This is a standard frame which references the IDR frame and
    /// previous P-frames.
    PFrame = FRAME_TYPE_PFRAME,
    /// This is a key frame.
    ///
    /// For H.264 and HEVC, this means the frame contains SPS, PPS, and VPS (HEVC only) NALUs
    /// as the first buffers in the list. The I-frame data follows immediately
    /// after the codec configuration NALUs.
    ///
    /// For other codecs, any configuration data is not split into separate buffers.
    Idr = FRAME_TYPE_IDR,
}

/// A decode unit describes a buffer chain of video data from multiple packets
pub struct VideoDecodeUnit<'a> {
    /// Frame Number
    pub frame_number: i32,
    /// Frame Type
    pub frame_type: FrameType,
    /// Optional host processing latency of the frame, in 1/10 ms units.
    /// Zero when the host doesn't provide the latency data
    /// or frame processing latency is not applicable to the current frame
    /// (happens when the frame is repeated).
    pub frame_processing_latency: Option<Duration>,
    /// Receive time of first buffer. This value uses an implementation-defined epoch,
    /// but the same epoch as enqueueTimeMs and LiGetMillis().
    pub receive_time: Duration,
    /// Time the frame was fully assembled and queued for the video decoder to process.
    /// This is also approximately the same time as the final packet was received, so
    /// enqueueTimeMs - receiveTimeMs is the time taken to receive the frame. At the
    /// time the decode unit is passed to submitDecodeUnit(), the total queue delay
    /// can be calculated by LiGetMillis() - enqueueTimeMs.
    pub enqueue_time: Duration,
    /// Presentation time in milliseconds with the epoch at the first captured frame.
    /// This can be used to aid frame pacing or to drop old frames that were queued too
    /// long prior to display.
    pub presentation_time: Duration,
    /// Original RTP timestamp in 90kHz units. Useful when using APIs that deal with integer
    /// time such as Apple's CMTime. To exactly recover the RTP timestamp, use something like
    /// CMTimeMake((int64_t)du->rtpTimestamp, 90000);
    pub rtp_timestamp: u32,
    /// Determines if this frame is SDR or HDR
    ///
    /// Note: This is not currently parsed from the actual bitstream, so if your
    /// client has access to a bitstream parser, prefer that over this field.
    pub hdr_active: bool,
    /// Provides the colorspace of this frame (see COLORSPACE_* defines above)
    ///
    /// Note: This is not currently parsed from the actual bitstream, so if your
    /// client has access to a bitstream parser, prefer that over this field.
    pub color_space: Colorspace,
    pub buffers: &'a [VideoDataBuffer<'a>],
}
pub struct VideoDataBuffer<'a> {
    /// Buffer type (listed above, only set for H.264 and HEVC formats)
    pub ty: BufferType,
    pub data: &'a [u8],
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, Default)]
pub enum DecodeResult {
    #[default]
    Ok = DR_OK as i32,
    NeedIdr = DR_NEED_IDR,
}

// --------------- Audio ---------------

/// This structure provides the Opus multistream decoder parameters required to successfully
/// decode the audio stream being sent from the computer. See opus_multistream_decoder_init docs
/// for details about these fields.
///
/// The supplied mapping array is indexed according to the following output channel order:
/// 0 - Front Left
/// 1 - Front Right
/// 2 - Center
/// 3 - LFE
/// 4 - Back Left
/// 5 - Back Right
/// 6 - Side Left
/// 7 - Side Right
///
/// If the mapping order does not match the channel order of the audio renderer, you may swap
/// the values in the mismatched indices until the mapping array matches the desired channel order.
#[derive(Debug, Clone)]
pub struct OpusMultistreamConfig {
    pub sample_rate: u32,
    pub channel_count: u32,
    pub streams: u32,
    pub coupled_streams: u32,
    pub samples_per_frame: u32,
    pub mapping: [u8; AUDIO_CONFIGURATION_MAX_CHANNEL_COUNT as usize],
}

impl OpusMultistreamConfig {
    pub const STEREO: OpusMultistreamConfig = OpusMultistreamConfig {
        sample_rate: 48000,
        channel_count: 2,
        streams: 1,
        coupled_streams: 1,
        samples_per_frame: 960,
        mapping: [0, 1, 0, 0, 0, 0, 0, 0],
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioConfig {
    pub channel_count: u32,
    pub channel_mask: u32,
}

#[derive(Debug, Error)]
#[error("failed to deserialize audio config!")]
pub struct FromRawAudioConfigError;

impl AudioConfig {
    /// Specifies that the audio stream should be encoded in stereo (default)
    pub const STEREO: AudioConfig = Self::new(2, 0x03);
    /// Specifies that the audio stream should be in 5.1 surround sound if the PC is able
    pub const SURROUND_51: AudioConfig = Self::new(6, 0x3F);
    /// Specifies that the audio stream should be in 7.1 surround sound if the PC is able
    pub const SURROUND_71: AudioConfig = Self::new(8, 0x63F);

    /// Specifies an audio configuration by channel count and channel mask
    /// See https://docs.microsoft.com/en-us/windows-hardware/drivers/audio/channel-mask for channelMask values
    /// NOTE: Not all combinations are supported by GFE and/or this library.
    pub const fn new(channel_count: u32, channel_mask: u32) -> Self {
        Self {
            channel_count,
            channel_mask,
        }
    }

    pub const fn from_raw(raw: u32) -> Result<Self, FromRawAudioConfigError> {
        // Check the magic byte before decoding to make sure we got something that's actually
        // a MAKE_AUDIO_CONFIGURATION()-based value and not something else like an older version
        // hardcoded AUDIO_CONFIGURATION value from an earlier version of moonlight-common-c.
        if (raw & 0xFF) != 0xCA {
            return Err(FromRawAudioConfigError);
        }

        Ok(Self {
            channel_count: (raw >> 8) & 0xFF,
            channel_mask: (raw >> 16) & 0xFFFF,
        })
    }

    pub fn raw(&self) -> u32 {
        (self.channel_mask << 16) | (self.channel_count << 8) | 0xCA
    }
}

// --------------- Keyboard ---------------

#[repr(i8)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum KeyAction {
    Up = KEY_ACTION_UP as i8,
    Down = KEY_ACTION_DOWN as i8,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct KeyModifiers: i8 {
        const SHIFT = MODIFIER_SHIFT as i8;
        const CTRL = MODIFIER_CTRL as i8;
        const ALT = MODIFIER_ALT as i8;
        const META = MODIFIER_META as i8;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct KeyFlags: i8 {
        const NON_NORMALIZED = SS_KBE_FLAG_NON_NORMALIZED as i8;
    }
}

// --------------- Mouse ---------------

#[repr(i8)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum MouseButtonAction {
    Press = BUTTON_ACTION_PRESS as i8,
    Release = BUTTON_ACTION_RELEASE as i8,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, FromPrimitive)]
pub enum MouseButton {
    Left = BUTTON_LEFT as i32,
    Middle = BUTTON_MIDDLE as i32,
    Right = BUTTON_RIGHT as i32,
    X1 = BUTTON_X1 as i32,
    X2 = BUTTON_X2 as i32,
}

// --------------- Touch ---------------

#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum TouchEventType {
    Hover = LI_TOUCH_EVENT_HOVER,
    Down = LI_TOUCH_EVENT_DOWN,
    Up = LI_TOUCH_EVENT_UP,
    Move = LI_TOUCH_EVENT_MOVE,
    Cancel = LI_TOUCH_EVENT_CANCEL,
    ButtonOnly = LI_TOUCH_EVENT_BUTTON_ONLY,
    HoverLeave = LI_TOUCH_EVENT_HOVER_LEAVE,
    CancelAll = LI_TOUCH_EVENT_CANCEL_ALL,
}

// --------------- Controller ---------------

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ControllerButtons: u32 {
        const A        = A_FLAG;
        const B        = B_FLAG;
        const X        = X_FLAG;
        const Y        = Y_FLAG;
        const UP       = UP_FLAG;
        const DOWN     = DOWN_FLAG;
        const LEFT     = LEFT_FLAG;
        const RIGHT    = RIGHT_FLAG;
        const LB       = LB_FLAG;
        const RB       = RB_FLAG;
        const PLAY     = PLAY_FLAG;
        const BACK     = BACK_FLAG;
        const LS_CLK   = LS_CLK_FLAG;
        const RS_CLK   = RS_CLK_FLAG;
        const SPECIAL  = SPECIAL_FLAG;

        /// Extended buttons (Sunshine only)
        const PADDLE1  = PADDLE1_FLAG;
        /// Extended buttons (Sunshine only)
        const PADDLE2  = PADDLE2_FLAG;
        /// Extended buttons (Sunshine only)
        const PADDLE3  = PADDLE3_FLAG;
        /// Extended buttons (Sunshine only)
        const PADDLE4  = PADDLE4_FLAG;
        /// Extended buttons (Sunshine only)
        /// Touchpad buttons on Sony controllers
        const TOUCHPAD = TOUCHPAD_FLAG;
        /// Extended buttons (Sunshine only)
        /// Share/Mic/Capture/Mute buttons on various controllers
        const MISC     = MISC_FLAG;
    }
}
bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ActiveGamepads: u16 {
        const GAMEPAD_1  = 0b0000_0000_0000_0001;
        const GAMEPAD_2  = 0b0000_0000_0000_0010;
        const GAMEPAD_3  = 0b0000_0000_0000_0100;
        const GAMEPAD_4  = 0b0000_0000_0000_1000;

        /// Extended gamepads (Sunshine only)
        const GAMEPAD_5  = 0b0000_0000_0001_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_6  = 0b0000_0000_0010_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_7  = 0b0000_0000_0100_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_8  = 0b0000_0000_1000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_9  = 0b0000_0001_0000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_10 = 0b0000_0010_0000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_11 = 0b0000_0100_0000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_12 = 0b0000_1000_0000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_13 = 0b0001_0000_0000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_14 = 0b0010_0000_0000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_15 = 0b0100_0000_0000_0000;
        /// Extended gamepads (Sunshine only)
        const GAMEPAD_16 = 0b1000_0000_0000_0000;
    }
}

impl ActiveGamepads {
    pub fn from_id(id: u8) -> Option<Self> {
        if id >= 16 {
            return None;
        }
        Some(ActiveGamepads::from_bits_truncate(1 << id))
    }
}

/// Represents the type of controller.
///
/// This is used to inform the host of what type of controller has arrived,
/// which can help the host decide how to emulate it and what features to expose.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ControllerType {
    /// Unknown controller type.
    Unknown = LI_CTYPE_UNKNOWN as u8,
    /// Microsoft Xbox-compatible controller.
    Xbox = LI_CTYPE_XBOX as u8,
    /// Sony PlayStation-compatible controller.
    PlayStation = LI_CTYPE_PS as u8,
    /// Nintendo-compatible controller (e.g., Switch Pro Controller).
    Nintendo = LI_CTYPE_NINTENDO as u8,
}

bitflags! {
    /// Represents the capabilities of a controller.
    ///
    /// This is typically sent along with controller arrival information so the host
    /// knows which features the controller supports.
    #[derive(Debug, Clone, Copy)]
    pub struct ControllerCapabilities: u16 {
        /// Reports values between `0x00` and `0xFF` for trigger axes.
        const ANALOG_TRIGGERS  = LI_CCAP_ANALOG_TRIGGERS as u16;
        /// Can rumble in response to `ConnListenerRumble()` callback.
        const RUMBLE           = LI_CCAP_RUMBLE as u16;
        /// Can rumble triggers in response to `ConnListenerRumbleTriggers()` callback.
        const TRIGGER_RUMBLE   = LI_CCAP_TRIGGER_RUMBLE as u16;
        /// Reports touchpad events via `LiSendControllerTouchEvent()`.
        const TOUCHPAD         = LI_CCAP_TOUCHPAD as u16;
        /// Can report accelerometer events via `LiSendControllerMotionEvent()`.
        const ACCEL            = LI_CCAP_ACCEL as u16;
        /// Can report gyroscope events via `LiSendControllerMotionEvent()`.
        const GYRO             = LI_CCAP_GYRO as u16;
        /// Reports battery state via `LiSendControllerBatteryEvent()`.
        const BATTERY_STATE    = LI_CCAP_BATTERY_STATE as u16;
        /// Can set RGB LED state via `ConnListenerSetControllerLED()`.
        const RGB_LED          = LI_CCAP_RGB_LED as u16;
    }
}

bitflags! {
    /// Motion sensor types for [`LiSendControllerMotionEvent`].
    #[derive(Debug, Clone, Copy)]
    pub struct MotionType: u8 {
        /// Accelerometer data in m/s² (inclusive of gravitational acceleration).
        const ACCEL = LI_MOTION_TYPE_ACCEL as u8;
        /// Gyroscope data in degrees per second.
        const GYRO  = LI_MOTION_TYPE_GYRO as u8;
    }
}

bitflags! {
    /// Battery states for [`LiSendControllerBatteryEvent`].
    #[derive(Debug, Clone, Copy)]
    pub struct BatteryState: u8 {
        /// Unknown battery state.
        const UNKNOWN       = LI_BATTERY_STATE_UNKNOWN as u8;
        /// No battery present.
        const NOT_PRESENT   = LI_BATTERY_STATE_NOT_PRESENT as u8;
        /// Battery is discharging.
        const DISCHARGING   = LI_BATTERY_STATE_DISCHARGING as u8;
        /// Battery is charging.
        const CHARGING      = LI_BATTERY_STATE_CHARGING as u8;
        /// Connected to power but not charging.
        const NOT_CHARGING  = LI_BATTERY_STATE_NOT_CHARGING as u8;
        /// Battery is full.
        const FULL          = LI_BATTERY_STATE_FULL as u8;
    }
}

// --------------- Misc ---------------

bitflags! {
    #[derive(Debug, Clone)]
    pub struct HostFeatures: u32 {
        const PEN_TOUCH_EVENTS = LI_FF_PEN_TOUCH_EVENTS;
        const CONTROLLER_TOUCH_EVENTS = LI_FF_CONTROLLER_TOUCH_EVENTS;
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy)]
pub struct EstimatedRttInfo {
    pub rtt: Duration,
    pub rtt_variance: Duration,
}
