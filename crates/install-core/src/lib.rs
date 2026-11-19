//! The InayOS installer core: reads an answers file and installs InayOS from the live USB
//! system. It has no interface: every line it reports goes to a `Sink`, and every command
//! and file change goes through a `Host`.
#![cfg(unix)]
pub mod answers;
pub mod catalog;
pub mod disk_names;
mod error;
mod hibernation;
pub mod host;
mod install;
pub mod inventory;
pub mod modules;
pub mod parse;
mod sink;
pub mod sizes;
pub mod windows;

pub use answers::{Answers, AnswersError, Choice, Disk, check_hostname, check_username};
pub use error::{Error, Failure, Stage};
pub use host::{Host, RealHost};
pub use install::{INSTALLED_LOG, Installed, RecoveryKey, Settings, install};
pub use sink::{ChannelSink, Line, Sink};

#[cfg(test)]
mod fake;
