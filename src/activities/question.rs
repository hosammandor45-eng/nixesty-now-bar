//! A quick question to Claude from the card: the text field's keys, what `claude -p`'s stream
//! says, the answer as plain text, where `claude` is. The pure part; question_worker.rs runs it.

use std::path::{Path, PathBuf};

/// A question this long at most (a paste is cut).
pub const MAX_QUESTION: usize = 2000;

/// Asked of every answer: it shows as plain text on a small card.
pub const SYSTEM_HINT: &str = "Answer briefly, in plain text without Markdown: the answer shows on a small card. Answer in the language of the question.";

/// One line of `claude -p --output-format stream-json`'s output, as far as the card cares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A piece of the answer as it's written.
    Delta(String),
    /// A whole message of the answer (when the pieces didn't come).
    Message(String),
    /// The end: the whole answer, or what went wrong.
    Done { text: String, error: bool },
}

pub fn parse_line(line: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    match v["type"].as_str()? {
        "stream_event" => {
            let e = &v["event"];
            (e["type"] == "content_block_delta" && e["delta"]["type"] == "text_delta").then(|| Event::Delta(e["delta"]["text"].as_str().unwrap_or("").to_string()))
        }
        "assistant" => {
            let text: String = v["message"]["content"].as_array()?.iter().filter(|c| c["type"] == "text").filter_map(|c| c["text"].as_str()).collect();
            (!text.is_empty()).then_some(Event::Message(text))
        }
        "result" => Some(Event::Done { text: v["result"].as_str().unwrap_or("").to_string(), error: v["is_error"].as_bool().unwrap_or(v["subtype"] != "success") }),
        _ => None,
    }
}

/// Claude Code isn't signed in on this PC (its own sign-in, not the desktop app's).
pub fn needs_login(text: &str) -> bool {
    text.contains("/login") || text.to_lowercase().contains("not logged in")
}

/// An answer's Markdown as plain text: no ** or `, headings as their words, bullets as "•".
pub fn plain(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .map(|l| {
            // "## Adımlar": the heading's words (not "#1", a number).
            let hashes = l.len() - l.trim_start_matches('#').len();
            let l = if hashes > 0 { l[hashes..].strip_prefix(' ').unwrap_or(l) } else { l };
            let t = l.trim_start();
            let l = match t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
                Some(rest) => format!("• {rest}"),
                None => l.to_string(),
            };
            l.replace("**", "").replace("__", "").replace('`', "")
        })
        .collect();
    lines.join("\n").trim().to_string()
}

/// What a key does to the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Changed,
    /// Enter: ask.
    Submit,
    /// Esc.
    Close,
    /// Ctrl+V: the app puts the clipboard's text in.
    Paste,
    None,
}

/// The question as it's typed (the caret stays at its end).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    /// The first half of a character outside the BMP (an emoji), waiting for its second.
    high: Option<u16>,
}

impl Field {
    /// One WM_CHAR: a character, or Backspace (0x08), Ctrl+Backspace (0x7F), Enter, Esc, Ctrl+V.
    pub fn key(&mut self, unit: u16) -> Edit {
        match unit {
            0x08 => match self.text.pop() {
                Some(_) => Edit::Changed,
                None => Edit::None,
            },
            // The spaces before the caret, then the word.
            0x7F if !self.text.is_empty() => {
                self.text.truncate(self.text.trim_end().len());
                self.text.truncate(self.text.trim_end_matches(|c: char| !c.is_whitespace()).len());
                Edit::Changed
            }
            0x0D => Edit::Submit,
            0x1B => Edit::Close,
            0x16 => Edit::Paste,
            0xD800..=0xDBFF => {
                self.high = Some(unit);
                Edit::None
            }
            0xDC00..=0xDFFF => match self.high.take().and_then(|h| char::decode_utf16([h, unit]).next()?.ok()) {
                Some(c) if self.room() => {
                    self.text.push(c);
                    Edit::Changed
                }
                _ => Edit::None,
            },
            u if u < 0x20 || u == 0x7F => Edit::None,
            u => match char::from_u32(u as u32) {
                Some(c) if self.room() => {
                    self.text.push(c);
                    Edit::Changed
                }
                _ => Edit::None,
            },
        }
    }

    fn room(&self) -> bool {
        self.text.chars().count() < MAX_QUESTION
    }

    /// A paste: one line (line breaks become spaces), cut at the field's length.
    pub fn insert(&mut self, s: &str) {
        let flat = s.replace("\r\n", " ").replace(['\r', '\n', '\t'], " ");
        for c in flat.chars().filter(|c| !c.is_control()) {
            if !self.room() {
                break;
            }
            self.text.push(c);
        }
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.high = None;
    }
}

/// Where `claude` is: on PATH (claude.exe, then claude.cmd), else the native installer's
/// `%USERPROFILE%\.local\bin`, else npm's `%APPDATA%\npm`.
pub fn find_claude(path_var: &str, profile: &Path, appdata: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let on_path = path_var.split(';').filter(|d| !d.trim().is_empty()).flat_map(|d| [Path::new(d.trim()).join("claude.exe"), Path::new(d.trim()).join("claude.cmd")]);
    let installed = [profile.join(r".local\bin\claude.exe"), appdata.join(r"npm\claude.cmd")];
    on_path.chain(installed).find(|p| exists(p))
}

/// How many lines `text` takes at about `per_line` characters a line, words kept whole (a
/// longer one breaks).
pub fn lines_estimate(text: &str, per_line: usize) -> usize {
    let per_line = per_line.max(1);
    let mut lines = 0;
    for para in text.split('\n') {
        let (mut n, mut used) = (1, 0);
        for word in para.split(' ').filter(|w| !w.is_empty()) {
            let w = word.chars().count();
            let need = if used == 0 { w } else { used + 1 + w };
            if need <= per_line {
                used = need;
                continue;
            }
            if used > 0 {
                n += 1;
            }
            let spans = w.div_ceil(per_line);
            n += spans - 1;
            used = w - (spans - 1) * per_line;
        }
        lines += n;
    }
    lines.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stream_says_its_pieces_and_its_end() {
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"İki"}},"session_id":"s"}"#;
        assert_eq!(parse_line(delta), Some(Event::Delta("İki".into())));
        let message = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Merhaba"},{"type":"text","text":" dünya"}]},"session_id":"s"}"#;
        assert_eq!(parse_line(message), Some(Event::Message("Merhaba dünya".into())));
        let done = r#"{"type":"result","subtype":"success","is_error":false,"result":"2","session_id":"s"}"#;
        assert_eq!(parse_line(done), Some(Event::Done { text: "2".into(), error: false }));
        let failed = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#;
        assert_eq!(parse_line(failed), Some(Event::Done { text: "Not logged in · Please run /login".into(), error: true }));
        let ran_out = r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#;
        assert_eq!(parse_line(ran_out), Some(Event::Done { text: String::new(), error: true }));
        assert_eq!(parse_line(r#"{"type":"system","subtype":"init","session_id":"s"}"#), None);
        assert_eq!(parse_line(r#"{"type":"stream_event","event":{"type":"message_start"}}"#), None);
        assert_eq!(parse_line("yarım {"), None);
    }

    #[test]
    fn a_missing_sign_in_is_told_apart() {
        assert!(needs_login("Not logged in · Please run /login"));
        assert!(needs_login("Invalid API key · Please run /login"));
        assert!(!needs_login("2"));
    }

    #[test]
    fn markdown_reads_as_plain_text() {
        assert_eq!(plain("**Kısa** yanıt: `cargo t` çalıştır."), "Kısa yanıt: cargo t çalıştır.");
        assert_eq!(plain("## Adımlar\n- birinci\n* ikinci\n1. üçüncü"), "Adımlar\n• birinci\n• ikinci\n1. üçüncü");
        assert_eq!(plain("```\nlet x = 1;\n```"), "let x = 1;");
        assert_eq!(plain("2 * 3 = 6"), "2 * 3 = 6", "a lone star is arithmetic");
    }

    #[test]
    fn the_field_takes_keys() {
        let mut f = Field::default();
        for c in "Merhaba dünya".encode_utf16() {
            assert_eq!(f.key(c), Edit::Changed);
        }
        assert_eq!(f.text, "Merhaba dünya");
        assert_eq!(f.key(0x08), Edit::Changed);
        assert_eq!(f.text, "Merhaba düny", "Backspace: a character");
        assert_eq!(f.key(0x7F), Edit::Changed);
        assert_eq!(f.text, "Merhaba ", "Ctrl+Backspace: the word");
        assert_eq!(f.key(0x7F), Edit::Changed);
        assert_eq!(f.text, "", "and the one before, with its space");
        assert_eq!(f.key(0x08), Edit::None, "nothing to delete");
        assert_eq!((f.key(0x0D), f.key(0x1B), f.key(0x16), f.key(0x01)), (Edit::Submit, Edit::Close, Edit::Paste, Edit::None));
        // 😀 comes as two halves.
        let units: Vec<u16> = "a😀".encode_utf16().collect();
        assert_eq!(units.len(), 3);
        for u in units {
            f.key(u);
        }
        assert_eq!(f.text, "a😀");
        f.key(0x08);
        assert_eq!(f.text, "a", "the emoji goes whole");
    }

    #[test]
    fn a_paste_is_one_line_and_has_an_end() {
        let mut f = Field::default();
        f.insert("ilk satır\r\nikinci\tsatır");
        assert_eq!(f.text, "ilk satır ikinci satır");
        f.insert(&"ş".repeat(5000));
        assert_eq!(f.text.chars().count(), MAX_QUESTION);
        assert_eq!(f.key('x' as u16), Edit::None, "full");
    }

    #[test]
    fn claude_is_found_where_it_installs() {
        let profile = Path::new(r"C:\Users\Ekin");
        let appdata = Path::new(r"C:\Users\Ekin\AppData\Roaming");
        let path = r"C:\Windows;C:\Tools";
        let on = |list: &'static [&'static str]| move |p: &Path| list.iter().any(|x| Path::new(x) == p);
        assert_eq!(find_claude(path, profile, appdata, on(&[r"C:\Tools\claude.exe"])), Some(PathBuf::from(r"C:\Tools\claude.exe")));
        assert_eq!(find_claude(path, profile, appdata, on(&[r"C:\Tools\claude.cmd"])), Some(PathBuf::from(r"C:\Tools\claude.cmd")));
        assert_eq!(find_claude(path, profile, appdata, on(&[r"C:\Users\Ekin\.local\bin\claude.exe"])), Some(PathBuf::from(r"C:\Users\Ekin\.local\bin\claude.exe")), "the native installer's");
        assert_eq!(find_claude(path, profile, appdata, on(&[r"C:\Users\Ekin\AppData\Roaming\npm\claude.cmd"])), Some(PathBuf::from(r"C:\Users\Ekin\AppData\Roaming\npm\claude.cmd")), "npm's");
        assert_eq!(find_claude(path, profile, appdata, on(&[])), None);
    }

    #[test]
    fn lines_by_words() {
        assert_eq!(lines_estimate("", 10), 1);
        assert_eq!(lines_estimate("kısa", 10), 1);
        assert_eq!(lines_estimate("bir iki üç dört", 10), 2, "words stay whole: 'bir iki üç' and 'dört'");
        assert_eq!(lines_estimate("a\n\nb", 10), 3, "line breaks count");
        assert_eq!(lines_estimate(&"x".repeat(25), 10), 3, "a long word breaks");
    }
}
