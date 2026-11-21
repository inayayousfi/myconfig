//! Where the install reports what it does. The command line prints to the terminal and a log
//! file; a graphical interface receives the same lines through a `ChannelSink`.
use std::sync::mpsc::Sender;

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

/// One reported line, tagged with the `Sink` method that received it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Step(String),
    Command(String),
    Output(String),
    Detail(String),
}

/// A sink that sends each line over a channel, so an interface can run the install on another
/// thread and show its lines as they come. Once the receiver is gone, lines are dropped.
pub struct ChannelSink {
    sender: Sender<Line>,
}

impl ChannelSink {
    pub fn new(sender: Sender<Line>) -> Self {
        ChannelSink { sender }
    }

    fn send(&self, line: Line) {
        // A closed receiver means nobody shows the lines any more; the install goes on.
        let _ = self.sender.send(line);
    }
}

impl Sink for ChannelSink {
    fn step(&self, text: &str) {
        self.send(Line::Step(text.to_owned()));
    }
    fn command(&self, command: &str) {
        self.send(Line::Command(command.to_owned()));
    }
    fn output(&self, line: &str) {
        self.send(Line::Output(line.to_owned()));
    }
    fn detail(&self, text: &str) {
        self.send(Line::Detail(text.to_owned()));
    }
}

/// A sink that drops every line, for code that only runs queries nobody needs to see.
pub(crate) struct Silent;

impl Sink for Silent {
    fn step(&self, _: &str) {}
    fn command(&self, _: &str) {}
    fn output(&self, _: &str) {}
    fn detail(&self, _: &str) {}
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread};

    use super::{ChannelSink, Line, Sink};
    use crate::{
        host::{Command, RealHost},
        install::machine::Machine,
    };

    #[test]
    fn an_install_thread_delivers_its_lines_in_order_to_the_receiver() {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let sink = ChannelSink::new(sender);
            sink.step("Checking");
            let m = Machine {
                host: &RealHost,
                sink: &sink,
            };
            m.run(Command::action("echo").arg("streamed")).unwrap();
        })
        .join()
        .unwrap();
        assert_eq!(
            receiver.iter().collect::<Vec<_>>(),
            [
                Line::Step("Checking".into()),
                Line::Command("echo streamed".into()),
                Line::Output("streamed".into()),
            ]
        );
    }

    #[test]
    fn a_closed_receiver_does_not_stop_the_sender() {
        let (sender, receiver) = mpsc::channel();
        drop(receiver);
        ChannelSink::new(sender).detail("nobody reads this");
    }
}
