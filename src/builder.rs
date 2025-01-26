use crate::{
    completion::{CommandScheme, Completion},
    line::{LineData, LineReader},
};

use crossterm::{cursor, QueueableCommand};
use shellwords::split as shellwords_split;

use std::io::{self, ErrorKind, Write};

/// Access through [`repl_builder`]
pub struct LineReaderBuilder<'a, W: Write> {
    completion: Option<&'static CommandScheme>,
    custom_quit: Option<&'a str>,
    term: Option<W>,
    term_size: Option<(u16, u16)>,
    prompt: Option<String>,
    prompt_end: Option<&'static str>,
}

/// Builder for [`LineReader`]
pub fn repl_builder<W: Write>() -> LineReaderBuilder<'static, W> {
    LineReaderBuilder {
        completion: None,
        custom_quit: None,
        term: None,
        term_size: None,
        prompt: None,
        prompt_end: None,
    }
}

impl<'a, W: Write> LineReaderBuilder<'a, W> {
    /// Supply a custom command to be executed when the user tries to quit with 'ctrl + c' when the current
    /// line is empty. If none is supplied `EventLoop::Break` will be returned.
    pub fn with_custom_quit_command(mut self, quit_cmd: &'a str) -> Self {
        self.custom_quit = Some(quit_cmd);
        self
    }
}

impl<W: Write> LineReaderBuilder<'_, W> {
    /// `LineReader` must include a terminal that is compatable with executing commands via the
    /// `crossterm` crate.
    pub fn terminal(mut self, term: W) -> Self {
        self.term = Some(term);
        self
    }

    /// `LineReader` must include a terminal size so rendering the window is displayed correctly.  
    /// `size`: (columns, rows)
    pub fn terminal_size(mut self, size: (u16, u16)) -> Self {
        self.term_size = Some(size);
        self
    }

    // MARK: TODO
    // add documentation
    pub fn with_completion(mut self, completion: &'static CommandScheme) -> Self {
        self.completion = Some(completion);
        self
    }

    /// Supply a default prompt the line should display, if none is supplied '>' is used.
    pub fn with_prompt(mut self, prompt: String) -> Self {
        self.prompt = Some(prompt);
        self
    }

    /// Supply a custom prompt separator to override the default prompt separator "> ".
    pub fn with_custom_prompt_separator(mut self, separator: &'static str) -> Self {
        self.prompt_end = Some(separator);
        self
    }

    pub fn build<Ctx>(self) -> io::Result<LineReader<Ctx, W>> {
        let mut term = self
            .term
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "terminal is required"))?;
        let term_size = self
            .term_size
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "terminal size is required"))?;
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
        term.queue(cursor::EnableBlinking)?;

        Ok(LineReader::new(
            LineData::new(self.prompt, self.prompt_end, !completion.is_empty()),
            term,
            term_size,
            custom_quit,
            completion,
        ))
    }
}
