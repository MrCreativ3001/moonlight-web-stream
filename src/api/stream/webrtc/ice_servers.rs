use crate::{api::bindings::RtcIceServer, config::WebRtcConfig};
use log::error;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tracing::debug;

use crate::app::{App, AppError};

pub async fn generate_ice_servers(app: &App) -> Result<Vec<RtcIceServer>, AppError> {
    // Load ice servers
    let mut ice_servers = app.config().webrtc.ice_servers.clone();

    // Load dynamic ice servers and append them to the current ice servers
    let dynamic_ice_servers = if app.config().webrtc.ice_server_script.is_some() {
        let mut cache = app.inner.ice_server_script_cache.lock().await;
        if let Some((loaded_at, ice_servers)) = cache.as_ref()
            && loaded_at.elapsed() < Duration::from_secs(60)
        {
            ice_servers.clone()
        } else {
            let ice_servers = load_dynamic_ice_servers(&app.config().webrtc).await;
            *cache = Some((Instant::now(), ice_servers.clone()));
            ice_servers
        }
    } else {
        load_dynamic_ice_servers(&app.config().webrtc).await
    };
    ice_servers.extend_from_slice(&dynamic_ice_servers);

    Ok(ice_servers)
}

async fn load_dynamic_ice_servers(config: &WebRtcConfig) -> Vec<RtcIceServer> {
    let Some(script_command) = config.ice_server_script.as_ref() else {
        debug!("No WebRTC ice server script found");
        return vec![];
    };

    debug!(script = script_command, "running WebRTC ice server script");

    let mut script = Command::new(script_command);

    let output = match script.output().await {
        Ok(value) => value,
        Err(err) => {
            error!("failed to run WebRTC ice server script: {err}");
            return vec![];
        }
    };

    if !matches!(output.status.code(), None | Some(0)) {
        error!(
            "WebRTC ice server script has a non zero exit code: {}",
            output.status
        );

        if let Ok(error) = String::from_utf8(output.stdout) {
            error!("WebRTC ice server script error:\n{error}");
        }
        return vec![];
    }

    let json: Vec<RtcIceServer> = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(err) => {
            error!("failed to deserialize WebRTC ice server script output: {err}");
            return vec![];
        }
    };

    json
}
