use std::io::Write;
use std::time::{Duration, Instant};

use cardano_ouroboros_network::mux;
use futures::executor::block_on;
use log::debug;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PingSuccess {
    status: String,
    host: String,
    port: u16,
    connect_duration_ms: u128,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PingError {
    status: String,
    host: String,
    port: u16,
    error_message: String,
}

pub fn ping<W: Write>(out: &mut W, host: &str, port: u16, network_magic: u32) {
    block_on(async {
        let start = Instant::now();
        match mux::connection::connect(host, port).await {
            Ok(channel) => {
                let connect_duration = start.elapsed();
                match channel.handshake(network_magic).await {
                    Ok(data) => {
                        let total_duration = start.elapsed();
                        debug!("{}", data);
                        ping_json_success(out, connect_duration, total_duration, host, port);
                    }
                    Err(error) => {
                        ping_json_error(out, error, host, port);
                    }
                }
            }
            Err(error) => {
                ping_json_error(out, format!("{}", error), host, port);
            }
        }
    });
}

fn ping_json_success<W: Write>(
    out: &mut W,
    connect_duration: Duration,
    total_duration: Duration,
    host: &str,
    port: u16,
) {
    serde_json::ser::to_writer_pretty(
        out,
        &PingSuccess {
            status: "ok".to_string(),
            host: host.to_string(),
            port,
            connect_duration_ms: connect_duration.as_millis(),
            duration_ms: total_duration.as_millis(),
        },
    )
    .unwrap();
}

fn ping_json_error<W: Write>(out: &mut W, message: String, host: &str, port: u16) {
    serde_json::ser::to_writer_pretty(
        out,
        &PingError {
            status: "error".to_string(),
            host: host.to_string(),
            port,
            error_message: message,
        },
    )
    .unwrap();
}
