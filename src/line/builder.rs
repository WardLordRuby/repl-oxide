use crate::line::{
    completion::{CommandScheme, Completion},
    history::History,
    LineData, ParseErrHook, Repl,
};

use std::io::{self, ErrorKind, Write};

use crossterm::{cursor, terminal, QueueableCommand};
use shellwords::split as shellwords_split;

/* -------------------------------- Debug tool -------------------------------- */
// static mut DEBUGGER: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();

// pub(crate) fn get_debugger() -> &'static mut std::fs::File {
//     #[allow(static_mut_refs)]
//     unsafe {
//         DEBUGGER.get_mut().unwrap()
//     }
// }

// fn await_debug_server(n_pipe: &str) {
//     let file = loop {
//         match std::fs::OpenOptions::new().write(true).open(n_pipe) {
//             Ok(file) => break file,
//             Err(_) => std::thread::sleep(std::time::Duration::from_millis(500)),
//         }
//     };

//     #[allow(static_mut_refs)]
//     unsafe {
//         DEBUGGER.set(file).unwrap();
//     }
// }
/* -------------------------------- Debug tool -------------------------------- */

/// Builder for [`Repl`]s
///
/// Access through [`Repl::new`]
pub struct ReplBuilder<'a, Ctx, W: Write> {
    completion: Option<&'static CommandScheme>,
    custom_quit: Option<&'a str>,
    term: W,
    term_size: Option<(u16, u16)>,
    prompt: Option<String>,
    prompt_end: Option<String>,
    starting_history: Option<History>,
    style_enabled: bool,
    parse_err_hook: Option<Box<dyn ParseErrHook<Ctx, W>>>,
}

impl<Ctx, W: Write> Repl<Ctx, W> {
    /// Builder for [`Repl`]
    ///
    /// Must supply a writer that is compatible with executing commands via the `crossterm` crate.
    #[expect(clippy::new_ret_no_self, reason = "force users into builder pattern")]
    pub fn new(terminal: W) -> ReplBuilder<'static, Ctx, W> {
        // await_debug_server(r"\\.\pipe\debug_log");

        ReplBuilder {
            completion: None,
            custom_quit: None,
            term: terminal,
            term_size: None,
            prompt: None,
            prompt_end: None,
            starting_history: None,
            style_enabled: true,
            parse_err_hook: None,
        }
    }
}

impl<'a, Ctx, W: Write> ReplBuilder<'a, Ctx, W> {
    /// Supply a custom command to be executed when the user tries to quit with 'ctrl + c' when the current
    /// line is empty, or anytime 'ctrl + d' is entered. If none is supplied [`EventLoop::Break`] will be
    /// returned.
    ///
    /// [`EventLoop::Break`]: crate::line::EventLoop
    pub fn with_custom_quit_command(mut self, quit_cmd: &'a str) -> Self {
        self.custom_quit = Some(quit_cmd);
        self
    }
}

impl<Ctx, W: Write> ReplBuilder<'_, Ctx, W> {
    /// Specify a starting size the the terminal should be set to on [`build`] if no size is supplied then
    /// size is found with a call to [`terminal::size`]
    ///
    /// `size`: `(columns, rows)`  
    /// The top left cell is represented `(1, 1)`.
    ///
    /// [`build`]: Self::build
    /// [`terminal::size`]: <https://docs.rs/crossterm/latest/crossterm/terminal/fn.size.html>
    pub fn with_size(mut self, size: (u16, u16)) -> Self {
        self.term_size = Some(size);
        self
    }

    // MARK: TODO
    // add documentation
    pub fn with_completion(mut self, completion: &'static CommandScheme) -> Self {
        self.completion = Some(completion);
        self
    }

    /// Disables line stylization
    pub fn without_line_stylization(mut self) -> Self {
        self.style_enabled = false;
        self
    }

    /// Supply a default prompt the line should display, if none is supplied `'>'` is used.
    pub fn with_prompt(mut self, prompt: &str) -> Self {
        self.prompt = Some(String::from(prompt.trim()));
        self
    }

    /// Supply a custom prompt separator to override the default prompt separator `'>'`.
    pub fn with_custom_prompt_separator(mut self, separator: &str) -> Self {
        self.prompt_end = Some(String::from(separator.trim()));
        self
    }

    /// Supply history entries that the repl should start with. The end of the given `entries` slice will
    /// be the most recent.
    pub fn with_history_entries<S: AsRef<str>>(mut self, entries: &[S]) -> Self {
        self.starting_history = Some(History::from_iter(entries));
        self
    }

    /// Sets the [`ParseErrHook`] that gets called when library default run eval process loops encounter a [`ParseErr`].\
    /// By default `ParseErr`s are displayed via a call to [`Repl::eprintln`], this behavior will be overwritten
    /// by setting a custom parse error hook.
    ///
    /// [`ParseErr`]: crate::line::ParseErr
    pub fn with_custom_parse_err_hook(mut self, hook: impl ParseErrHook<Ctx, W>) -> Self {
        self.parse_err_hook = Some(Box::new(hook));
        self
    }

    /// Builds a [`Repl`] that you can manually turn into a repl or call [`run`] / [`spawn`]
    /// on to start or spawn the repl process
    ///
    /// This function can return an `Err` if
    /// - The supplied terminal writer does not accept crossterm commands
    /// - No terminal size was provided and a call to [`terminal::size`] returns `Err`
    /// - A custom quit command was supplied and the string contained mismatched quotes
    ///
    /// This function will panic if an ill formed [`&'static CommandScheme`] was supplied
    ///
    /// [`run`]: crate::line::Repl::run
    /// [`spawn`]: crate::line::Repl::spawn
    /// [`&'static CommandScheme`]: crate::completion::CommandScheme
    /// [`terminal::size`]: <https://docs.rs/crossterm/latest/crossterm/terminal/fn.size.html>
    pub fn build(mut self) -> io::Result<Repl<Ctx, W>> {
        let term_size = match self.term_size {
            Some((columns, rows)) => {
                self.term.queue(terminal::SetSize(columns, rows))?;
                (columns, rows)
            }
            None => terminal::size()?,
        };
        let custom_quit = match self.custom_quit {
            Some(quit_cmd) => Some(shellwords_split(quit_cmd).map_err(|_| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("Custom quit command: {quit_cmd}, contains mismatched quotes"),
                )
            })?),
            None => None,
        };
        let completion = self.completion.map(Completion::from).unwrap_or_default();

        crossterm::terminal::enable_raw_mode()?;
        self.term.queue(cursor::EnableBlinking)?;

        Ok(Repl::from(
            LineData::new(
                self.prompt,
                self.prompt_end,
                self.style_enabled,
                !completion.is_empty(),
            ),
            self.term,
            term_size,
            custom_quit,
            completion,
            self.starting_history,
            self.parse_err_hook,
        ))
    }
}
