//! The record of the last checks, `/var/lib/inayos-guard/status.json`, and the file
//! `/var/lib/inayos-guard/failed` that exists while the last boot update failed.
//!
//! The record holds the last result of each command and, at the top, their sum:
//!
//! ```json
//! {
//!   "time": "2026-11-19T10:00:00Z",
//!   "passed": false,
//!   "failures": ["..."],
//!   "update": { "time": "...", "passed": false, "failures": ["..."], "previous_system_default": true },
//!   "boot": { "time": "...", "passed": true, "failures": [] }
//! }
//! ```
use std::{path::Path, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::{
    Error,
    system::{System, read_text},
    time::rfc3339,
};

pub const STATE_DIR: &str = "/var/lib/inayos-guard";
pub const STATUS_FILE: &str = "/var/lib/inayos-guard/status.json";
pub const FAILED_FILE: &str = "/var/lib/inayos-guard/failed";

/// The result of one command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// When the check ran, in UTC, as `YYYY-MM-DDTHH:MM:SSZ`.
    pub time: String,
    pub passed: bool,
    pub failures: Vec<String>,
    /// For `check-update` only: whether rEFInd now starts the "Previous system" entry by
    /// default. A failure leaves the normal entry the default when no snapshot can start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_system_default: Option<bool>,
}

impl CheckResult {
    pub fn new(time: SystemTime, failures: Vec<String>) -> Self {
        CheckResult {
            time: rfc3339(time),
            passed: failures.is_empty(),
            failures,
            previous_system_default: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// The time of the newest result.
    pub time: String,
    /// Whether every recorded result passed.
    pub passed: bool,
    /// The failures of every recorded result, the boot update's first.
    pub failures: Vec<String>,
    /// The last `check-update`.
    #[serde(default)]
    pub update: Option<CheckResult>,
    /// The last `check-boot`.
    #[serde(default)]
    pub boot: Option<CheckResult>,
}

/// Which command a result comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Update,
    Boot,
}

impl Status {
    /// The status with `result` as the last result of `check`, and the sum updated.
    pub fn with(mut self, check: Check, result: CheckResult) -> Self {
        self.time = result.time.clone();
        match check {
            Check::Update => self.update = Some(result),
            Check::Boot => self.boot = Some(result),
        }
        let results = [&self.update, &self.boot];
        self.passed = results.iter().all(|r| r.as_ref().is_none_or(|r| r.passed));
        self.failures = results
            .iter()
            .filter_map(|r| r.as_ref())
            .flat_map(|r| r.failures.iter().cloned())
            .collect();
        self
    }
}

/// The recorded status, or None when no check has run yet.
pub fn read(system: &dyn System) -> Result<Option<Status>, Error> {
    let path = Path::new(STATUS_FILE);
    if !system.exists(path) {
        return Ok(None);
    }
    let text = read_text(system, path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|source| Error::Status { source })
}

fn write_error(path: &str) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Write {
        path: path.into(),
        source,
    }
}

/// Records `result` as the last result of `check`. A record that cannot be read is
/// replaced. A boot update's result also creates the failed file, or removes it when it
/// passed.
pub fn record(system: &dyn System, check: Check, result: CheckResult) -> Result<Status, Error> {
    let previous = read(system).ok().flatten().unwrap_or_default();
    let status = previous.with(check, result);
    system
        .create_dir_all(Path::new(STATE_DIR))
        .map_err(write_error(STATE_DIR))?;
    let mut json =
        serde_json::to_string_pretty(&status).map_err(|source| Error::Status { source })?;
    json.push('\n');
    system
        .write_atomic(Path::new(STATUS_FILE), json.as_bytes(), 0o644)
        .map_err(write_error(STATUS_FILE))?;
    if check == Check::Update {
        let failed = Path::new(FAILED_FILE);
        if status.update.as_ref().is_some_and(|update| update.passed) {
            system
                .remove_file(failed)
                .map_err(write_error(FAILED_FILE))?;
        } else {
            system
                .write_atomic(failed, b"", 0o644)
                .map_err(write_error(FAILED_FILE))?;
        }
    }
    Ok(status)
}

/// The status in plain words, for `inayos-guard status`.
pub fn describe(status: &Status) -> String {
    let mut text = String::new();
    for (name, result) in [
        ("Last boot update check", &status.update),
        ("Last boot check", &status.boot),
    ] {
        let Some(result) = result else {
            text.push_str(&format!("{name}: has not run\n"));
            continue;
        };
        let verdict = if result.passed { "passed" } else { "failed" };
        text.push_str(&format!("{name}: {verdict} at {}\n", result.time));
        for failure in &result.failures {
            text.push_str(&format!("  - {failure}\n"));
        }
        match result.previous_system_default {
            Some(true) => text.push_str("  rEFInd now starts the Previous system entry by default.\n"),
            Some(false) if !result.passed => text.push_str(
                "  No earlier snapshot can start, so rEFInd still starts the normal entry by default.\n",
            ),
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(time: &str, failures: &[&str]) -> CheckResult {
        CheckResult {
            time: time.to_owned(),
            passed: failures.is_empty(),
            failures: failures.iter().map(|f| (*f).to_owned()).collect(),
            previous_system_default: None,
        }
    }

    #[test]
    fn the_sum_keeps_an_update_failure_after_a_passing_boot_check() {
        let status = Status::default()
            .with(
                Check::Update,
                result("2026-11-19T10:00:00Z", &["bad image"]),
            )
            .with(Check::Boot, result("2026-11-19T10:05:00Z", &[]));
        assert!(!status.passed);
        assert_eq!(status.failures, ["bad image"]);
        assert_eq!(status.time, "2026-11-19T10:05:00Z");
        let status = status.with(Check::Update, result("2026-11-19T11:00:00Z", &[]));
        assert!(status.passed);
        assert!(status.failures.is_empty());
    }

    #[test]
    fn the_record_uses_the_documented_field_names() {
        let status = Status::default().with(Check::Update, result("t", &["x"]));
        let json: serde_json::Value = serde_json::to_value(&status).expect("json");
        assert_eq!(json["passed"], false);
        assert_eq!(json["failures"][0], "x");
        assert_eq!(json["update"]["time"], "t");
        assert!(json["boot"].is_null());
        assert!(json["update"].get("previous_system_default").is_none());
    }

    #[test]
    fn describe_says_which_entry_refind_starts_after_a_failure() {
        let mut fallback = result("t", &["bad image"]);
        fallback.previous_system_default = Some(true);
        let text = describe(&Status::default().with(Check::Update, fallback.clone()));
        assert!(text.contains("  - bad image\n"));
        assert!(text.contains("now starts the Previous system entry"));
        fallback.previous_system_default = Some(false);
        let text = describe(&Status::default().with(Check::Update, fallback));
        assert!(text.contains("still starts the normal entry"));
    }
}
