//! The plain command line: output and questions in the normal terminal.
use std::{
    io::{BufRead, IsTerminal, Write},
    process::{Command, ExitStatus},
};

use inayos_modules::{Event, Interaction, Step, Unanswered};

pub struct Plain;

fn verb(step: Step) -> &'static str {
    match step {
        Step::Install => "Installing",
        Step::Verify => "Verifying",
        Step::Remove => "Removing",
    }
}

impl Interaction for Plain {
    fn output(&self, line: &str) {
        println!("{line}");
    }

    fn note(&self, message: &str) {
        eprintln!("note: {message}");
    }

    fn confirm(&self, question: &str) -> Result<bool, Unanswered> {
        let unanswered = || Unanswered {
            question: question.to_owned(),
        };
        if !std::io::stdin().is_terminal() {
            return Err(unanswered());
        }
        print!("{question}{}", crate::QUESTION_END);
        std::io::stdout().flush().map_err(|_| unanswered())?;
        let mut answer = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut answer)
            .map_err(|_| unanswered())?;
        Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes" | "YES"))
    }

    fn has_terminal(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn with_terminal(&self, command: &mut Command) -> std::io::Result<ExitStatus> {
        command.status()
    }

    fn event(&self, event: Event) {
        match event {
            Event::Started { module, step } => println!("==> {} {module}", verb(step)),
            Event::Finished {
                module,
                step,
                error: Some(error),
            } => eprintln!("==> {} {module} failed: {error}", verb(step)),
            Event::Finished { .. } => {}
            Event::Skipped { reason, .. } => eprintln!("==> Skipping: {reason}"),
            Event::Declined { module } => {
                println!("==> {module} is not installed: its install was declined");
            }
        }
    }
}
