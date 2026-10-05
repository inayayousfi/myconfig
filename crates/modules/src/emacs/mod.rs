//! The Emacs workbench config.
mod copied;
mod stowed;

pub use copied::EmacsCopied;
pub use stowed::EmacsStowed;

pub struct EmacsOptions {
    /// Allows the plaintext browser terminal on ports 18080 and 18081 from private networks.
    pub browser_terminal_firewall: bool,
}
