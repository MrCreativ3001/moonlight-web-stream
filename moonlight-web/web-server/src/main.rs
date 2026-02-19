use common::config::Config;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use std::{fs::OpenOptions, io::ErrorKind, path::PathBuf, str::FromStr};
use tokio::fs::{self};
use tracing::level_filters::LevelFilter;
use tracing_actix_web::TracingLogger;
use tracing_appender::non_blocking;
use tracing_subscriber::{
    EnvFilter, Registry,
    fmt::{self},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

use actix_web::{
    App as ActixApp, HttpServer,
    middleware::{self},
    web::{Data, scope},
};
use log::{error, info};

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
mod human_json;

#[actix_web::main]
async fn main() {
    let cli = Cli::load();

    // Load Config
    let config_path = PathBuf::from_str(&cli.config_path).expect("invalid config file path");
    let config = match fs::read_to_string(&config_path).await {
        Ok(mut value) => {
            value = preprocess_human_json(value);

            let mut config = serde_json::from_str(&value).expect("invalid file");
            cli.options.apply(&mut config);
            config
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            let mut new_config = Config::default();
            cli.options.apply(&mut new_config);

            let value_str =
                serde_json::to_string_pretty(&new_config).expect("failed to serialize file");

            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)
                    .await
                    .expect("failed to create directories to file");
            }
            fs::write(config_path, value_str)
                .await
                .expect("failed to write default file");

            new_config
        }
        Err(err) => panic!("failed to read file: {err}"),
    };

    match cli.command {
        Some(Command::PrintConfig) => {
            let json =
                serde_json::to_string_pretty(&config).expect("failed to serialize config to json");
            println!("{json}");
            return;
        }
        None | Some(Command::Run) => {
            // Fallthrough
        }
    }

    let guard = init_log(&config);

    if let Err(err) = start(config).await {
        error!("{err:?}");
    }

    drop(guard);
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
            "mio::poll=off"
                .parse()
                .expect("failed to add mio tracing directive"),
        );

    let stdout_layer = fmt::layer().with_target(false);

    let (file_layer, guard) = if let Some(log_file) = &config.log.file_path {
        let file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(log_file)
            .expect("failed to open log file");

        let (writer, guard) = non_blocking(file);

        let fmt_layer = fmt::layer().with_writer(writer).with_ansi(false);

        (Some(fmt_layer), Some(guard))
    } else {
        (None, None)
    };

    Registry::default()
        .with(env_filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    guard
}

async fn start(config: Config) -> Result<(), anyhow::Error> {
    let app = App::new(config.clone()).await?;
    let app = Data::new(app);

    let bind_address = app.config().web_server.bind_address;
    let server = HttpServer::new({
        let url_path_prefix = config.web_server.url_path_prefix.clone();
        let app = app.clone();

        move || {
            ActixApp::new().wrap(TracingLogger::default()).service(
                scope(&url_path_prefix)
                    .app_data(app.clone())
                    .wrap(
                        // TODO: maybe only re cache when required?
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

    if let Some(certificate) = app.config().web_server.certificate.as_ref() {
        info!("[Server]: Running Https Server with ssl tls");

        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls())
            .expect("failed to create ssl tls acceptor");
        builder
            .set_private_key_file(&certificate.private_key_pem, SslFiletype::PEM)
            .expect("failed to set private key");
        builder
            .set_certificate_chain_file(&certificate.certificate_pem)
            .expect("failed to set certificate");

        server.bind_openssl(bind_address, builder)?.run().await?;
    } else {
        server.bind(bind_address)?.run().await?;
    }

    Ok(())
}
