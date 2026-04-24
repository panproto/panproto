//! Binary entry point for the panproto REPL.
//!
//! Wraps [`panproto_repl::Repl`] with a `rustyline` line editor and
//! prints results to stdout.

use clap::Parser;
use panproto_repl::{Repl, ReplOutcome};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

/// CLI arguments for the panproto REPL binary.
#[derive(Parser, Debug)]
#[command(
    name = "panproto-repl",
    about = "Interactive REPL for panproto theories"
)]
struct Args {
    /// Load one or more theory documents on startup.
    #[arg(long = "load", value_name = "PATH")]
    load: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut repl = Repl::new();
    for path in &args.load {
        match repl.handle_line(&format!(":load {path}")) {
            ReplOutcome::Output(msg) => println!("{msg}"),
            ReplOutcome::Error(msg) => eprintln!("{msg}"),
            ReplOutcome::Quit => return Ok(()),
        }
    }

    let mut editor = DefaultEditor::new()?;
    loop {
        let prompt = repl
            .active
            .as_deref()
            .map_or_else(|| "panproto> ".to_string(), |n| format!("{n}> "));
        match editor.readline(&prompt) {
            Ok(line) => {
                let _ = editor.add_history_entry(line.as_str());
                match repl.handle_line(&line) {
                    ReplOutcome::Output(msg) => {
                        if !msg.is_empty() {
                            println!("{msg}");
                        }
                    }
                    ReplOutcome::Error(msg) => eprintln!("error: {msg}"),
                    ReplOutcome::Quit => break,
                }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        }
    }
    Ok(())
}
