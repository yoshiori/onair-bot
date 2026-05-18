//! onair-bot: watch the camera and switch an on-air light accordingly.
//!
//! The daemon polls camera usage, and on every confirmed change drives a
//! SwitchBot device on or off. On startup it syncs the light to the current
//! camera state so a restart mid-meeting cannot leave the light wrong.

mod camera;
mod config;
mod switchbot;

use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;

use camera::CameraWatcher;
use config::Config;
use switchbot::SwitchBot;

/// Linux exposes every process under `/proc`.
const PROC_ROOT: &str = "/proc";

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    dotenvy::dotenv().ok();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            log::error!("configuration error: {error}");
            return ExitCode::FAILURE;
        }
    };

    match run(config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            log::error!("fatal: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(config: Config) -> Result<(), String> {
    let switchbot = SwitchBot::new(config.token, config.secret, config.device_id);

    let mut watcher = CameraWatcher::new(
        PathBuf::from(PROC_ROOT),
        config.ignore_processes,
        config.debounce,
    )
    .map_err(|error| format!("could not read {PROC_ROOT}: {error}"))?;

    let initial = watcher.state();
    log::info!("startup: camera is {}, syncing light", on_off(initial));
    apply(&switchbot, initial);

    loop {
        thread::sleep(config.poll_interval);
        if let Some(in_use) = watcher.poll() {
            log::info!("camera turned {}", on_off(in_use));
            apply(&switchbot, in_use);
        }
    }
}

/// Drive the light to match the camera state.
///
/// A failed command is logged but not fatal: the daemon keeps running and
/// will correct the light on the next transition.
fn apply(switchbot: &SwitchBot, in_use: bool) {
    let result = if in_use {
        switchbot.turn_on()
    } else {
        switchbot.turn_off()
    };
    if let Err(error) = result {
        log::error!("{error}");
    }
}

fn on_off(on: bool) -> &'static str {
    if on {
        "ON"
    } else {
        "OFF"
    }
}
