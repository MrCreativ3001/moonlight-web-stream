#![feature(if_let_guard)]
#![feature(async_fn_traits)]

use std::{
    panic,
    process::exit,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use common::{
    StreamSettings,
    api_bindings::{
        GeneralClientMessage, GeneralServerMessage, LogMessageType, StreamClientMessage,
        TransportType,
    },
    ipc::{
        IpcReceiver, IpcSender, ServerIpcMessage, StreamerConfig, StreamerIpcMessage,
        create_process_ipc,
    },
};
use log::{LevelFilter, debug, error, info, trace, warn};
use moonlight_common::{
    MoonlightError,
    high::{HostError, MoonlightHost, StreamConfigError},
    network::backend::reqwest::ReqwestClient,
    pair::ClientAuth,
    stream::{
        MoonlightInstance, MoonlightStream,
        bindings::{
            ActiveGamepads, ColorRange, ConnectionStatus, ControllerButtons, EncryptionFlags,
            HostFeatures, OpusMultistreamConfig, Stage, VideoFormat,
        },
        connection::ConnectionListener,
        video::VideoSetup,
    },
};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use tokio::{
    io::{stdin, stdout},
    runtime::Handle,
    spawn,
    sync::{Mutex, Notify, RwLock},
    task::spawn_blocking,
    time::sleep,
};

use common::api_bindings::{StreamCapabilities, StreamServerMessage};

use crate::{
    audio::StreamAudioDecoder,
    transport::{
        InboundPacket, OutboundPacket, TransportError, TransportEvent, TransportEvents,
        TransportSender, web_socket,
        webrtc::{self},
    },
    video::StreamVideoDecoder,
};

pub type RequestClient = ReqwestClient;

pub const TIMEOUT_DURATION: Duration = Duration::from_secs(10);

mod audio;
mod buffer;
mod convert;
mod transport;
mod video;

#[tokio::main]
async fn main() {
    let default_panic = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        default_panic(info);
        exit(0);
    }));

    // At this point we're authenticated
    let (mut ipc_sender, mut ipc_receiver) =
        create_process_ipc::<ServerIpcMessage, StreamerIpcMessage>(stdin(), stdout()).await;

    // Send stage
    ipc_sender
        .send(StreamerIpcMessage::WebSocket(
            StreamServerMessage::DebugLog {
                message: "Completed Stage: Launch Streamer".to_string(),
                ty: None,
            },
        ))
        .await;

    let (
        config,
        host_address,
        host_http_port,
        client_unique_id,
        client_private_key,
        client_certificate,
        server_certificate,
        app_id,
        video_frame_queue_size,
        audio_sample_queue_size,
    ) = loop {
        match ipc_receiver.recv().await {
            Some(ServerIpcMessage::Init {
                config,
                host_address,
                host_http_port,
                client_unique_id,
                client_private_key,
                client_certificate,
                server_certificate,
                app_id,
                video_frame_queue_size,
                audio_sample_queue_size,
            }) => {
                break (
                    config,
                    host_address,
                    host_http_port,
                    client_unique_id,
                    client_private_key,
                    client_certificate,
                    server_certificate,
                    app_id,
                    video_frame_queue_size,
                    audio_sample_queue_size,
                );
            }
            _ => continue,
        }
    };

    TermLogger::init(
        config.log_level,
        simplelog::ConfigBuilder::new()
            .add_filter_ignore_str("webrtc_sctp")
            .set_time_level(LevelFilter::Off)
            .build(),
        TerminalMode::Stderr,
        ColorChoice::Never,
    )
    .expect("failed to init logger");

    // Send stage
    ipc_sender
        .send(StreamerIpcMessage::WebSocket(
            StreamServerMessage::DebugLog {
                message: "Waiting for Transport to negotiate".to_string(),
                ty: None,
            },
        ))
        .await;

    // -- Create the host and pair it
    let mut host = MoonlightHost::new(host_address, host_http_port, client_unique_id)
        .expect("failed to create host");

    host.set_pairing_info(
        &ClientAuth {
            certificate: client_certificate,
            private_key: client_private_key,
        },
        &server_certificate,
    )
    .expect("failed to set pairing info");

    // -- Configure moonlight
    let moonlight = MoonlightInstance::global().expect("failed to find moonlight");

    // -- Create and Configure Peer
    let connection = StreamConnection::new(
        moonlight,
        StreamInfo {
            host: Mutex::new(host),
            app_id,
        },
        ipc_sender.clone(),
        ipc_receiver,
        config,
        video_frame_queue_size,
        audio_sample_queue_size,
    )
    .await
    .expect("failed to create connection");

    // Send Info for streamer
    ipc_sender
        .send(StreamerIpcMessage::WebSocket(StreamServerMessage::Setup {
            ice_servers: connection.config.webrtc.ice_servers.clone(),
        }))
        .await;

    // Wait for termination
    connection.terminate.notified().await;

    // Exit streamer
    exit(0);
}

struct StreamInfo {
    host: Mutex<MoonlightHost<RequestClient>>,
    app_id: u32,
}

struct StreamSetup {
    video: Option<VideoSetup>,
    audio: Option<OpusMultistreamConfig>,
}

struct StreamConnection {
    pub runtime: Handle,
    pub moonlight: MoonlightInstance,
    pub config: StreamerConfig,
    pub info: StreamInfo,
    pub ipc_sender: IpcSender<StreamerIpcMessage>,
    // Video
    pub video_frame_queue_size: usize,
    pub audio_sample_queue_size: usize,
    pub stream_setup: Mutex<StreamSetup>,
    // Stream
    pub stream: RwLock<Option<MoonlightStream>>,
    pub active_gamepads: RwLock<ActiveGamepads>,
    pub transport_sender: Mutex<Option<Box<dyn TransportSender + Send + Sync + 'static>>>,
    // Timeout / Terminate
    pub timeout_terminate_request: Mutex<Option<Instant>>,
    pub terminate: Notify,
    is_terminating: AtomicBool,
}

impl StreamConnection {
    pub async fn new(
        moonlight: MoonlightInstance,
        info: StreamInfo,
        ipc_sender: IpcSender<StreamerIpcMessage>,
        mut ipc_receiver: IpcReceiver<ServerIpcMessage>,
        config: StreamerConfig,
        video_frame_queue_size: usize,
        audio_sample_queue_size: usize,
    ) -> Result<Arc<Self>, anyhow::Error> {
        let this = Arc::new(Self {
            runtime: Handle::current(),
            moonlight,
            config,
            info,
            ipc_sender,
            stream_setup: Mutex::new(StreamSetup {
                video: None,
                audio: None,
            }),
            video_frame_queue_size,
            audio_sample_queue_size,
            stream: RwLock::new(None),
            active_gamepads: RwLock::new(ActiveGamepads::empty()),
            transport_sender: Mutex::new(None),
            timeout_terminate_request: Default::default(),
            terminate: Notify::default(),
            is_terminating: AtomicBool::new(false),
        });

        spawn({
            let this = Arc::downgrade(&this);

            async move {
                while let Some(message) = ipc_receiver.recv().await {
                    let Some(this) = this.upgrade() else {
                        debug!("Received ipc message while the main type is already deallocated");
                        return;
                    };

                    if let ServerIpcMessage::Stop = &message {
                        this.on_ipc_message(ServerIpcMessage::Stop).await;
                        return;
                    }

                    this.on_ipc_message(message).await;
                }
            }
        });

        Ok(this)
    }

    async fn set_transport(
        self: &Arc<Self>,
        new_sender: Box<dyn TransportSender + Send + Sync + 'static>,
        mut events: Box<dyn TransportEvents + Send + Sync + 'static>,
    ) {
        let this = self.clone();

        let old_transport = {
            let mut sender = this.transport_sender.lock().await;
            sender.replace(new_sender)
        };

        spawn({
            let mut ipc_sender = this.ipc_sender.clone();
            let this = Arc::downgrade(&this);

            async move {
                loop {
                    trace!("Polling new transport event");
                    let event = events.poll_event().await;
                    trace!("Polled transport event: {event:?}");

                    match event {
                        Ok(TransportEvent::SendIpc(message)) => {
                            ipc_sender.send(message).await;
                        }
                        Ok(TransportEvent::StartStream { settings }) => {
                            let Some(this) = this.upgrade() else {
                                warn!(
                                    "Failed to get stream connection, stopping listening to events"
                                );
                                return;
                            };

                            let this = this.clone();
                            spawn(async move {
                                if let Err(err) = this.start_stream(settings).await {
                                    error!("Failed to start stream, stopping: {err}");

                                    this.stop().await;
                                }
                            });
                        }
                        Ok(TransportEvent::RecvPacket(packet)) => {
                            let Some(this) = this.upgrade() else {
                                warn!(
                                    "Failed to get stream connection, stopping listening to events"
                                );
                                return;
                            };

                            this.on_packet(packet).await;
                        }
                        Err(TransportError::Closed) | Ok(TransportEvent::Closed) => {
                            let Some(this) = this.upgrade() else {
                                warn!(
                                    "Failed request session termination because of missing stream (maybe it was already terminated)"
                                );
                                return;
                            };

                            this.request_terminate().await;

                            break;
                        }
                        // It wouldn't make sense to return this
                        Err(TransportError::ChannelClosed) => unreachable!(),
                        Err(TransportError::Implementation(err)) => {
                            let Some(this) = this.upgrade() else {
                                warn!(
                                    "Failed to get stream connection, stopping listening to events"
                                );
                                return;
                            };

                            info!(
                                "Stopping stream because of transport implementation error: {err}"
                            );

                            this.stop().await;
                            break;
                        }
                    }
                }
            }
        });

        if let Some(old_transport) = old_transport {
            spawn(async move {
                if let Err(err) = old_transport.close().await {
                    warn!("Failed to close old transport: {err:?}");
                }
            });
        }
    }
    async fn try_send_packet(&self, packet: OutboundPacket, packet_ty: &str, should_warn: bool) {
        let mut sender = self.transport_sender.lock().await;

        if let Some(sender) = sender.as_mut() {
            if let Err(err) = sender.send(packet).await {
                if should_warn {
                    warn!("Failed to send outbound packet: {packet_ty}, {err:?}");
                } else {
                    debug!("Failed to send outbound packet: {packet_ty}, {err:?}");
                }
            }
        } else {
            debug!("Dropping packet {packet:?} because no transport is selected!");
        }
    }

    async fn on_packet(&self, packet: InboundPacket) {
        let stream_lock = self.stream.read().await;
        let Some(stream) = stream_lock.as_ref() else {
            warn!("Failed to send packet {packet:?} because of missing stream");
            return;
        };

        let err = match packet {
            InboundPacket::General { message } => {
                debug!("General message: {message:?}");

                // currently there are no packets associated with that
                match message {
                    GeneralClientMessage::Stop => {
                        debug!("Received stop from client. Stopping stream now!");

                        drop(stream_lock);

                        self.stop().await;

                        None
                    }
                }
            }
            InboundPacket::MousePosition {
                x,
                y,
                reference_width,
                reference_height,
            } => stream
                .send_mouse_position(x, y, reference_width, reference_height)
                .err(),
            InboundPacket::MouseButton { action, button } => {
                stream.send_mouse_button(action, button).err()
            }
            InboundPacket::MouseMove { delta_x, delta_y } => {
                stream.send_mouse_move(delta_x, delta_y).err()
            }
            InboundPacket::HighResScroll { delta_x, delta_y } => {
                let mut err = None;
                if delta_y != 0 {
                    err = stream.send_high_res_scroll(delta_y).err()
                }
                if delta_x != 0 {
                    err = stream.send_high_res_horizontal_scroll(delta_x).err()
                }
                err
            }
            InboundPacket::Scroll { delta_x, delta_y } => {
                let mut err = None;
                if delta_y != 0 {
                    err = stream.send_scroll(delta_y).err();
                }
                if delta_x != 0 {
                    err = stream.send_horizontal_scroll(delta_x).err();
                }
                err
            }
            InboundPacket::Key {
                action,
                modifiers,
                key,
                flags,
            } => stream
                .send_keyboard_event_non_standard(key as i16, action, modifiers, flags)
                .err(),
            InboundPacket::Text { text } => stream.send_text(&text).err(),
            InboundPacket::Touch {
                pointer_id,
                x,
                y,
                pressure_or_distance,
                contact_area_major,
                contact_area_minor,
                rotation,
                event_type,
            } => stream
                .send_touch(
                    pointer_id,
                    x,
                    y,
                    pressure_or_distance,
                    contact_area_major,
                    contact_area_minor,
                    rotation,
                    event_type,
                )
                .err(),
            InboundPacket::ControllerConnected {
                id,
                ty,
                supported_buttons,
                capabilities,
            } => {
                let Some(gamepad) = ActiveGamepads::from_id(id) else {
                    warn!("Failed to add gamepad because it is out of range: {id}");
                    return;
                };

                let mut active_gamepads = self.active_gamepads.write().await;

                active_gamepads.insert(gamepad);

                stream
                    .send_controller_arrival(
                        id,
                        *active_gamepads,
                        ty,
                        supported_buttons,
                        capabilities,
                    )
                    .err()
            }
            InboundPacket::ControllerDisconnected { id } => {
                let Some(gamepad) = ActiveGamepads::from_id(id) else {
                    warn!("Failed to remove gamepad because it is out of range: {id}");
                    return;
                };

                let mut active_gamepads = self.active_gamepads.write().await;
                active_gamepads.remove(gamepad);

                stream
                    .send_multi_controller(
                        id,
                        *active_gamepads,
                        ControllerButtons::empty(),
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    )
                    .err()
            }
            InboundPacket::ControllerState {
                id,
                buttons,
                left_trigger,
                right_trigger,
                left_stick_x,
                left_stick_y,
                right_stick_x,
                right_stick_y,
            } => {
                let Some(gamepad) = ActiveGamepads::from_id(id) else {
                    warn!("Failed to update gamepad state because it is out of range: {id}");
                    return;
                };

                let active_gamepads = self.active_gamepads.read().await;
                if !active_gamepads.contains(gamepad) {
                    warn!(
                        "Failed to send gamepad event for not registered gamepad, gamepad: {id}, currently active: {:?}",
                        *active_gamepads
                    );
                    return;
                }

                stream
                    .send_multi_controller(
                        id,
                        *active_gamepads,
                        buttons,
                        left_trigger,
                        right_trigger,
                        left_stick_x,
                        left_stick_y,
                        right_stick_x,
                        right_stick_y,
                    )
                    .err()
            }
            _ => None,
        };

        if let Some(err) = err {
            warn!("Failed to handle packet: {err:?}");
        }
    }

    async fn on_ipc_message(self: &Arc<StreamConnection>, message: ServerIpcMessage) {
        if let ServerIpcMessage::WebSocket(StreamClientMessage::SetTransport(transport_type)) =
            &message
        {
            self.clear_terminate_request().await;

            match transport_type {
                TransportType::WebRTC => {
                    info!("Trying WebRTC transport");

                    let (sender, events) = match webrtc::new(
                        &self.config.webrtc,
                        self.video_frame_queue_size,
                        self.audio_sample_queue_size,
                    )
                    .await
                    {
                        Ok(value) => value,
                        Err(err) => {
                            error!("Failed to start webrtc transport: {err}");
                            return;
                        }
                    };
                    self.set_transport(Box::new(sender), Box::new(events)).await;
                }
                TransportType::WebSocket => {
                    info!("Trying Web Socket transport");

                    let (sender, events) = match web_socket::new().await {
                        Ok(value) => value,
                        Err(err) => {
                            error!("Failed to start web socket transport: {err}");
                            return;
                        }
                    };
                    self.set_transport(Box::new(sender), Box::new(events)).await;
                }
            }
        }

        let mut sender = self.transport_sender.lock().await;
        if let Some(sender) = sender.as_mut() {
            if let Err(err) = sender.on_ipc_message(message).await {
                warn!("Failed to send ipc message: {err}");
            }
        } else {
            warn!("Failed to process ipc message because of missing transport: {message:?}");
        }
    }

    // Start Moonlight Stream
    async fn start_stream(self: &Arc<Self>, settings: StreamSettings) -> Result<(), anyhow::Error> {
        // We might already be streaming -> remove and wait for connection close firstly
        {
            let mut stream = self.stream.write().await;
            if let Some(stream) = stream.take() {
                spawn_blocking(move || {
                    stream.stop();
                });
            }
        }
        info!("Starting Moonlight stream with settings: {settings}");

        // Send stage
        let mut ipc_sender = self.ipc_sender.clone();
        ipc_sender
            .send(StreamerIpcMessage::WebSocket(
                StreamServerMessage::DebugLog {
                    message: "Moonlight Stream".to_string(),
                    ty: None,
                },
            ))
            .await;

        let mut host = self.info.host.lock().await;

        let video_decoder = StreamVideoDecoder {
            stream: Arc::downgrade(self),
            supported_formats: settings.video_supported_formats,
            stats: Default::default(),
        };

        let audio_decoder = StreamAudioDecoder {
            stream: Arc::downgrade(self),
        };

        let connection_listener = StreamConnectionListener {
            stream: Arc::downgrade(self),
        };

        let stream = match host
            .start_stream(
                &self.moonlight,
                self.info.app_id,
                settings.width,
                settings.height,
                settings.fps,
                settings.hdr,
                true,
                settings.play_audio_local,
                ActiveGamepads::empty(),
                false,
                settings.video_colorspace,
                if settings.video_color_range_full {
                    ColorRange::Full
                } else {
                    ColorRange::Limited
                },
                settings.bitrate,
                settings.packet_size,
                EncryptionFlags::all(),
                connection_listener,
                video_decoder,
                audio_decoder,
            )
            .await
        {
            Ok(value) => value,
            Err(err) => {
                warn!("[Stream]: failed to start moonlight stream: {err}");

                #[allow(clippy::single_match)]
                match err {
                    HostError::Moonlight(MoonlightError::ConnectionAlreadyExists) => {
                        ipc_sender
                            .send(StreamerIpcMessage::WebSocket(
                                StreamServerMessage::DebugLog { message: "Failed to start stream because this streamer is already streaming".to_string(), ty: None },
                            ))
                            .await;
                    }
                    HostError::StreamConfig(StreamConfigError::NotSupportedHdr) => {
                        ipc_sender.send(
                        StreamerIpcMessage::WebSocket(StreamServerMessage::DebugLog {
                            message: "Failed to start stream because this app doesn't support HDR!"
                                .to_string(),
                            ty: Some(LogMessageType::FatalDescription),
                        })).await;
                    }
                    _ => {}
                }

                return Err(err.into());
            }
        };

        let host_features = stream.host_features().unwrap_or_else(|err| {
            warn!("[Stream]: failed to get host features: {err:?}");
            HostFeatures::empty()
        });

        let capabilities = StreamCapabilities {
            touch: host_features.contains(HostFeatures::PEN_TOUCH_EVENTS),
        };

        let (video_setup, audio_setup) = {
            let setup = self.stream_setup.lock().await;

            let video = setup.video.unwrap_or_else(|| {
                warn!("failed to query video setup information. Giving the browser guessed information");
                VideoSetup { format: VideoFormat::H264, width: settings.width, height: settings.height, redraw_rate: settings.fps, flags: 0 }
            });

            let audio = setup.audio.clone().unwrap_or(OpusMultistreamConfig::STEREO);

            (video, audio)
        };

        info!(
            "Stream uses these settings: {:?} with {}x{}x{}",
            video_setup.format, video_setup.width, video_setup.height, video_setup.redraw_rate
        );

        spawn(async move {
            ipc_sender
                .send(StreamerIpcMessage::WebSocket(
                    StreamServerMessage::ConnectionComplete {
                        capabilities,
                        format: video_setup.format as u32,
                        width: video_setup.width,
                        height: video_setup.height,
                        fps: video_setup.redraw_rate,
                        audio_sample_rate: audio_setup.sample_rate,
                        audio_channel_count: audio_setup.channel_count,
                        audio_streams: audio_setup.streams,
                        audio_coupled_streams: audio_setup.coupled_streams,
                        audio_samples_per_frame: audio_setup.samples_per_frame,
                        audio_mapping: audio_setup.mapping,
                    },
                ))
                .await;
        });

        let mut stream_guard = self.stream.write().await;
        stream_guard.replace(stream);

        Ok(())
    }

    // -- Termination
    async fn request_terminate(self: &Arc<Self>) {
        let this = self.clone();

        let mut terminate_request = self.timeout_terminate_request.lock().await;
        *terminate_request = Some(Instant::now());
        drop(terminate_request);

        spawn(async move {
            sleep(TIMEOUT_DURATION + Duration::from_millis(200)).await;

            let now = Instant::now();

            let terminate_request = this.timeout_terminate_request.lock().await;
            if let Some(terminate_request) = *terminate_request
                && (now - terminate_request) > TIMEOUT_DURATION
            {
                info!("Stopping because of timeout");

                this.stop().await;
            }
        });
    }
    async fn clear_terminate_request(&self) {
        let mut request = self.timeout_terminate_request.lock().await;

        *request = None;
    }

    async fn stop(&self) {
        if self
            .is_terminating
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            debug!("[Stream]: stream is already terminating, won't stop twice");
            return;
        }

        debug!("[Stream]: Stopping...");

        {
            let mut stream = self.stream.write().await;
            if let Some(stream) = stream.take() {
                spawn_blocking(move || {
                    stream.stop();
                });
            }
        }

        let mut transport = self.transport_sender.lock().await;
        if let Some(transport) = transport.take() {
            if let Err(err) = transport.close().await {
                warn!("Error whilst closing transport: {err}");
            }
            drop(transport);
        }

        let mut ipc_sender = self.ipc_sender.clone();
        ipc_sender.send(StreamerIpcMessage::Stop).await;

        // Wait for ipc message before stop
        sleep(Duration::from_millis(200)).await;

        // TODO: should we terminate or wait for a new retry?
        info!("Terminating Self");
        self.terminate.notify_waiters();
    }
}

struct StreamConnectionListener {
    stream: Weak<StreamConnection>,
}

impl ConnectionListener for StreamConnectionListener {
    fn stage_starting(&mut self, stage: Stage) {
        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        let mut ipc_sender = stream.ipc_sender.clone();

        stream.runtime.spawn(async move {
            ipc_sender
                .send(StreamerIpcMessage::WebSocket(
                    StreamServerMessage::DebugLog {
                        message: format!("Starting Stage: {}", stage.name()),
                        ty: None,
                    },
                ))
                .await;
        });
    }

    fn stage_complete(&mut self, stage: Stage) {
        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        let mut ipc_sender = stream.ipc_sender.clone();
        ipc_sender.blocking_send(StreamerIpcMessage::WebSocket(
            StreamServerMessage::DebugLog {
                message: format!("Completed Stage: {}", stage.name()),
                ty: None,
            },
        ));
    }

    fn stage_failed(&mut self, stage: Stage, error_code: i32) {
        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        let mut ipc_sender = stream.ipc_sender.clone();
        ipc_sender.blocking_send(StreamerIpcMessage::WebSocket(
            StreamServerMessage::DebugLog {
                message: format!(
                    "Failed Stage: {} with error code {}",
                    stage.name(),
                    error_code
                ),
                ty: Some(LogMessageType::Fatal),
            },
        ));
    }

    fn connection_started(&mut self) {}

    fn connection_terminated(&mut self, error_code: i32) {
        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        let mut ipc_sender = stream.ipc_sender.clone();
        ipc_sender.blocking_send(StreamerIpcMessage::WebSocket(
            StreamServerMessage::ConnectionTerminated { error_code },
        ));

        stream.runtime.clone().block_on(async move {
            stream.stop().await;
        });
    }

    fn log_message(&mut self, message: &str) {
        info!(target: "moonlight", "{}", message.trim());
    }

    fn connection_status_update(&mut self, status: ConnectionStatus) {
        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        stream.clone().runtime.block_on(async move {
            stream
                .try_send_packet(
                    OutboundPacket::General {
                        message: GeneralServerMessage::ConnectionStatusUpdate {
                            status: status.into(),
                        },
                    },
                    "connection status update",
                    true,
                )
                .await
        })
    }

    fn set_hdr_mode(&mut self, hdr_enabled: bool) {
        info!(
            "[HDR] Host called set_hdr_mode with enabled={}",
            hdr_enabled
        );

        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        stream.clone().runtime.block_on(async move {
            info!("[HDR] Sending HdrModeUpdate to client");
            stream
                .try_send_packet(
                    OutboundPacket::General {
                        message: GeneralServerMessage::HdrModeUpdate {
                            enabled: hdr_enabled,
                        },
                    },
                    "hdr mode update",
                    true,
                )
                .await
        })
    }

    fn controller_rumble(
        &mut self,
        controller_number: u16,
        low_frequency_motor: u16,
        high_frequency_motor: u16,
    ) {
        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        stream.runtime.clone().block_on(async move {
            stream
                .try_send_packet(
                    OutboundPacket::ControllerRumble {
                        controller_number: controller_number as u8,
                        low_frequency_motor,
                        high_frequency_motor,
                    },
                    "controller rumble",
                    true,
                )
                .await;
        });
    }

    fn controller_rumble_triggers(
        &mut self,
        controller_number: u16,
        left_trigger_motor: u16,
        right_trigger_motor: u16,
    ) {
        let Some(stream) = self.stream.upgrade() else {
            warn!("Failed to get stream because it is already deallocated");
            return;
        };

        stream.runtime.clone().block_on(async move {
            stream
                .try_send_packet(
                    OutboundPacket::ControllerTriggerRumble {
                        controller_number: controller_number as u8,
                        left_trigger_motor,
                        right_trigger_motor,
                    },
                    "controller rumble triggers",
                    true,
                )
                .await;
        });
    }

    fn controller_set_motion_event_state(
        &mut self,
        _controller_number: u16,
        _motion_type: u8,
        _report_rate_hz: u16,
    ) {
        // unsupported: https://github.com/w3c/gamepad/issues/211
    }

    fn controller_set_adaptive_triggers(
        &mut self,
        _controller_number: u16,
        _event_flags: u8,
        _type_left: u8,
        _type_right: u8,
        _left: &mut u8,
        _right: &mut u8,
    ) {
        // unsupported
    }

    fn controller_set_led(&mut self, _controller_number: u16, _r: u8, _g: u8, _b: u8) {
        // unsupported
    }
}
