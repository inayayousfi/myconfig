//! The InayOS installer core: reads an answers file and installs InayOS from the live USB
//! system. It has no interface: every line it reports goes to a `Sink`, and every command
//! and file change goes through a `Host`.
#![cfg(unix)]
pub mod answers;
pub mod disk_names;
mod error;
pub mod host;
mod install;
pub mod parse;
mod sink;
pub mod sizes;

pub use answers::{Answers, AnswersError, Disk};
pub use error::{Error, Failure, Stage};
pub use host::{Host, RealHost};
pub use install::{INSTALLED_LOG, Settings, install};
pub use sink::Sink;

#[cfg(test)]
mod fake;
