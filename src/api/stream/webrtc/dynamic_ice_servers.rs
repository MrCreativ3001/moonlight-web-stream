use common::config::{RtcIceServer, WebRtcConfig};
use log::error;
use tokio::process::Command;
use tracing::debug;

pub async fn load_dynamic_ice_servers(config: &WebRtcConfig) -> Vec<RtcIceServer> {
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
