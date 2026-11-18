//! A client for the QEMU Machine Protocol (QMP), the JSON control channel of a running VM.
//! It takes screenshots and presses keys and mouse buttons in the guest.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};

const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum QmpError {
    #[error("could not connect to the QMP socket {0}: {1}")]
    Connect(PathBuf, io::Error),
    #[error("the QMP connection failed: {0}")]
    Io(#[from] io::Error),
    #[error("QEMU closed the QMP connection")]
    Closed,
    #[error("QEMU sent a QMP message that is not valid: {0}")]
    Protocol(String),
    #[error("QEMU refused {command}: {class}: {description}")]
    Refused {
        command: String,
        class: String,
        description: String,
    },
    #[error("the character {0:?} has no key on the US keyboard layout")]
    Untypable(char),
}

/// A QMP session with capabilities negotiated, ready for commands.
pub struct Qmp {
    writer: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Qmp {
    pub fn connect(socket: &Path) -> Result<Qmp, QmpError> {
        let stream = UnixStream::connect(socket)
            .map_err(|error| QmpError::Connect(socket.to_owned(), error))?;
        stream.set_read_timeout(Some(REPLY_TIMEOUT))?;
        let reader = BufReader::new(stream.try_clone()?);
        let mut qmp = Qmp {
            writer: stream,
            reader,
        };
        let greeting = qmp.read_message()?;
        if greeting.get("QMP").is_none() {
            return Err(QmpError::Protocol(greeting.to_string()));
        }
        qmp.execute("qmp_capabilities", json!({}))?;
        Ok(qmp)
    }

    /// Runs one QMP command and returns its `return` value. Events that arrive before the
    /// reply are skipped.
    pub fn execute(&mut self, command: &str, arguments: Value) -> Result<Value, QmpError> {
        let mut request =
            serde_json::to_vec(&json!({ "execute": command, "arguments": arguments }))
                .map_err(|error| QmpError::Protocol(error.to_string()))?;
        request.push(b'\n');
        self.writer.write_all(&request)?;
        loop {
            let mut message = self.read_message()?;
            if message.get("event").is_some() {
                continue;
            }
            if let Some(reply) = message.get_mut("return") {
                return Ok(reply.take());
            }
            if let Some(error) = message.get("error") {
                let field = |name| {
                    error
                        .get(name)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                return Err(QmpError::Refused {
                    command: command.to_owned(),
                    class: field("class"),
                    description: field("desc"),
                });
            }
            return Err(QmpError::Protocol(message.to_string()));
        }
    }

    /// Saves the current screen of the guest as a PNG file on the host.
    pub fn screendump(&mut self, file: &Path) -> Result<(), QmpError> {
        let file = file
            .to_str()
            .ok_or_else(|| QmpError::Protocol(format!("{} is not UTF-8", file.display())))?;
        self.execute("screendump", json!({ "filename": file, "format": "png" }))?;
        Ok(())
    }

    /// Saves the current screen of the guest as a binary PPM file on the host, the format
    /// that `crate::ppm` reads.
    pub fn screendump_ppm(&mut self, file: &Path) -> Result<(), QmpError> {
        let file = file
            .to_str()
            .ok_or_else(|| QmpError::Protocol(format!("{} is not UTF-8", file.display())))?;
        self.execute("screendump", json!({ "filename": file, "format": "ppm" }))?;
        Ok(())
    }

    /// Presses the keys together, then releases them. Keys are QEMU key codes, such as
    /// `alt`, `y`, `ret` or `shift`.
    pub fn send_keys(&mut self, keys: &[&str]) -> Result<(), QmpError> {
        let keys: Vec<Value> = keys
            .iter()
            .map(|key| json!({ "type": "qcode", "data": key }))
            .collect();
        self.execute("send-key", json!({ "keys": keys }))?;
        Ok(())
    }

    /// Types the text one key at a time. The guest must use the US keyboard layout, because
    /// QEMU sends key positions, not characters.
    pub fn type_text(&mut self, text: &str) -> Result<(), QmpError> {
        let keys = text
            .chars()
            .map(|character| key_for(character).ok_or(QmpError::Untypable(character)));
        for key in keys.collect::<Result<Vec<_>, _>>()? {
            match key {
                (code, false) => self.send_keys(&[code])?,
                (code, true) => self.send_keys(&["shift", code])?,
            }
        }
        Ok(())
    }

    /// Asks the guest to shut down, as a press of the power button does.
    pub fn powerdown(&mut self) -> Result<(), QmpError> {
        self.execute("system_powerdown", json!({}))?;
        Ok(())
    }

    fn read_message(&mut self) -> Result<Value, QmpError> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(QmpError::Closed);
        }
        serde_json::from_str(&line).map_err(|_| QmpError::Protocol(line.trim().to_owned()))
    }
}

/// The QEMU key code for a character on the US layout, and whether it needs Shift.
pub(crate) fn key_for(character: char) -> Option<(&'static str, bool)> {
    const LETTERS: [&str; 26] = [
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
        "s", "t", "u", "v", "w", "x", "y", "z",
    ];
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    const SHIFTED_DIGITS: &str = ")!@#$%^&*(";
    const SYMBOLS: [(char, char, &str); 11] = [
        ('-', '_', "minus"),
        ('=', '+', "equal"),
        ('[', '{', "bracket_left"),
        (']', '}', "bracket_right"),
        ('\\', '|', "backslash"),
        (';', ':', "semicolon"),
        ('\'', '"', "apostrophe"),
        ('`', '~', "grave_accent"),
        (',', '<', "comma"),
        ('.', '>', "dot"),
        ('/', '?', "slash"),
    ];

    if character.is_ascii_lowercase() {
        return Some((LETTERS[usize::from(character as u8 - b'a')], false));
    }
    if character.is_ascii_uppercase() {
        return Some((LETTERS[usize::from(character as u8 - b'A')], true));
    }
    if character.is_ascii_digit() {
        return Some((DIGITS[usize::from(character as u8 - b'0')], false));
    }
    if let Some(index) = SHIFTED_DIGITS.find(character) {
        return Some((DIGITS[index], true));
    }
    match character {
        ' ' => return Some(("spc", false)),
        '\n' => return Some(("ret", false)),
        '\t' => return Some(("tab", false)),
        _ => {}
    }
    SYMBOLS.iter().find_map(|&(plain, shifted, code)| {
        (character == plain)
            .then_some((code, false))
            .or((character == shifted).then_some((code, true)))
    })
}

#[cfg(test)]
mod tests {
    use super::key_for;

    #[test]
    fn letters_digits_and_space_need_no_shift_and_capitals_do() {
        assert_eq!(key_for('t'), Some(("t", false)));
        assert_eq!(key_for('T'), Some(("t", true)));
        assert_eq!(key_for('0'), Some(("0", false)));
        assert_eq!(key_for(' '), Some(("spc", false)));
        assert_eq!(key_for('\n'), Some(("ret", false)));
    }

    #[test]
    fn symbols_use_the_key_of_the_us_layout_with_shift_for_the_upper_one() {
        assert_eq!(key_for('!'), Some(("1", true)));
        assert_eq!(key_for(')'), Some(("0", true)));
        assert_eq!(key_for('-'), Some(("minus", false)));
        assert_eq!(key_for('_'), Some(("minus", true)));
        assert_eq!(key_for('"'), Some(("apostrophe", true)));
        assert_eq!(key_for('?'), Some(("slash", true)));
    }

    #[test]
    fn characters_without_a_us_key_are_refused() {
        assert_eq!(key_for('é'), None);
    }
}
