//! The record of the last checks, `/var/lib/inayos-guard/status.json`, and the file
//! `/var/lib/inayos-guard/failed` that exists while the record holds a failure, which the
//! desktop notification watches.
//!
//! The record holds the last result of each command and, at the top, their sum:
//!
//! ```json
//! {
//!   "time": "2026-11-19T10:00:00Z",
//!   "passed": false,
//!   "failures": ["..."],
//!   "update": { "time": "...", "passed": false, "failures": ["..."], "previous_system_default": true },
//!   "boot": { "time": "...", "passed": true, "failures": [], "tpm": ["..."], "changes": ["..."] },
//!   "firmware": { "time": "...", "passed": true, "failures": [], "tpm": ["..."], "changes": ["..."], "boot_id": "..." }
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
    /// On an encrypted system, for `check-boot` and `prepare-firmware-update`: the TPM2
    /// slots of each LUKS volume after the command, one line per volume.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tpm: Vec<String>,
    /// What the command changed on the LUKS volumes, one line per change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<String>,
    /// What the command left alone on purpose, and why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// For `prepare-firmware-update` only: the kernel's boot id, from
    /// `/proc/sys/kernel/random/boot_id`, of the boot it ran in. `check-boot` keeps the
    /// temporary slots of a preparation made during the same boot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot_id: Option<String>,
}

impl CheckResult {
    pub fn new(time: SystemTime, failures: Vec<String>) -> Self {
        CheckResult {
            time: rfc3339(time),
            passed: failures.is_empty(),
            failures,
            previous_system_default: None,
            tpm: Vec::new(),
            changes: Vec::new(),
            notes: Vec::new(),
            boot_id: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// The time of the newest result.
    pub time: String,
    /// Whether every recorded result passed.
    pub passed: bool,
    /// The failures of every recorded result: the boot update's, the boot check's, then
    /// the firmware update preparation's.
    pub failures: Vec<String>,
    /// The last `check-update`.
    #[serde(default)]
    pub update: Option<CheckResult>,
    /// The last `check-boot`.
    #[serde(default)]
    pub boot: Option<CheckResult>,
    /// The last `prepare-firmware-update` on an encrypted system, until a normal boot after
    /// it binds the volumes again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware: Option<CheckResult>,
}

/// Which command a result comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    Update,
    Boot,
    Firmware,
}

impl Status {
    /// The status with `result` as the last result of `check`, and the sum updated.
    pub fn with(mut self, check: Check, result: CheckResult) -> Self {
        self.time = result.time.clone();
        match check {
            Check::Update => self.update = Some(result),
            Check::Boot => self.boot = Some(result),
            Check::Firmware => self.firmware = Some(result),
        }
        self.sum()
    }

    /// The status without the firmware update preparation, once a later boot has used or
    /// replaced its temporary slots.
    pub fn without_firmware(mut self) -> Self {
        self.firmware = None;
        self.sum()
    }

    fn sum(mut self) -> Self {
        let results = [&self.update, &self.boot, &self.firmware];
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
/// replaced.
pub fn record(system: &dyn System, check: Check, result: CheckResult) -> Result<Status, Error> {
    record_with(system, |status| status.with(check, result))
}

/// Records the status `change` makes of the recorded one, then creates the failed file
/// when the new status holds a failure, or removes it when everything passed.
pub fn record_with(
    system: &dyn System,
    change: impl FnOnce(Status) -> Status,
) -> Result<Status, Error> {
    let previous = read(system).ok().flatten().unwrap_or_default();
    let status = change(previous);
    system
        .create_dir_all(Path::new(STATE_DIR))
        .map_err(write_error(STATE_DIR))?;
    let mut json =
        serde_json::to_string_pretty(&status).map_err(|source| Error::Status { source })?;
    json.push('\n');
    system
        .write_atomic(Path::new(STATUS_FILE), json.as_bytes(), 0o644)
        .map_err(write_error(STATUS_FILE))?;
    let failed = Path::new(FAILED_FILE);
    if status.passed {
        system
            .remove_file(failed)
            .map_err(write_error(FAILED_FILE))?;
    } else {
        system
            .write_atomic(failed, b"", 0o644)
            .map_err(write_error(FAILED_FILE))?;
    }
    Ok(status)
}

/// The status in plain words, for `inayos-guard status`.
pub fn describe(status: &Status) -> String {
    let mut text = String::new();
    // Only an encrypted system prepares firmware updates, so that line shows once it ran.
    for (name, result, always) in [
        ("Last boot update check", &status.update, true),
        ("Last boot check", &status.boot, true),
        ("Last firmware update preparation", &status.firmware, false),
    ] {
        let Some(result) = result else {
            if always {
                text.push_str(&format!("{name}: has not run\n"));
            }
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
        for change in &result.changes {
            text.push_str(&format!("  Changed: {change}\n"));
        }
        for note in &result.notes {
            text.push_str(&format!("  Note: {note}\n"));
        }
        for volume in &result.tpm {
            text.push_str(&format!("  TPM: {volume}\n"));
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
            tpm: Vec::new(),
            changes: Vec::new(),
            notes: Vec::new(),
            boot_id: None,
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
        assert!(json["update"].get("tpm").is_none());
        assert!(json.get("firmware").is_none());
    }

    #[test]
    fn describe_lists_the_tpm_changes_and_state_and_skips_a_firmware_step_that_never_ran() {
        let mut boot = result("t", &[]);
        boot.changes = vec!["system: enrolled TPM2 slot 2 bound to PCR 7".to_owned()];
        boot.tpm = vec!["system: TPM2 slot 2 bound to PCR 7".to_owned()];
        let text = describe(&Status::default().with(Check::Boot, boot));
        assert!(text.contains("  Changed: system: enrolled TPM2 slot 2 bound to PCR 7\n"));
        assert!(text.contains("  TPM: system: TPM2 slot 2 bound to PCR 7\n"));
        assert!(!text.contains("firmware"));
        let text = describe(&Status::default().with(Check::Firmware, result("t", &[])));
        assert!(text.contains("Last firmware update preparation: passed at t\n"));
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

    #[test]
    fn the_failed_file_follows_every_result_and_dropping_the_firmware_result_clears_it() {
        let system = crate::fake::FakeSystem::default();
        record(&system, Check::Boot, result("t", &["no TPM2 slot opens"])).expect("written");
        assert!(system.exists(Path::new(FAILED_FILE)));
        record(&system, Check::Boot, result("t", &[])).expect("written");
        assert!(!system.exists(Path::new(FAILED_FILE)));
        record(&system, Check::Firmware, result("t", &["TPM busy"])).expect("written");
        assert!(system.exists(Path::new(FAILED_FILE)));
        let status = record_with(&system, Status::without_firmware).expect("written");
        assert!(status.passed);
        assert!(status.firmware.is_none());
        assert!(!system.exists(Path::new(FAILED_FILE)));
    }
}
