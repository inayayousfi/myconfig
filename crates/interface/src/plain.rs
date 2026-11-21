//! The plain command line: output in the normal terminal, and questions asked there or,
//! in installer mode, answered from a file.
use std::{
    collections::HashMap,
    io::{BufRead, IsTerminal, Write},
    path::Path,
    process::{Command, ExitStatus},
};

use inayos_modules::{Event, Interaction, ModuleResult, Step, Unanswered};

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

/// Installer mode: output and notes as on the plain command line, and answers read from a
/// file, because the installer's run has no terminal.
pub struct Answered {
    answers: HashMap<String, bool>,
}

impl Answered {
    /// Reads the answers from `path`, a JSON object that maps each question's exact text to
    /// true or false. Without a file, no question has an answer.
    pub fn read(path: Option<&Path>) -> ModuleResult<Self> {
        let answers = match path {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .map_err(|error| format!("could not read {}: {error}", path.display()))?;
                serde_json::from_str(&text).map_err(|error| {
                    format!(
                        "{} is not a JSON object of yes or no answers: {error}",
                        path.display()
                    )
                })?
            }
            None => HashMap::new(),
        };
        Ok(Self { answers })
    }
}

impl Interaction for Answered {
    fn output(&self, line: &str) {
        Plain.output(line);
    }

    fn note(&self, message: &str) {
        Plain.note(message);
    }

    fn confirm(&self, question: &str) -> Result<bool, Unanswered> {
        self.answers
            .get(question)
            .copied()
            .ok_or_else(|| Unanswered {
                question: question.to_owned(),
            })
    }

    fn has_terminal(&self) -> bool {
        false
    }

    fn with_terminal(&self, _command: &mut Command) -> std::io::Result<ExitStatus> {
        Err(std::io::Error::other("installer mode has no terminal"))
    }

    fn event(&self, event: Event) {
        Plain.event(event);
    }
}

#[cfg(test)]
mod tests {
    use inayos_modules::Interaction;

    use super::Answered;

    #[test]
    fn installer_answers_come_from_the_file_and_other_questions_have_none() {
        let path = std::env::temp_dir().join(format!("inayos-answers-{}", std::process::id()));
        std::fs::write(
            &path,
            r#"{"The OpenSSH server is off. Turn it on now?": true, "Log in now?": false}"#,
        )
        .unwrap();
        let answers = Answered::read(Some(&path));
        std::fs::remove_file(&path).unwrap();
        let answers = answers.unwrap();
        assert!(
            answers
                .confirm("The OpenSSH server is off. Turn it on now?")
                .unwrap()
        );
        assert!(!answers.confirm("Log in now?").unwrap());
        assert!(answers.confirm("Remove it anyway?").is_err());
        assert!(!answers.has_terminal());
    }
}
