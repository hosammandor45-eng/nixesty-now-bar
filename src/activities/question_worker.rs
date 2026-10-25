//! A question to Claude Code on its own thread: `claude -p` with the question on stdin and the
//! answer read from its stream as it's written. Claude Code's own sign-in and account; no tools,
//! no hooks, no session kept.

use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::question::{find_claude, needs_login, parse_line, Event, SYSTEM_HINT};
use crate::platform::log::log;

/// An answer that hasn't ended in this long won't.
const GIVE_UP_AFTER: Duration = Duration::from_secs(180);
/// The card repaints at most this often while the answer streams.
const POST_EVERY: Duration = Duration::from_millis(60);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AskState {
    #[default]
    Idle,
    /// Asked; nothing written yet.
    Thinking,
    Writing,
    Done,
    /// No `claude` on this PC.
    NoClaude,
    /// Claude Code isn't signed in (its own `/login`).
    NotSignedIn,
    Failed,
    TimedOut,
}

/// What the card reads: which question (a newer one wins), where it is, the answer so far.
#[derive(Clone, Debug, Default)]
pub struct AskBox {
    pub run: u32,
    pub state: AskState,
    pub answer: String,
}

pub type AskMailbox = Arc<Mutex<AskBox>>;

/// `claude` on PATH or where its installers put it.
pub fn locate() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from).unwrap_or_default();
    find_claude(&std::env::var("PATH").unwrap_or_default(), &var("USERPROFILE"), &var("APPDATA"), Path::is_file)
}

#[derive(Default)]
pub struct AskWorker {
    run: Arc<AtomicU32>,
    child: Arc<Mutex<Option<Child>>>,
}

impl AskWorker {
    /// Asks `question` (one at a time: a new one stops the last); the answer comes into `mailbox`,
    /// with `post` after each piece.
    pub fn ask(&self, question: String, mailbox: AskMailbox, post: fn()) {
        self.stop();
        let run = self.run.fetch_add(1, Ordering::SeqCst) + 1;
        let (current, child_slot) = (self.run.clone(), self.child.clone());
        // Only the question still asked writes to the card.
        let say = move |state: AskState, answer: Option<String>| {
            if current.load(Ordering::SeqCst) != run {
                return;
            }
            if let Ok(mut b) = mailbox.lock() {
                b.run = run;
                b.state = state;
                if let Some(a) = answer {
                    b.answer = a;
                }
            }
            post();
        };
        say(AskState::Thinking, Some(String::new()));
        let spawned = std::thread::Builder::new().name("question".into()).spawn(move || {
            let Some(exe) = locate() else {
                log("question: no claude on this PC");
                return say(AskState::NoClaude, None);
            };
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
            let started = Command::new(&exe)
                .args(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages", "--tools", "", "--no-session-persistence"])
                .args(["--settings", r#"{"disableAllHooks":true}"#, "--append-system-prompt", SYSTEM_HINT])
                // Started from inside a Claude Code session, it would refuse to nest.
                .env_remove("CLAUDECODE")
                .env_remove("CLAUDE_CODE_ENTRYPOINT")
                // Our own hook stays out of it even if hooks run (no live item for the card's question).
                .env("NOWBAR_QUESTION", "1")
                .current_dir(home)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
            let mut child = match started {
                Ok(c) => c,
                Err(e) => {
                    log(&format!("question: claude didn't start ({:?})", e.kind()));
                    return say(AskState::NoClaude, None);
                }
            };
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(question.as_bytes());
            }
            let Some(stdout) = child.stdout.take() else { return say(AskState::Failed, None) };
            if let Ok(mut slot) = child_slot.lock() {
                *slot = Some(child);
            }
            // A watchdog: an answer that never ends is stopped.
            let (watch_slot, watch_run, watch_say) = (child_slot.clone(), run, say.clone());
            let ended = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let watch_ended = ended.clone();
            let _ = std::thread::Builder::new().name("question-watch".into()).spawn(move || {
                std::thread::sleep(GIVE_UP_AFTER);
                if !watch_ended.load(Ordering::SeqCst) {
                    log(&format!("question {watch_run}: no answer in {} s", GIVE_UP_AFTER.as_secs()));
                    if let Some(mut c) = watch_slot.lock().ok().and_then(|mut s| s.take()) {
                        let _ = c.kill();
                    }
                    watch_say(AskState::TimedOut, None);
                }
            });
            let (mut answer, mut deltas, mut last_post) = (String::new(), false, Instant::now() - POST_EVERY);
            let mut end: Option<AskState> = None;
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match parse_line(&line) {
                    Some(Event::Delta(t)) => {
                        deltas = true;
                        answer.push_str(&t);
                    }
                    Some(Event::Message(t)) if !deltas => answer = t,
                    Some(Event::Message(_)) => continue,
                    Some(Event::Done { text, error }) => {
                        end = Some(match (error, needs_login(&text)) {
                            (true, true) => AskState::NotSignedIn,
                            (true, false) => AskState::Failed,
                            (false, _) => {
                                answer = text;
                                AskState::Done
                            }
                        });
                        break;
                    }
                    None => continue,
                }
                if last_post.elapsed() >= POST_EVERY {
                    last_post = Instant::now();
                    say(AskState::Writing, Some(answer.clone()));
                }
            }
            ended.store(true, Ordering::SeqCst);
            if let Some(mut c) = child_slot.lock().ok().and_then(|mut s| s.take()) {
                let _ = c.wait();
            }
            let end = end.unwrap_or(AskState::Failed);
            if end != AskState::Done {
                log(&format!("question {run}: {end:?}"));
            }
            say(end, (end == AskState::Done).then_some(answer));
        });
        if let Err(e) = spawned {
            log(&format!("question: no thread ({e})"));
        }
    }

    /// The question asked stops (the card closed, or another came).
    pub fn stop(&self) {
        self.run.fetch_add(1, Ordering::SeqCst);
        if let Some(mut c) = self.child.lock().ok().and_then(|mut s| s.take()) {
            let _ = c.kill();
        }
    }
}
