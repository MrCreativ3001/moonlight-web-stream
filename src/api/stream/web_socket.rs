use std::{
    pin::pin,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crate::api::{
    bindings::{
        StreamStatsClientboundMessage, StreamStatsServerboundMessage, WebSocketChannel,
        WebSocketClientboundMessage, WebSocketServerboundMessage, WebSocketStreamResponse,
    },
    stream::{PACKET_SIZE, apply_role_restrictions},
};
use actix_web::{Error, HttpRequest, HttpResponse, get, rt::spawn, web::Payload};
use actix_ws::{Message, MessageStream, Session};
use bytes::Bytes;
use moonlight_common::{
    AppId,
    crypto::rustcrypto::RustCryptoBackend,
    stream::{
        AesIv, AesKey, EncryptionFlags, MoonlightStreamSettings, StreamingConfig,
        audio::AudioConfig,
        control::ActiveGamepads,
        proto::{
            MoonlightStreamSetup,
            audio::AudioStreamEvent,
            control::{
                ControlStreamEvent,
                packet::{ControlPacket, ControlPacketConfig, PacketDirection},
            },
            video::VideoStreamEvent,
        },
        tokio::{MoonlightStream, MoonlightStreamEvent},
        video::{ColorRange, ColorSpace, VideoCapabilities, VideoFormats},
    },
};
use tokio::{
    select,
    sync::mpsc::{UnboundedSender, unbounded_channel},
    time::{interval, sleep},
};
use tracing::{Instrument, debug, debug_span, error, info, instrument, trace, warn};

use crate::{
    api::stream::create_control_packet_config,
    app::{AppError, host::HostId, user::AuthenticatedUser},
};

/// How long the message the sending task is writing may have been queued before
/// video frames are dropped. Measuring the age of the queue instead of its size
/// means a single large frame that was just queued does not count as a backlog.
const MAX_QUEUE_DELAY_MS: u64 = 250;
const MAX_QUEUE_DELAY: Duration = Duration::from_millis(MAX_QUEUE_DELAY_MS);
/// Queue delay that has to be reached again before video frames are sent again.
const RESUME_QUEUE_DELAY: Duration = Duration::from_millis(MAX_QUEUE_DELAY_MS / 4);
/// Minimum time between two idr requests while video frames are being dropped.
/// Requesting an idr for every dropped idr would make the host encode a keyframe
/// for almost every frame, which wastes bandwidth and does not recover any faster.
const IDR_RE_REQUEST_INTERVAL: Duration = Duration::from_millis(250);

/// Decides whether a video frame can be sent to the client or has to be
/// dropped because the client cannot keep up.
///
/// The queue delay is how long the message the sending task is currently
/// writing has been queued: it grows while the client is not reading. When it
/// exceeds the limit, video frames are dropped until the queue has drained and
/// an idr frame arrives that the client can resume decoding with.
struct VideoBacklog {
    /// Shared with the sending task so video that is already queued is dropped too.
    dropping_video: Arc<AtomicBool>,
    dropping_since: Instant,
    dropped_frames: u32,
    last_idr_request_at: Instant,
}

/// What to do with a video frame that is about to be queued for the client.
enum VideoFrameDecision {
    /// The client is keeping up, send the frame.
    Send,
    /// The client is too far behind, drop the frame.
    Drop {
        /// Whether a new idr frame has to be requested from the host.
        request_idr: bool,
    },
}

impl VideoBacklog {
    fn new(dropping_video: Arc<AtomicBool>) -> Self {
        let now = Instant::now();

        Self {
            dropping_video,
            dropping_since: now,
            dropped_frames: 0,
            last_idr_request_at: now,
        }
    }

    /// Returns whether video frames are currently being dropped.
    fn is_dropping(&self) -> bool {
        self.dropping_video.load(Ordering::Relaxed)
    }

    /// Decides what to do with the next video frame. `is_idr` has to be set for
    /// a key frame, `queue_delay` is how long the message currently being sent
    /// has been queued, and `now` is the time the frame arrived.
    fn on_frame(
        &mut self,
        is_idr: bool,
        queue_delay: Duration,
        now: Instant,
    ) -> VideoFrameDecision {
        if self.is_dropping() {
            if !is_idr || queue_delay > RESUME_QUEUE_DELAY {
                self.dropped_frames += 1;

                // Only ask for a new idr once the queue has drained. Asking
                // for one for every idr that arrives while the queue is still
                // being drained makes the host encode a keyframe for nearly
                // every frame, and all of them get dropped anyway.
                if queue_delay <= RESUME_QUEUE_DELAY
                    && now.duration_since(self.last_idr_request_at) >= IDR_RE_REQUEST_INTERVAL
                {
                    self.last_idr_request_at = now;
                    return VideoFrameDecision::Drop { request_idr: true };
                }

                return VideoFrameDecision::Drop { request_idr: false };
            }

            info!(
                dropped_for_ms = now.duration_since(self.dropping_since).as_millis() as u64,
                dropped_frames = self.dropped_frames,
                "web socket client caught up, resuming video"
            );
            self.dropping_video.store(false, Ordering::Relaxed);
        } else if queue_delay > MAX_QUEUE_DELAY {
            warn!(
                queue_delay_ms = queue_delay.as_millis() as u64,
                queue_delay_limit_ms = MAX_QUEUE_DELAY_MS,
                "web socket client is behind, dropping video until the next idr"
            );
            self.dropping_video.store(true, Ordering::Relaxed);
            self.dropping_since = now;
            self.dropped_frames = 1;
            self.last_idr_request_at = now;

            return VideoFrameDecision::Drop { request_idr: true };
        }

        VideoFrameDecision::Send
    }
}

enum WsData {
    Bytes(Bytes),
    Video(Bytes),
    Text(String),
}

#[get("/host/stream/web_socket")]
#[instrument(skip(user, req, body_stream), fields(user = %user.id()))]
pub async fn web_socket_stream(
    mut user: AuthenticatedUser,
    req: HttpRequest,
    body_stream: Payload,
) -> Result<HttpResponse, Error> {
    if !user
        .role()
        .await?
        .permissions()
        .await?
        .allow_transport_websockets
    {
        return Err(AppError::Forbidden.into());
    }

    // upgrade connection to web socket connection
    let (res, ws_sender, ws_receiver) = actix_ws::handle(&req, body_stream)?;

    spawn(
        async move {
            match handle_ws(user, ws_sender, ws_receiver).await {
                Ok(_) => {}
                Err(err) => {
                    error!(error = %err, "stream failed");
                }
            }
        }
        .instrument(debug_span!("ws handler")),
    );

    Ok(res)
}

async fn handle_ws(
    mut user: AuthenticatedUser,
    mut ws_sender: Session,
    mut ws_receiver: MessageStream,
) -> Result<(), AppError> {
    let control_config = create_control_packet_config();

    // See if the user is allowed to use web sockets
    let permissions = user.role().await?.permissions().await?;
    if !permissions.allow_transport_websockets {
        return Err(AppError::Forbidden);
    }

    // Wait for stream request
    let stream_request = select! {
        _ = sleep(Duration::from_secs(10)) => {
            return Err(AppError::StreamClosed);
        }
        request = ws_receiver.recv() => request
    };

    // Deserialize message
    let stream_request = match stream_request.expect("stream request") {
        Ok(Message::Text(text)) => text,
        Ok(message) => {
            error!(message = ?message, "web socket received unexpected start message");
            return Err(AppError::StreamClosed);
        }
        Err(err) => {
            error!(error = %err, "web socket protocol error");
            return Err(AppError::StreamClosed);
        }
    };
    let stream_request = match serde_json::from_str::<WebSocketServerboundMessage>(&stream_request)
    {
        Ok(WebSocketServerboundMessage::Request(request)) => request,
        Ok(message) => {
            error!(message = ?message, "expected web socket stream request but got another message");
            return Err(AppError::StreamClosed);
        }
        Err(err) => {
            error!(error = %err, "failed to deserialize json");
            return Err(AppError::StreamClosed);
        }
    };

    // -- Get host
    let host_id = HostId(stream_request.host_id);
    let mut host = user.host(host_id).await?;

    let host = host.use_host(&mut user).await?;

    if !host.is_paired().await.map_err(AppError::from)? {
        return Err(AppError::HostNotPaired);
    }

    // -- Get Apps
    let app_id = AppId(stream_request.app_id);
    let apps = host.app_list().await?;
    let app_title = apps
        .into_iter()
        .find(|app| app.id == app_id)
        .map(|app| app.title);

    // -- Start stream
    // get settings
    let mut settings = MoonlightStreamSettings {
        width: stream_request.width,
        height: stream_request.height,
        fps: stream_request.fps,
        fps_x100: stream_request.fps * 100,
        bitrate: stream_request.bitrate,
        packet_size: PACKET_SIZE,
        encryption_flags: EncryptionFlags::AUDIO | EncryptionFlags::FOUNDATION_MICROPHONE,
        streaming_remotely: StreamingConfig::Auto,
        sops: true,
        hdr: stream_request.hdr,
        supported_video_formats: VideoFormats::from_bits_retain(stream_request.supported_codecs),
        // TODO: color range?
        color_space: ColorSpace::Rec709,
        color_range: ColorRange::Limited,
        local_audio_play_mode: stream_request.local_audio_play_mode,
        audio_config: AudioConfig::STEREO,
        gamepads_attached: ActiveGamepads::empty(),
        gamepads_persist_after_disconnect: false,
        // TODO: mic?
        enable_mic: false,
    };

    // apply permissions
    apply_role_restrictions(&permissions, &mut settings);

    // adjust settings
    let server_version = host.version().await?;
    let gfe_version = host.gfe_version().await?;
    let server_codec_mode_support = host.server_codec_mode_support().await?;
    settings.adjust_for_server(server_version, &gfe_version, server_codec_mode_support)?;

    // Drop video for clients that cannot keep up with the stream.
    let dropping_video = Arc::new(AtomicBool::new(false));
    let video_backlog = VideoBacklog::new(dropping_video.clone());

    // encryption
    let aes_key = AesKey::new_random(&RustCryptoBackend)?;
    let aes_iv = AesIv::new_random(&RustCryptoBackend)?;

    info!(settings = ?settings, "starting stream");

    // start stream
    let config = host
        .start_stream(
            app_id,
            &settings,
            aes_key,
            aes_iv,
            MoonlightStreamSetup::launch_query_parameters(),
        )
        .await?;

    let stream = MoonlightStream::connect(
        config,
        settings,
        Arc::new(RustCryptoBackend),
        VideoCapabilities::default(),
    )
    .await?;

    // send stream start response
    let audio_setup = stream.audio_setup();
    let video_setup = stream.video_setup();

    let response = WebSocketClientboundMessage::Response(WebSocketStreamResponse {
        video_codec: video_setup.format as u32,
        audio_sample_rate: audio_setup.sample_rate,
        audio_channel_count: audio_setup.channel_count,
        audio_streams: audio_setup.streams,
        audio_coupled_streams: audio_setup.coupled_streams,
        audio_samples_per_frame: audio_setup.samples_per_frame,
        audio_mapping: audio_setup.mapping,
        app_name: app_title,
    });
    info!(response = ?response, "sending response to client");

    let (mut ws_channel_sender, mut ws_channel_receiver) = unbounded_channel();

    // When the message the sending task is currently writing was queued, None while idle.
    let sending_queued_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let sending_queued_at_sender = sending_queued_at.clone();
    let dropping_video_sender = dropping_video.clone();
    spawn(
        async move {
            while let Some((queued_at, data)) = ws_channel_receiver.recv().await {
                if let WsData::Video(_) = &data
                    && dropping_video_sender.load(Ordering::Relaxed)
                {
                    continue;
                }

                set_sending_queued_at(&sending_queued_at_sender, Some(queued_at));
                let result = match data {
                    WsData::Bytes(bytes) | WsData::Video(bytes) => ws_sender.binary(bytes).await,
                    WsData::Text(text) => ws_sender.text(text).await,
                };
                set_sending_queued_at(&sending_queued_at_sender, None);

                if result.is_err() {
                    break;
                }
            }

            debug!("stopped web socket sending task");
        }
        .instrument(debug_span!("ws_sender")),
    );

    // send response
    send_ws_message(&mut ws_channel_sender, response);

    // main loop
    if let Err(err) = ws_loop(
        ws_channel_sender,
        sending_queued_at,
        video_backlog,
        ws_receiver,
        stream,
        control_config,
    )
    .await
    {
        error!(error = %err, "web socket main loop errored, closing stream");
    }

    Ok(())
}

async fn ws_loop(
    mut ws_sender: UnboundedSender<(Instant, WsData)>,
    sending_queued_at: Arc<Mutex<Option<Instant>>>,
    mut video_backlog: VideoBacklog,
    mut ws_receiver: MessageStream,
    mut stream: MoonlightStream,
    control_config: ControlPacketConfig,
) -> Result<(), AppError> {
    let mut relay_stats_ticker = pin!(interval(Duration::from_secs(1)));

    let mut ws_stopped = false;

    loop {
        if !stream.is_alive() {
            break;
        }

        select! {
            // drive the moonlight stream forward
            result = stream.drive() => {
                let event = result?;

                match event {
                    MoonlightStreamEvent::Audio(AudioStreamEvent::OnFrame(frame)) => {
                        if ws_stopped {
                            continue;
                        }

                        let mut buffer = vec![0; 1 + frame.buffer.len()];
                        buffer[1..].copy_from_slice(&frame.buffer);

                        buffer[0] = WebSocketChannel::AUDIO;

                        let _ = queue_ws_data(&ws_sender, WsData::Bytes(buffer.into()));
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::SignalIdr) => {
                        if let Err(err)=  stream.send_raw(ControlPacket::RequestIdr) {
                            warn!(error = %err, "failed to request idr after the moonlight video stream requested an idr");
                        }
                    }
                    MoonlightStreamEvent::Video(VideoStreamEvent::OnFrame(frame)) => {
                        if ws_stopped {
                            continue;
                        }

                        // TODO: make frame type from video packet public, 2==Idr
                        let is_idr = frame.metadata().frame_type.serialize() == 2;
                        let now = Instant::now();
                        let queue_delay = get_sending_queued_at(&sending_queued_at)
                            .map_or(Duration::ZERO, |queued_at| now.duration_since(queued_at));

                        if let VideoFrameDecision::Drop { request_idr } =
                            video_backlog.on_frame(is_idr, queue_delay, now)
                        {
                            if request_idr
                                && let Err(err) = stream.send_raw(ControlPacket::RequestIdr)
                            {
                                warn!(error = %err, "failed to request idr after dropping video frames");
                            }

                            continue;
                        }

                        // TODO: avoid using payloading and depayloading the frame like this
                        let mut buffer = vec![0; 1 + 5 + frame.raw().len()];
                        buffer[(1 + 5)..].copy_from_slice(frame.raw());

                        buffer[0] = WebSocketChannel::VIDEO;
                        buffer[1] = if is_idr { 1 } else { 0 };
                        buffer[2..6].copy_from_slice(
                            &(frame.metadata().timestamp.as_micros() as u32).to_be_bytes(),
                        );

                        let _ = queue_ws_data(&ws_sender, WsData::Video(buffer.into()));
                    }
                    MoonlightStreamEvent::Control(ControlStreamEvent::Packet(packet)) => {
                        let mut buffer = vec![0; ControlPacket::MAX_SIZE + 1];

                        buffer[0] = WebSocketChannel::CONTROL;

                        #[allow(clippy::unwrap_used)]
                        let packet_len = packet
                            .serialize(&control_config, buffer[1..].as_mut_array().unwrap())
                            .unwrap();

                        buffer.truncate(1 + packet_len);
                        let _ = queue_ws_data(&ws_sender, WsData::Bytes(buffer.into()));
                    }
                    _ => {}
                }
            }
            // relay stats
            _ = relay_stats_ticker.tick() => {
                let rtt = match stream.estimated_rtt() {
                    Ok(value) => value,
                    Err(err) => {
                        warn!(error = %err, "failed to send rtt to client");
                        break;
                    }
                };

                send_ws_message(
                    &mut ws_sender,
                    WebSocketClientboundMessage::Stats(StreamStatsClientboundMessage::RelayRtt {
                        rtt_ms: rtt.rtt.as_millis() as u32,
                        rtt_variance_ms: rtt.rtt_variance.as_millis() as u32,
                    })
                );
            }
            // Handle incoming ws requests
            result = ws_receiver.recv(), if !ws_stopped => {
                let Some(Ok(message)) = result else {
                    ws_stopped = true;
                    let _ = stream.disconnect();
                    continue;
                };

                match message {
                    Message::Binary(message) => {
                        if message.is_empty() {
                            continue;
                        }

                        if message[0] == WebSocketChannel::CONTROL {
                            let Some(packet) = ControlPacket::deserialize(
                                PacketDirection::ServerBound,
                                &control_config,
                                &message[1..],
                            ) else {
                                warn!(message = ?message, "received unknown control packet");
                                continue;
                            };

                            if let Err(err) = stream.send_raw(packet) {
                                warn!(error = %err, "failed to send control packet");
                            }
                        }
                    }
                    Message::Text(text) => {
                        let message = match serde_json::from_str::<WebSocketServerboundMessage>(&text) {
                            Ok(value) => value,
                            Err(err) => {
                                warn!(error = %err, "failed to deserialize serverbound web socket message");
                                continue;
                            }
                        };

                        if let WebSocketServerboundMessage::Stats(StreamStatsServerboundMessage::Ping(id)) =
                            message
                        {
                            send_ws_message(&mut ws_sender, WebSocketClientboundMessage::Stats(StreamStatsClientboundMessage::Pong(id)));
                        }
                    }
                    Message::Close(_) => {
                        // The client closed the web socket. Stop the host stream,
                        // but keep driving it so the host gets a graceful
                        // disconnect, and stop relaying video and audio to the
                        // closed connection.
                        info!("client closed the web socket, stopping the host stream");
                        ws_stopped = true;
                        let _ = stream.disconnect();
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

fn set_sending_queued_at(sending_queued_at: &Mutex<Option<Instant>>, value: Option<Instant>) {
    *sending_queued_at
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = value;
}

fn get_sending_queued_at(sending_queued_at: &Mutex<Option<Instant>>) -> Option<Instant> {
    *sending_queued_at
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn queue_ws_data(sender: &UnboundedSender<(Instant, WsData)>, data: WsData) -> bool {
    if let Err(err) = sender.send((Instant::now(), data)) {
        warn!(error = %err, "failed to send web socket message");
        return false;
    }

    true
}

fn send_ws_message(
    sender: &mut UnboundedSender<(Instant, WsData)>,
    message: WebSocketClientboundMessage,
) -> bool {
    trace!(message = ?message, "sending text message to client");

    let text = match serde_json::to_string(&message) {
        Ok(value) => value,
        Err(err) => {
            warn!(error = %err, "failed to send web socket message");
            return false;
        }
    };

    queue_ws_data(sender, WsData::Text(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    /// Creates a backlog policy with a fresh start time.
    fn backlog() -> (VideoBacklog, Instant) {
        (
            VideoBacklog::new(Arc::new(AtomicBool::new(false))),
            Instant::now(),
        )
    }

    #[test]
    fn sends_frames_while_the_queue_delay_is_at_the_limit() {
        let (mut backlog, now) = backlog();

        assert!(matches!(
            backlog.on_frame(false, MAX_QUEUE_DELAY, now),
            VideoFrameDecision::Send
        ));
        assert!(!backlog.is_dropping());
    }

    #[test]
    fn sends_frames_while_a_large_frame_was_just_queued() {
        let (mut backlog, now) = backlog();

        // A large frame that was queued right before this one is no backlog.
        assert!(matches!(
            backlog.on_frame(false, Duration::ZERO, now),
            VideoFrameDecision::Send
        ));
        assert!(!backlog.is_dropping());
    }

    #[test]
    fn starts_dropping_and_requests_an_idr_when_the_queue_delay_is_too_large() {
        let (mut backlog, now) = backlog();

        assert!(matches!(
            backlog.on_frame(true, MAX_QUEUE_DELAY + ms(1), now),
            VideoFrameDecision::Drop { request_idr: true }
        ));
        assert!(backlog.is_dropping());
    }

    #[test]
    fn keeps_dropping_until_the_queue_drained_and_an_idr_arrives() {
        let (mut backlog, now) = backlog();

        assert!(matches!(
            backlog.on_frame(true, MAX_QUEUE_DELAY + ms(1), now),
            VideoFrameDecision::Drop { .. }
        ));

        // Still dropping, so no idr request yet: the last one was just sent.
        assert!(matches!(
            backlog.on_frame(false, Duration::ZERO, now),
            VideoFrameDecision::Drop { request_idr: false }
        ));
        assert!(backlog.is_dropping());

        // An idr while the queue has not drained is dropped, and asking for
        // another one would just make the host encode another keyframe.
        assert!(matches!(
            backlog.on_frame(
                true,
                RESUME_QUEUE_DELAY + ms(1),
                now + IDR_RE_REQUEST_INTERVAL
            ),
            VideoFrameDecision::Drop { request_idr: false }
        ));
        assert!(backlog.is_dropping());

        // Once the queue has drained, the idr is used to resume.
        assert!(matches!(
            backlog.on_frame(true, RESUME_QUEUE_DELAY, now + IDR_RE_REQUEST_INTERVAL),
            VideoFrameDecision::Send
        ));
        assert!(!backlog.is_dropping());
    }

    #[test]
    fn re_requests_an_idr_after_the_interval_once_the_queue_drained() {
        let (mut backlog, now) = backlog();

        assert!(matches!(
            backlog.on_frame(false, MAX_QUEUE_DELAY + ms(1), now),
            VideoFrameDecision::Drop { request_idr: true }
        ));

        // The queue has drained but the interval has not passed yet.
        assert!(matches!(
            backlog.on_frame(false, Duration::ZERO, now + IDR_RE_REQUEST_INTERVAL - ms(1)),
            VideoFrameDecision::Drop { request_idr: false }
        ));

        assert!(matches!(
            backlog.on_frame(false, Duration::ZERO, now + IDR_RE_REQUEST_INTERVAL),
            VideoFrameDecision::Drop { request_idr: true }
        ));

        // Rate limited again.
        assert!(matches!(
            backlog.on_frame(
                false,
                Duration::ZERO,
                now + IDR_RE_REQUEST_INTERVAL * 2 - ms(1)
            ),
            VideoFrameDecision::Drop { request_idr: false }
        ));

        assert!(matches!(
            backlog.on_frame(false, Duration::ZERO, now + IDR_RE_REQUEST_INTERVAL * 2),
            VideoFrameDecision::Drop { request_idr: true }
        ));
    }

    #[test]
    fn does_not_re_request_an_idr_while_the_queue_has_not_drained() {
        let (mut backlog, now) = backlog();

        assert!(matches!(
            backlog.on_frame(false, MAX_QUEUE_DELAY + ms(1), now),
            VideoFrameDecision::Drop { request_idr: true }
        ));

        // Even after the interval, no new idr is requested while the message
        // being sent has been queued longer than the resume delay.
        assert!(matches!(
            backlog.on_frame(
                false,
                RESUME_QUEUE_DELAY + ms(1),
                now + Duration::from_secs(1)
            ),
            VideoFrameDecision::Drop { request_idr: false }
        ));
    }
}
