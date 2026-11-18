//! Where the install reports what it does. The command line prints to the terminal and a log
//! file; a graphical interface can show the same lines in its own way.

/// Receives every line the install reports, in order. Lines of a streamed command arrive from
/// the threads that read its output, hence `Sync`.
pub trait Sink: Sync {
    /// A step of the install starts, such as "Partitioning /dev/vda".
    fn step(&self, text: &str);
    /// A command that changes the machine is about to run. Secrets show as `<hidden>`.
    fn command(&self, command: &str);
    /// A line printed by a command that changes the machine.
    fn output(&self, line: &str);
    /// A detail for the log only: a query and its output, or a file the install changed.
    fn detail(&self, text: &str);
}
