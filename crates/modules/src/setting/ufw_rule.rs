//! A `ufw` rule, given as the arguments that add it.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct UfwRule<'a> {
    pub rule: &'a [String],
}

impl Kind for UfwRule<'_> {
    fn describe(&self) -> String {
        format!("firewall rule: ufw {}", self.rule.join(" "))
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let added = ctx.read(cmd!(ctx.shell, "sudo ufw show added"))?;
        Ok(added
            .lines()
            .any(|line| rule_matches(self.rule, line))
            .then(|| "present".to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let rule = self.rule;
        ctx.run(cmd!(ctx.shell, "sudo ufw {rule...}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let rule = self.rule;
        ctx.run(cmd!(ctx.shell, "sudo ufw delete {rule...}"))
    }
}

/// The parts of a ufw rule that identify it: its action, then `from`, `to`, `port`
/// and `proto` with their values. The direction `in` is ufw's default and is not listed.
fn rule_parts(words: &[&str]) -> Vec<String> {
    let end = words
        .iter()
        .position(|word| *word == "comment")
        .unwrap_or(words.len());
    let words = &words[..end];
    let mut parts: Vec<String> = words
        .first()
        .map(|action| (*action).to_owned())
        .into_iter()
        .collect();
    for pair in words.windows(2) {
        if matches!(pair[0], "from" | "to" | "port" | "proto") {
            parts.push(format!("{} {}", pair[0], pair[1]));
        }
    }
    parts.sort();
    parts
}

/// Whether a line of `ufw show added` is the rule given by these `ufw` arguments.
/// ufw prints rules in its own word order, so the parts are compared, not the text.
fn rule_matches(rule: &[String], line: &str) -> bool {
    let Some(listed) = line.trim().strip_prefix("ufw ") else {
        return false;
    };
    let wanted: Vec<&str> = rule
        .iter()
        .map(String::as_str)
        .filter(|word| *word != "in")
        .collect();
    let listed: Vec<&str> = listed
        .split_whitespace()
        .filter(|word| *word != "in")
        .collect();
    rule_parts(&wanted) == rule_parts(&listed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ufw_rules_match_the_form_ufw_lists_them_in() {
        let rule: Vec<String> = [
            "allow",
            "in",
            "proto",
            "tcp",
            "from",
            "10.0.0.0/8",
            "to",
            "any",
            "port",
            "18080,18081",
            "comment",
            "myconfig Emacs browser terminal",
        ]
        .iter()
        .map(|word| (*word).to_owned())
        .collect();
        // The form ufw's own parser prints for these arguments.
        let listed = "ufw allow from 10.0.0.0/8 to any port 18080,18081 proto tcp comment 'myconfig Emacs browser terminal'";
        assert!(rule_matches(&rule, listed));
        assert!(!rule_matches(
            &rule,
            "ufw allow from 192.168.0.0/16 to any port 18080,18081 proto tcp"
        ));
        assert!(!rule_matches(
            &rule,
            "ufw deny from 10.0.0.0/8 to any port 18080,18081 proto tcp"
        ));
    }
}
