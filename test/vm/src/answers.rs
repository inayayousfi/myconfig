//! Decides what to type when the installer waits for input: the password at a `sudo` prompt,
//! or the answer from a fixed table at a yes or no question. A question that the table does
//! not know stops the cycle, so a new question is never answered by guess.

use inayos_interface::QUESTION_END;
use regex::Regex;

/// The prompts of `sudo` and `sudo-rs`, such as `[sudo] password for tester: `.
const SUDO_PROMPT: &str = r"(?m)^\[sudo[^\]\n]*\] [Pp]assword[^\n]*: ?\z";

/// One known question. `pattern` is a regular expression that must match the whole question.
struct Entry {
    pattern: &'static str,
    yes: bool,
}

/// The questions of `crates/modules`, matched by their text. Install questions get the answer
/// that a person setting up a new machine gives. Remove questions all get yes, because the
/// cycle expects removal to bring back the state from before the install.
const ENTRIES: &[Entry] = &[
    // ssh.rs
    Entry {
        pattern: r"The OpenSSH server is off\. Turn it on now\?",
        yes: true,
    },
    // arch_wsl.rs
    Entry {
        pattern: r"The setup will shut down all running WSL distributions to apply Arch WSL settings\. Continue\?",
        yes: true,
    },
    // arch_wsl.rs
    Entry {
        pattern: r"Replace .+ with the following WSL idle settings\?\n[\s\S]*",
        yes: true,
    },
    // shared_desktop.rs
    Entry {
        pattern: r"The shared desktop item .+ would replace .+\. Replace it\?",
        yes: true,
    },
    // runner.rs, during remove.
    Entry {
        pattern: r".+ is still needed by .+\. Remove it anyway\?",
        yes: true,
    },
    Entry {
        pattern: r"Delete .+\? It may hold changes made after install\.",
        yes: true,
    },
    Entry {
        pattern: r"Install .+ again\? Install uninstalled it on purpose\.",
        yes: true,
    },
    Entry {
        pattern: r"Link the retired config package .+ into your home again\?",
        yes: true,
    },
    Entry {
        pattern: r"Move .+ back to .+\?",
        yes: true,
    },
    Entry {
        pattern: r"Unregister the WSL distribution .+ and delete everything inside it\?",
        yes: true,
    },
    // login.rs: a test machine has no accounts to log in to.
    Entry {
        pattern: r"GitHub CLI is not authenticated\. Log in now\?",
        yes: false,
    },
    Entry {
        pattern: r"Tailscale is not authenticated\. Log in now\?",
        yes: false,
    },
];

/// What to do about the installer output seen since the last thing typed.
#[derive(Debug, PartialEq, Eq)]
pub enum Response {
    /// The installer is not waiting for input.
    Wait,
    /// Type the sudo password, then Enter.
    Password,
    /// Type `y` or `n`, then Enter.
    Answer { question: String, yes: bool },
    /// The installer asks a question that the table does not know.
    Stop(UnknownQuestion),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("the installer asked a question that the answer table does not know: {question}")]
pub struct UnknownQuestion {
    pub question: String,
}

/// The answer table, with its patterns compiled.
pub struct Answers {
    entries: Vec<(Regex, bool)>,
    sudo: Regex,
}

impl Answers {
    pub fn new() -> Answers {
        let entries = ENTRIES
            .iter()
            .map(|entry| {
                let pattern = format!(r"(?:\A|\n)(?:{})\z", entry.pattern);
                (
                    Regex::new(&pattern).expect("every answer pattern is valid"),
                    entry.yes,
                )
            })
            .collect();
        Answers {
            entries,
            sudo: Regex::new(SUDO_PROMPT).expect("the sudo pattern is valid"),
        }
    }

    /// Decides what to type after `output`, which is everything the installer printed since
    /// the last thing typed, with carriage returns removed.
    pub fn respond(&self, output: &str) -> Response {
        if self.sudo.is_match(output) {
            return Response::Password;
        }
        let Some(asked) = output.strip_suffix(QUESTION_END) else {
            return Response::Wait;
        };
        // A question starts on a new line, after the output of the step before it.
        if let Some((_, yes)) = self
            .entries
            .iter()
            .find(|(pattern, _)| pattern.is_match(asked))
        {
            let question = asked.rsplit('\n').next().unwrap_or(asked).to_owned();
            return Response::Answer {
                question,
                yes: *yes,
            };
        }
        Response::Stop(UnknownQuestion {
            question: last_lines(asked, 5),
        })
    }
}

/// The last `count` lines of `text`, enough to show a question that spans several lines.
fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_question_gets_its_answer_from_the_table() {
        let output = "==> Installing ssh\nThe OpenSSH server is off. Turn it on now? [y/N] ";
        assert_eq!(
            Answers::new().respond(output),
            Response::Answer {
                question: "The OpenSSH server is off. Turn it on now?".to_owned(),
                yes: true,
            }
        );
    }

    #[test]
    fn an_unknown_question_stops_the_run_with_its_text() {
        let output = "==> Installing docker\nAdd tester to the docker group? [y/N] ";
        assert_eq!(
            Answers::new().respond(output),
            Response::Stop(UnknownQuestion {
                question: "==> Installing docker\nAdd tester to the docker group?".to_owned(),
            })
        );
    }

    #[test]
    fn a_known_question_inside_a_longer_line_is_not_taken_for_it() {
        // Only the whole question counts, so a different question that ends like a known one
        // stops the run instead of getting that answer.
        let output = "Before: The OpenSSH server is off. Turn it on now? [y/N] ";
        assert!(matches!(Answers::new().respond(output), Response::Stop(_)));
    }

    #[test]
    fn a_sudo_prompt_asks_for_the_password() {
        let output = "==> Installing base\n[sudo] password for tester: ";
        assert_eq!(Answers::new().respond(output), Response::Password);
    }

    #[test]
    fn output_without_a_prompt_waits() {
        let output = "==> Installing base\nresolving dependencies...\n";
        assert_eq!(Answers::new().respond(output), Response::Wait);
    }
}
