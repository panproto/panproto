//! Shared rustyline driver and syntax-highlighter for the panproto
//! REPLs (`schema expr repl` and `schema theory repl`).
//!
//! Both REPLs accept input that mixes a small set of `:`-prefixed
//! meta-commands with a JSON-flavoured term syntax. They share a
//! [`ReplHelper`] that provides ANSI syntax highlighting, line
//! editing, and command-name completion via `rustyline`. Per-REPL
//! differences (the keyword set, the prompt, what to do with each
//! line) live in the call site.

use std::borrow::Cow;
use std::path::PathBuf;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Cmd, Editor, Helper, KeyEvent};

mod highlight;

pub use highlight::error;

/// Outcome of processing one input line. Drives the loop in
/// [`run_repl`].
pub enum LineResult {
    /// Print this string and continue the loop. Empty strings are
    /// suppressed.
    Output(String),
    /// Print this string in the error palette and continue.
    Error(String),
    /// Exit the REPL cleanly.
    Quit,
}

/// Per-REPL configuration: keyword set, REPL meta-commands, banner
/// lines, and the prompt callback. The prompt is recomputed each
/// iteration so callers can reflect mode (e.g. an "active theory"
/// indicator on the theory REPL prompt).
pub struct ReplConfig<'a> {
    /// Banner text printed once at startup. Each entry is a line.
    pub banner: &'a [&'a str],
    /// Language keywords used by the highlighter.
    pub keywords: &'static [&'static str],
    /// Meta-commands (without leading `:`) used for tab completion.
    pub commands: &'static [&'static str],
    /// File for persisted history. Created if absent.
    pub history_file: Option<PathBuf>,
    /// Prompt-renderer; called on each iteration.
    pub prompt: Box<dyn FnMut() -> String + 'a>,
}

/// rustyline `Helper` impl that combines syntax highlighting,
/// command-name completion, and the default validator/hinter.
pub struct ReplHelper {
    keywords: &'static [&'static str],
    commands: &'static [&'static str],
}

impl ReplHelper {
    /// Build a helper that highlights `keywords` as language keywords
    /// and tab-completes `commands` after a leading `:`.
    #[must_use]
    pub const fn new(keywords: &'static [&'static str], commands: &'static [&'static str]) -> Self {
        Self { keywords, commands }
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        highlight::highlight_line(line, self.keywords)
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        Cow::Owned(highlight::colour_prompt(prompt))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        // Re-render on every keystroke so colour follows the cursor as
        // the user types. Cheap because `highlight_line` short-circuits
        // when no token would change colour.
        true
    }
}

impl Hinter for ReplHelper {
    type Hint = String;
}

impl Validator for ReplHelper {}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Only complete a leading `:command` token. The cursor must be
        // inside the first whitespace-bounded word, and the line must
        // start with `:`.
        if !line.starts_with(':') {
            return Ok((0, Vec::new()));
        }
        let first_space = line.find(char::is_whitespace).unwrap_or(line.len());
        if pos > first_space {
            return Ok((0, Vec::new()));
        }
        let stem = &line[1..pos.min(first_space)];
        let mut hits: Vec<Pair> = self
            .commands
            .iter()
            .filter(|c| c.starts_with(stem))
            .map(|c| Pair {
                display: format!(":{c}"),
                replacement: format!(":{c}"),
            })
            .collect();
        hits.sort_by(|a, b| a.replacement.cmp(&b.replacement));
        Ok((0, hits))
    }
}

impl Helper for ReplHelper {}

/// Run an interactive REPL loop. `handle_line` is called for each
/// non-empty input line and returns the next [`LineResult`]; the loop
/// exits on `Quit`, `Ctrl-D`, or `Ctrl-C`.
///
/// # Errors
///
/// Returns an error if the underlying rustyline editor fails to
/// initialise or to read a line for reasons other than user-driven
/// EOF/interrupt.
pub fn run_repl<F>(mut config: ReplConfig<'_>, mut handle_line: F) -> miette::Result<()>
where
    F: FnMut(&str) -> LineResult,
{
    use miette::IntoDiagnostic;

    for line in config.banner {
        println!("{line}");
    }
    if !config.banner.is_empty() {
        println!();
    }

    let helper = ReplHelper::new(config.keywords, config.commands);
    let mut editor: Editor<ReplHelper, DefaultHistory> = Editor::new().into_diagnostic()?;
    editor.set_helper(Some(helper));
    // Bind Ctrl-L to clear the screen, mirroring most line editors.
    editor.bind_sequence(KeyEvent::ctrl('L'), Cmd::ClearScreen);

    if let Some(path) = config.history_file.as_deref() {
        let _ = editor.load_history(path);
    }

    loop {
        let prompt = (config.prompt)();
        match editor.readline(&prompt) {
            Ok(line) => {
                let _ = editor.add_history_entry(line.as_str());
                match handle_line(&line) {
                    LineResult::Output(msg) => {
                        if !msg.is_empty() {
                            println!("{msg}");
                        }
                    }
                    LineResult::Error(msg) => {
                        eprintln!("{}", error(&msg));
                    }
                    LineResult::Quit => break,
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => {
                return Err(miette::miette!("readline error: {e}"));
            }
        }
    }

    if let Some(path) = config.history_file.as_deref() {
        let _ = editor.save_history(path);
    }
    Ok(())
}

/// Resolve `$XDG_DATA_HOME/panproto/<name>` (or
/// `$HOME/.local/share/panproto/<name>` on Linux/macOS,
/// `%LOCALAPPDATA%\panproto\<name>` on Windows) for REPL history
/// persistence. Returns `None` when no suitable directory can be
/// found, in which case the caller should run without history.
#[must_use]
pub fn history_path(name: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    }?;
    let dir = base.join("panproto");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(name))
}
