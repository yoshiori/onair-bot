//! Runtime configuration loaded from environment variables (and `.env`).

use std::env;
use std::time::Duration;

const DEFAULT_POLL_INTERVAL_SECS: u64 = 1;
const DEFAULT_DEBOUNCE: u32 = 2;
const DEFAULT_IGNORE_PROCESSES: &str = "pipewire,wireplumber";

/// Fully validated configuration for the daemon.
#[derive(Debug, Clone)]
pub struct Config {
    /// SwitchBot API open token.
    pub token: String,
    /// SwitchBot API secret key (used to sign requests).
    pub secret: String,
    /// Target SwitchBot device id to switch on/off.
    pub device_id: String,
    /// How often to poll for camera usage.
    pub poll_interval: Duration,
    /// Consecutive identical readings required to confirm a state change.
    pub debounce: u32,
    /// Process name prefixes whose open camera handles are ignored.
    pub ignore_processes: Vec<String>,
}

impl Config {
    /// Build the configuration from the process environment.
    pub fn from_env() -> Result<Config, String> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// Build the configuration from an arbitrary key lookup.
    ///
    /// Kept separate from [`Config::from_env`] so the parsing logic can be
    /// unit tested without mutating the global process environment.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let token = required(&get, "TOKEN")?;
        let secret = required(&get, "SECRET")?;
        let device_id = required(&get, "DEVICE_ID")?;

        let poll_secs =
            parse_or(&get, "ONAIR_POLL_INTERVAL_SECS", DEFAULT_POLL_INTERVAL_SECS)?.max(1);
        let debounce = parse_or(&get, "ONAIR_DEBOUNCE", DEFAULT_DEBOUNCE)?.max(1);

        let ignore_processes = get("ONAIR_IGNORE_PROCESSES")
            .unwrap_or_else(|| DEFAULT_IGNORE_PROCESSES.to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(Config {
            token,
            secret,
            device_id,
            poll_interval: Duration::from_secs(poll_secs),
            debounce,
            ignore_processes,
        })
    }
}

fn required(get: &impl Fn(&str) -> Option<String>, key: &str) -> Result<String, String> {
    match get(key) {
        Some(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!(
            "environment variable {key} is required (set it in .env)"
        )),
    }
}

fn parse_or<T>(get: &impl Fn(&str) -> Option<String>, key: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
{
    match get(key) {
        Some(raw) => raw
            .trim()
            .parse::<T>()
            .map_err(|_| format!("environment variable {key} has an invalid value: {raw}")),
        None => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn loads_required_values_and_applies_defaults() {
        let config = Config::from_lookup(lookup(&[
            ("TOKEN", "tok"),
            ("SECRET", "sec"),
            ("DEVICE_ID", "dev"),
        ]))
        .unwrap();

        assert_eq!(config.token, "tok");
        assert_eq!(config.secret, "sec");
        assert_eq!(config.device_id, "dev");
        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.debounce, 2);
        assert_eq!(config.ignore_processes, ["pipewire", "wireplumber"]);
    }

    #[test]
    fn missing_required_value_is_an_error() {
        let err = Config::from_lookup(lookup(&[("TOKEN", "tok"), ("SECRET", "sec")])).unwrap_err();
        assert!(err.contains("DEVICE_ID"), "unexpected error: {err}");
    }

    #[test]
    fn blank_required_value_is_an_error() {
        let err = Config::from_lookup(lookup(&[
            ("TOKEN", "tok"),
            ("SECRET", "   "),
            ("DEVICE_ID", "dev"),
        ]))
        .unwrap_err();
        assert!(err.contains("SECRET"), "unexpected error: {err}");
    }

    #[test]
    fn optional_values_override_defaults() {
        let config = Config::from_lookup(lookup(&[
            ("TOKEN", "tok"),
            ("SECRET", "sec"),
            ("DEVICE_ID", "dev"),
            ("ONAIR_POLL_INTERVAL_SECS", "5"),
            ("ONAIR_DEBOUNCE", "4"),
            ("ONAIR_IGNORE_PROCESSES", "pipewire, obs , "),
        ]))
        .unwrap();

        assert_eq!(config.poll_interval, Duration::from_secs(5));
        assert_eq!(config.debounce, 4);
        assert_eq!(config.ignore_processes, ["pipewire", "obs"]);
    }

    #[test]
    fn zero_values_are_clamped_to_one() {
        let config = Config::from_lookup(lookup(&[
            ("TOKEN", "tok"),
            ("SECRET", "sec"),
            ("DEVICE_ID", "dev"),
            ("ONAIR_POLL_INTERVAL_SECS", "0"),
            ("ONAIR_DEBOUNCE", "0"),
        ]))
        .unwrap();

        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.debounce, 1);
    }

    #[test]
    fn invalid_numeric_value_is_an_error() {
        let err = Config::from_lookup(lookup(&[
            ("TOKEN", "tok"),
            ("SECRET", "sec"),
            ("DEVICE_ID", "dev"),
            ("ONAIR_DEBOUNCE", "soon"),
        ]))
        .unwrap_err();
        assert!(err.contains("ONAIR_DEBOUNCE"), "unexpected error: {err}");
    }
}
