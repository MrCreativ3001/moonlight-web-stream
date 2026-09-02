use crate::{cli::ConfigCommand, config::Config};
use futures::{
    channel::oneshot,
    future::{Either, select},
};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use std::{
    fs::OpenOptions,
    io::{self, IsTerminal},
    path::PathBuf,
    str::FromStr,
};
use tokio::fs::{self};
use tracing::{Level, Span, level_filters::LevelFilter, span};
use tracing_actix_web::{RootSpanBuilder, TracingLogger};
use tracing_appender::non_blocking;
use tracing_subscriber::{
    EnvFilter, Registry,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};
use venator::Venator;

use actix_web::{
    App as ActixApp, HttpServer,
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    http::header::HeaderMap,
    middleware::{self},
    web::{Data, scope},
};
use tracing::{error, info, trace};

use crate::{
    api::api_service,
    app::App,
    cli::{Cli, Command},
    human_json::preprocess_human_json,
    web::{web_config_js_service, web_service},
};

mod api;
mod app;
mod web;

mod cli;
mod config;
mod human_json;

#[cfg(not(windows))]
#[actix_web::main]
async fn main() {
    if let Err(err) = run_application(None).await {
        eprintln!("web-server failed: {err:?}");
    }
}

#[cfg(windows)]
fn main() {
    let service_mode = std::env::args_os()
        .skip(1)
        .any(|argument| argument == std::ffi::OsStr::new("--service"));

    if service_mode {
        if let Err(err) = windows_service::service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        {
            eprintln!("failed to start Windows service dispatcher: {err:?}");
        }
        return;
    }

    if let Err(err) = actix_web::rt::System::new().block_on(run_application(None)) {
        eprintln!("web-server failed: {err:?}");
    }
}

async fn run_application(shutdown: Option<oneshot::Receiver<()>>) -> Result<(), anyhow::Error> {
    let cli = Cli::load();

    // Load Config
    let config_path = PathBuf::from_str(&cli.config_path).expect("invalid config file path");
    let mut config = match fs::read_to_string(&config_path).await {
        Ok(mut value) => {
            value = preprocess_human_json(value);

            serde_json::from_str(&value).expect("invalid file")
        }
        Err(err) if matches!(err.kind(), io::ErrorKind::NotFound) => Config::default(),
        Err(err) => {
            panic!("failed to read config: {err}");
        }
    };
    cli.options.apply(&mut config);

    match cli.command {
        Some(Command::Config(ConfigCommand::Print)) => {
            let json =
                serde_json::to_string_pretty(&config).expect("failed to serialize config to json");
            println!("{json}");
            return Ok(());
        }
        Some(Command::Config(ConfigCommand::Generate)) => {
            let value_str =
                serde_json::to_string_pretty(&config).expect("failed to serialize file");

            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .expect("failed to create directories to file");
            }
            fs::write(&config_path, value_str)
                .await
                .expect("failed to write default file");

            println!("Successfully generate config at {config_path:?}");
            return Ok(());
        }
        None | Some(Command::Run) => {
            // Fallthrough
        }
    }

    let guard = init_log(&config);

    // Initialize crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to set ring crypto provider as default");

    // Start the server
    let result = start(config, shutdown).await;
    if let Err(err) = &result {
        error!("{err:?}");
    }

    drop(guard);
    result
}

fn init_log(config: &Config) -> Option<non_blocking::WorkerGuard> {
    let config_level_filter = match config.log.level_filter {
        log::LevelFilter::Off => LevelFilter::OFF,
        log::LevelFilter::Error => LevelFilter::ERROR,
        log::LevelFilter::Info => LevelFilter::INFO,
        log::LevelFilter::Warn => LevelFilter::WARN,
        log::LevelFilter::Debug => LevelFilter::DEBUG,
        log::LevelFilter::Trace => LevelFilter::TRACE,
    };

    let env_filter = EnvFilter::builder()
        .with_default_directive(config_level_filter.into())
        .from_env_lossy()
        // Add default directives
        .add_directive(
            "actix_http::h1=off"
                .parse()
                .expect("failed to add actix-web tracing directive"),
        )
        .add_directive(
            "h2=off"
                .parse()
                .expect("failed to add h2 tracing directive"),
        )
        .add_directive(
            "mio::poll=off"
                .parse()
                .expect("failed to add mio tracing directive"),
        )
        .add_directive(
            "webrtc_sctp=off"
                .parse()
                .expect("failed to add rtc tracing directive"),
        );

    #[cfg(windows)]
    enable_ansi_windows();

    let stdout_layer = fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_ansi(io::stdout().is_terminal());

    let (file_layer, guard) = if let Some(log_file) = &config.log.file_path {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(log_file)
            .expect("failed to open log file");

        let (writer, guard) = non_blocking(file);

        let fmt_layer = fmt::layer()
            .with_span_events(FmtSpan::ACTIVE)
            .with_writer(writer)
            .with_ansi(false);

        (Some(fmt_layer), Some(guard))
    } else {
        (None, None)
    };

    let venator = config.log.dev_venator.then(Venator::default);

    Registry::default()
        .with(venator)
        .with(env_filter.clone())
        .with(file_layer)
        .with(stdout_layer)
        .init();

    trace!("Using env_filter: {env_filter}");

    guard
}

#[cfg(windows)]
fn enable_ansi_windows() {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, SetConsoleMode,
    };

    unsafe {
        let handle = io::stdout().as_raw_handle();
        let mut mode = 0;
        if GetConsoleMode(handle as _, &mut mode) != 0 {
            SetConsoleMode(handle as _, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

struct ActixDebugSpan;

impl ActixDebugSpan {
    fn sanitize_headers(headers: &HeaderMap) -> Vec<(String, String)> {
        const SENSITIVE: &[&str] = &["authorization", "cookie", "set-cookie"];

        headers
            .iter()
            .map(|(name, value)| {
                let name_str = name.as_str().to_string();

                let value_str = if SENSITIVE.contains(&name_str.to_ascii_lowercase().as_str()) {
                    "<redacted>".to_string()
                } else {
                    value.to_str().unwrap_or("<binary>").to_string()
                };

                (name_str, value_str)
            })
            .collect()
    }
}

impl RootSpanBuilder for ActixDebugSpan {
    fn on_request_start(request: &ServiceRequest) -> Span {
        if tracing::enabled!(Level::TRACE) {
            span!(
                Level::TRACE,
                "http_request",
                method = %request.method(),
                uri = %request.uri(),
                headers = ?Self::sanitize_headers(request.headers()),
                peer_addr = ?request.peer_addr(),
            )
        } else {
            span!(
                Level::DEBUG,
                "http_request",
                method = %request.method(),
                uri = %request.uri(),
            )
        }
    }
    fn on_request_end<B: MessageBody>(
        _span: Span,
        _outcome: &Result<ServiceResponse<B>, actix_web::Error>,
    ) {
    }
}

async fn start(
    config: Config,
    shutdown: Option<oneshot::Receiver<()>>,
) -> Result<(), anyhow::Error> {
    let app = App::new(config.clone()).await?;
    let app = Data::new(app);

    let bind_address = app.config().web_server.bind_address;
    let server = HttpServer::new({
        let url_path_prefix = config.web_server.url_path_prefix.clone();
        let app = app.clone();

        move || {
            ActixApp::new()
                .wrap(TracingLogger::<ActixDebugSpan>::new())
                .service(
                    scope(&url_path_prefix)
                        .app_data(app.clone())
                        .wrap(
                            middleware::DefaultHeaders::new()
                                .add((
                                    "Cache-Control",
                                    "no-store, no-cache, must-revalidate, private",
                                ))
                                .add(("Pragma", "no-cache"))
                                .add(("Expires", "0")),
                        )
                        .service(api_service())
                        .service(web_config_js_service())
                        .service(web_service()),
                )
        }
    });

    let server = if let Some(certificate) = app.config().web_server.certificate.as_ref() {
        info!("[Server]: Running Https Server with ssl tls");

        let certificate_chain = {
            let results =
                CertificateDer::pem_file_iter(&certificate.certificate_pem)?.collect::<Vec<_>>();
            let mut chain = Vec::with_capacity(results.len());

            for result in results {
                chain.push(result?);
            }

            chain
        };
        let private_key = PrivateKeyDer::from_pem_file(&certificate.private_key_pem)?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificate_chain, private_key)?;

        server.bind_rustls_0_23(bind_address, config)?.run()
    } else {
        server.bind(bind_address)?.run()
    };

    let server_handle = server.handle();
    match shutdown {
        Some(shutdown) => match select(Box::pin(server), Box::pin(shutdown)).await {
            Either::Left((result, _)) => result?,
            Either::Right((_, _server)) => {
                // A service stop must not wait for a WebRTC session to drain naturally.
                server_handle.stop(false).await;
            }
        },
        None => server.await?,
    }

    Ok(())
}

#[cfg(windows)]
const SERVICE_NAME: &str = "MoonlightWebStream";

#[cfg(windows)]
windows_service::define_windows_service!(ffi_service_main, windows_service_main);

#[cfg(windows)]
fn windows_service_main(_arguments: Vec<std::ffi::OsString>) {
    use std::{
        env,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use windows_service::{
        service::{
            ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
            ServiceType,
        },
        service_control_handler::{self, ServiceControlHandlerResult},
    };

    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let shutdown_sender = Arc::new(Mutex::new(Some(shutdown_sender)));
    let control_sender = shutdown_sender.clone();
    let event_handler = move |control_event| match control_event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            if let Ok(mut sender) = control_sender.lock() {
                if let Some(sender) = sender.take() {
                    let _ = sender.send(());
                }
            }
            // Windows SCM must release the executable before an installer can replace it.
            // Actix may retain stream workers indefinitely, so do not wait for them here.
            std::process::exit(0);
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };

    let status_handle = match service_control_handler::register(SERVICE_NAME, event_handler) {
        Ok(handle) => handle,
        Err(err) => {
            eprintln!("failed to register Windows service control handler: {err:?}");
            return;
        }
    };

    let running_status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    if let Err(err) = status_handle.set_service_status(running_status) {
        eprintln!("failed to report Windows service status: {err:?}");
        return;
    }

    let result = (|| -> Result<(), anyhow::Error> {
        let executable_directory = env::current_exe()?
            .parent()
            .expect("executable does not have a parent directory")
            .to_path_buf();
        env::set_current_dir(executable_directory)?;

        actix_web::rt::System::new().block_on(run_application(Some(shutdown_receiver)))
    })();

    if let Err(err) = &result {
        eprintln!("Windows service failed: {err:?}");
    }

    let stopped_status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: if result.is_ok() {
            ServiceExitCode::Win32(0)
        } else {
            ServiceExitCode::Win32(1)
        },
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    if let Err(err) = status_handle.set_service_status(stopped_status) {
        eprintln!("failed to report Windows service stop status: {err:?}");
    }
}
