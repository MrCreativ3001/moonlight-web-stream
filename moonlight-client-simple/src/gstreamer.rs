use std::{io::Write, str::FromStr};

use gstreamer::{
    Buffer, BufferFlags, Caps, ClockTime, DebugGraphDetails, Element, ElementFactory, Format,
    Pipeline, State,
    event::Eos,
    glib::{self, object::ObjectExt},
    prelude::{ElementExt, ElementExtManual, GstBinExt, GstBinExtManual},
};
use gstreamer_app::AppSrc;
use moonlight_common::{
    audio::{AudioConfig, AudioDecoder, OpusMultistreamConfig},
    stream::Capabilities,
    video::{
        DecodeResult, FrameType, SupportedVideoFormats, VideoDecodeUnit, VideoDecoder, VideoFormat,
    },
};

pub fn init() {
    gstreamer::init().expect("failed to init gstreamer");
}

pub fn gstreamer_pipeline()
-> Result<(GStreamerVideoHandler, GStreamerAudioHandler), glib::BoolError> {
    let pipeline = Pipeline::new();

    // Video
    let (video_decoder, video_output) = GStreamerVideoHandler::new(pipeline.clone())?;

    let video_sink = ElementFactory::make_with_name("autovideosink", Some("play video"))?;
    video_sink.set_property("sync", false);
    video_sink.set_property("async-handling", true);

    pipeline.add(&video_sink)?;

    video_output.link(&video_sink)?;

    // Audio
    let (audio_decoder, audio_output) = GStreamerAudioHandler::new(pipeline.clone())?;

    let audio_sink = ElementFactory::make_with_name("autoaudiosink", Some("play audio"))?;
    audio_sink.set_property("sync", false);
    audio_sink.set_property("async-handling", true);

    pipeline.add(&audio_sink)?;

    audio_output.link(&audio_sink)?;

    let dot_data = pipeline.debug_to_dot_data(DebugGraphDetails::all());
    std::fs::write("appimages/pipeline.dot", dot_data).unwrap();

    Ok((video_decoder, audio_decoder))
}

pub struct GStreamerVideoHandler {
    pipeline: Pipeline,
    app_src: AppSrc,
}

impl GStreamerVideoHandler {
    pub fn new(pipeline: Pipeline) -> Result<(Self, Element), glib::BoolError> {
        let app_src = AppSrc::builder().name("moonlight video packets").build();
        app_src.set_is_live(true);
        app_src.set_format(Format::Buffers);
        app_src.set_block(false);
        app_src.set_do_timestamp(true);
        app_src.set_min_latency(-1);

        let parse = ElementFactory::make_with_name("h265parse", Some("parse packets"))?;
        parse.set_property("config-interval", 0);

        let decode = ElementFactory::make_with_name("avdec_h265", Some("decode video"))?;
        let convert = ElementFactory::make_with_name("videoconvert", Some("convert video"))?;

        pipeline
            .add_many([app_src.as_ref(), &parse, &decode, &convert])
            .unwrap();

        app_src.link(&parse)?;
        parse.link(&decode)?;
        decode.link(&convert)?;

        Ok((Self { pipeline, app_src }, convert))
    }
}

impl VideoDecoder for GStreamerVideoHandler {
    fn setup(
        &mut self,
        format: VideoFormat,
        width: u32,
        height: u32,
        redraw_rate: u32,
        flags: (),
    ) -> i32 {
        let _ = (format, width, height, redraw_rate, flags);
        0
    }

    fn start(&mut self) {
        self.pipeline.set_state(State::Playing).unwrap();
    }
    fn stop(&mut self) {
        self.pipeline.send_event(Eos::new());
        self.pipeline.set_state(State::Null).unwrap();
    }

    fn submit_decode_unit(&mut self, unit: VideoDecodeUnit<'_>) -> DecodeResult {
        if matches!(self.pipeline.current_state(), State::Null) {
            return DecodeResult::Ok;
        }

        for buffer in unit.buffers {
            let mut gst_buffer = Buffer::with_size(buffer.data.len()).unwrap();
            {
                let buffer_mut = gst_buffer.get_mut().unwrap();

                buffer_mut.copy_from_slice(0, buffer.data).unwrap();

                let pts_ns = unit.presentation_time_ms as u64 * 1_000_000;
                buffer_mut.set_pts(ClockTime::from_nseconds(pts_ns));
                buffer_mut.set_dts(ClockTime::from_nseconds(pts_ns));

                match unit.frame_type {
                    FrameType::Idr => {
                        // Keyframe (contains SPS/PPS/VPS + I-frame)
                        buffer_mut.set_flags(BufferFlags::empty());
                    }
                    FrameType::PFrame => {
                        // Predictive frame
                        buffer_mut.set_flags(BufferFlags::DELTA_UNIT);
                    }
                }
            }

            self.app_src.push_buffer(gst_buffer).unwrap();
        }

        DecodeResult::Ok
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::empty()
    }
    fn supported_formats(&self) -> SupportedVideoFormats {
        SupportedVideoFormats::H265
    }
}

pub struct GStreamerAudioHandler {
    pipeline: Pipeline,
    app_src: AppSrc,
}

impl GStreamerAudioHandler {
    pub fn new(pipeline: Pipeline) -> Result<(Self, Element), glib::BoolError> {
        let app_src = AppSrc::builder().name("moonlight_pcm_input").build();
        app_src.set_is_live(true);
        app_src.set_format(Format::Time);
        app_src.set_block(false);
        app_src.set_do_timestamp(true);

        let opusparse = ElementFactory::make_with_name("opusparse", Some("audio parse")).unwrap();
        let opusdec = ElementFactory::make_with_name("opusdec", Some("audio decode")).unwrap();
        let audioconvert =
            ElementFactory::make_with_name("audioconvert", Some("audio convert")).unwrap();
        let audioresample =
            ElementFactory::make_with_name("audioresample", Some("audio resample")).unwrap();

        pipeline.add_many([
            app_src.as_ref(),
            // &opusparse,
            &opusdec,
            &audioconvert,
            &audioresample,
        ])?;
        Element::link_many([
            app_src.as_ref(),
            // &opusparse,
            &opusdec,
            &audioconvert,
            &audioresample,
        ])?;

        // Configure appsrc caps (must match Opus stream properties)
        // This will be set later in setup()

        Ok((Self { pipeline, app_src }, audioresample))
    }
}

impl AudioDecoder for GStreamerAudioHandler {
    fn setup(
        &mut self,
        audio_config: AudioConfig,
        stream_config: OpusMultistreamConfig,
        ar_flags: (),
    ) -> i32 {
        let caps_str = "audio/x-opus, rate=48000, channels=2, channel-mapping-family=0";

        let caps = Caps::from_str(&caps_str).unwrap();
        self.app_src.set_caps(Some(&caps));

        // self.audio_config = Some(audio_config);

        0
    }

    fn start(&mut self) {
        self.pipeline.set_state(State::Playing).unwrap();
    }

    fn stop(&mut self) {
        self.pipeline.send_event(Eos::new());
        self.pipeline.set_state(State::Null).unwrap();
    }

    fn decode_and_play_sample(&mut self, data: &[u8]) {
        let mut buffer = Buffer::with_size(data.len()).unwrap();
        let buffer_mut = buffer.get_mut().unwrap();

        let _ = buffer_mut.copy_from_slice(0, data);
        let _ = self.app_src.push_buffer(buffer);
    }

    fn config(&self) -> AudioConfig {
        AudioConfig::STEREO
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::empty()
    }
}
