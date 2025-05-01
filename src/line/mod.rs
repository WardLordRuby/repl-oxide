mod builder;
mod history;
mod print;
pub(crate) mod style;

/// Collection of types used for auto completion of user input
pub mod completion;

/// Collection of types used for custom control over the EventStream
pub mod input_hook;

pub use builder::*;
pub use print::*;

use crate::line::{
    completion::{Completion, Direction},
    history::History,
    input_hook::{AsyncCallback, HookControl, InputHook},
    style::ansi_code::{DIM_WHITE, RED, RESET},
};

use std::{
    collections::VecDeque,
    fmt::Display,
    io::{self, Write},
};

use constcat::concat;
use crossterm::{
    cursor,
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::Print,
    terminal::{
        BeginSynchronizedUpdate, Clear, ClearType::FromCursorDown, EndSynchronizedUpdate, SetSize,
    },
    QueueableCommand,
};
use shellwords::split as shellwords_split;
use strip_ansi::strip_ansi;
use tokio::time::{timeout, Duration};
use tokio_stream::StreamExt;

// MARK: TODOS
// 1. Make the basic use cases as easy to set up as possible
// 2. Create example for `Callback` and `AsyncCallback`
// 3. Add docs for completion
// 4. Finish doc todos + create README.md

const DEFAULT_SEPARATOR: &str = ">";
const DEFAULT_PROMPT: &str = ">";

// .len() here is only ok since we use all chars that are 1 byte
// The '+ 1' is accounting for the space character located in our impl `Display` for `Self`
const DEFAULT_PROMPT_LEN: u16 = DEFAULT_PROMPT.len() as u16 + DEFAULT_SEPARATOR.len() as u16 + 1;

const NEW_LINE: &str = "\r\n";

/// Holds all context for REPL events
pub struct Repl<Ctx, W: Write> {
    completion: Completion,
    line: LineData,
    history: History,
    ghost_text: Option<GhostTextMeta>,
    term: W,
    /// (columns, rows)
    term_size: (u16, u16),
    uneventful: bool,
    custom_quit: Option<Vec<String>>,
    cursor_at_start: bool,
    command_entered: bool,
    input_hooks: VecDeque<InputHook<Ctx, W>>,
}

impl<Ctx, W: Write> Drop for Repl<Ctx, W> {
    fn drop(&mut self) {
        execute!(self.term, cursor::Show).expect("Still accepting commands");
        crossterm::terminal::disable_raw_mode().expect("enabled on creation");
    }
}

#[derive(Default)]
struct LineData {
    prompt: String,
    prompt_separator: String,
    input: String,
    comp_enabled: bool,
    style_enabled: bool,
    err: bool,
    len: u16,
    prompt_len: u16,
}

impl LineData {
    fn new(
        prompt: Option<String>,
        prompt_separator: Option<String>,
        style_enabled: bool,
        completion_enabled: bool,
    ) -> Self {
        let prompt = prompt.unwrap_or_else(|| String::from(DEFAULT_PROMPT));
        let prompt_separator = prompt_separator.unwrap_or_else(|| String::from(DEFAULT_SEPARATOR));
        LineData {
            prompt_len: LineData::prompt_len(&prompt, &prompt_separator),
            prompt_separator,
            prompt,
            style_enabled,
            comp_enabled: completion_enabled,
            ..Default::default()
        }
    }

    #[inline]
    fn update_prompt_len(&mut self) {
        self.prompt_len = Self::prompt_len(&self.prompt, &self.prompt_separator)
    }

    fn prompt_len(prompt: &str, separator: &str) -> u16 {
        // The '+ 1' is accounting for the space character located in our impl `Display` for `Self`
        strip_ansi(prompt).chars().count() as u16 + strip_ansi(separator).chars().count() as u16 + 1
    }
}

#[derive(Clone, Copy)]
enum GhostTextMeta {
    History { pos: usize },
    Recommendation { len: usize },
}

// MARK: TODO
// Add support for a movable cursor
// currently `CompletionState` only supports char events at line end
// `CompletionState` will have to be carefully managed if cursor is moveable

/// Error type for parsing user input into tokens
#[non_exhaustive]
pub enum ParseErr {
    MismatchedQuotes,
}

impl Display for ParseErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ParseErr::MismatchedQuotes => "Mismatched quotes",
            }
        )
    }
}

/// Communicates to the REPL how it should react to input events
///
/// `EventLoop` enum acts as a control router for how your read eval print loop should react to input events.
/// It provides mutable access back to your `Ctx` both synchronously and asynchronously. If your callback
/// can error the [`remove_current_hook_by_error`] method can restore the initial state of the `Repl` as
/// well as remove the queued input hook that was responsible for spawning the callback that resulted in an
/// error.  
///
/// `TryProcessInput` uses [`shellwords::split`] to parse user input into common shell tokens.
///
/// [`remove_current_hook_by_error`]: Repl::remove_current_hook_by_error
/// [`shellwords::split`]: <https://docs.rs/shell-words/latest/shell_words/fn.split.html>
pub enum EventLoop<Ctx, W: Write> {
    Continue,
    Break,
    AsyncCallback(Box<dyn AsyncCallback<Ctx, W>>),
    TryProcessInput(Result<Vec<String>, ParseErr>),
}

impl<Ctx, W: Write> Repl<Ctx, W> {
    #[inline]
    fn new(
        line: LineData,
        term: W,
        term_size: (u16, u16),
        custom_quit: Option<Vec<String>>,
        completion: Completion,
        history: Option<History>,
    ) -> Self {
        Self {
            line,
            history: history.unwrap_or_default(),
            ghost_text: None,
            term,
            term_size,
            uneventful: false,
            cursor_at_start: false,
            command_entered: true,
            custom_quit,
            completion,
            input_hooks: VecDeque::new(),
        }
    }

    /// It is recommended to call this method at the top of your read eval print loop see: [`render`]
    /// This method will insure all user input events are disregarded when a command is being processed
    ///
    /// [`render`]: Self::render
    pub async fn clear_unwanted_inputs(
        &mut self,
        stream: &mut crossterm::event::EventStream,
    ) -> io::Result<()> {
        if !self.command_entered {
            return Ok(());
        }

        let Ok(res) = timeout(Duration::from_millis(10), async {
            while let Some(event_res) = stream.fuse().next().await {
                if let Event::Resize(x, y) = event_res? {
                    self.term_size = (x, y)
                }
            }
            Ok(())
        })
        .await
        else {
            return Ok(());
        };
        res
    }

    /// Get an exclusive reference to the supplied writer for the rare cases you want to manually write into
    /// it. Prefer: [`Repl::println`], [`Repl::eprintln`], or [`Repl::print_lines`] as they handle all the
    /// nuances for you.
    #[inline]
    pub fn writer(&mut self) -> &mut W {
        self.term.by_ref()
    }

    /// Returns if completion is currently enabled
    #[inline]
    pub fn completion_enabled(&self) -> bool {
        self.line.comp_enabled
    }

    /// Enables completion as long as the set [`CommandScheme`] is not empty
    ///
    /// [`CommandScheme`]: crate::completion::CommandScheme
    #[inline]
    pub fn enable_completion(&mut self) {
        if self.completion.is_empty() {
            return;
        }
        self.line.comp_enabled = true
    }

    /// Disables completion
    #[inline]
    pub fn disable_completion(&mut self) {
        self.line.comp_enabled = false
    }

    /// Returns if line stylization is currently enabled
    #[inline]
    pub fn line_stylization_enabled(&self) -> bool {
        self.line.style_enabled
    }

    /// Enables line stylization
    #[inline]
    pub fn enable_line_stylization(&mut self) {
        self.line.style_enabled = true
    }

    /// Disables line stylization
    #[inline]
    pub fn disable_line_stylization(&mut self) {
        self.line.style_enabled = false
    }

    /// Sets the currently displayed prompt
    pub fn set_prompt(&mut self, prompt: &str) {
        self.line.prompt = String::from(prompt.trim());
        self.line.update_prompt_len();
    }

    /// Sets the currently displayed prompt separator  
    pub fn set_prompt_separator(&mut self, prompt_separator: &str) {
        self.line.prompt_separator = String::from(prompt_separator.trim());
        self.line.update_prompt_len();
    }

    /// Sets the currently displayed prompt and prompt separator  
    pub fn set_prompt_and_separator(&mut self, prompt: &str, prompt_separator: &str) {
        self.line.prompt = String::from(prompt.trim());
        self.line.prompt_separator = String::from(prompt_separator.trim());
        self.line.update_prompt_len();
    }

    /// Sets the currently displayed prompt to the library supplied default
    pub fn set_default_prompt(&mut self) {
        self.line.prompt = String::from(DEFAULT_PROMPT);
        self.line.update_prompt_len();
    }

    /// Sets the currently displayed prompt and prompt separator to the library supplied default
    pub fn set_default_prompt_and_separator(&mut self) {
        self.line.prompt = String::from(DEFAULT_PROMPT);
        self.line.prompt_separator = String::from(DEFAULT_SEPARATOR);
        self.line.prompt_len = DEFAULT_PROMPT_LEN;
    }

    /// Returns a reference to the current user input
    #[inline]
    pub fn input(&self) -> &str {
        &self.line.input
    }

    /// Appends a given string slice to the end of the currently displayed input line
    pub fn append_to_line(&mut self, new: &str) -> io::Result<()> {
        self.line.input.push_str(new);
        self.line.len += new.chars().count() as u16;
        self.move_to_end(self.line_len())?;
        self.update_completion();
        Ok(())
    }

    /// Gets the number of lines wrapped
    #[inline]
    fn line_height(&self, line_len: u16) -> u16 {
        line_len / self.term_size.0
    }

    /// Gets the total length of the line (prompt + user input)
    #[inline]
    fn line_len(&self) -> u16 {
        self.line.prompt_len.saturating_add(self.line.len)
    }

    #[inline]
    fn line_remainder(&self, line_len: u16) -> u16 {
        line_len % self.term_size.0
    }

    fn move_to_beginning(&mut self, from: u16) -> io::Result<()> {
        let line_height = self.line_height(from);
        if line_height != 0 {
            self.term.queue(cursor::MoveUp(line_height))?;
        }
        self.term.queue(cursor::MoveToColumn(0))?;
        self.cursor_at_start = true;
        Ok(())
    }

    fn move_to_end(&mut self, line_len: u16) -> io::Result<()> {
        let line_remaining_len = self.line_remainder(line_len);
        if line_remaining_len == 0 {
            self.term.queue(Print(NEW_LINE))?;
        }
        let line_height = self.line_height(line_len);
        if line_height != 0 && self.ghost_text.is_some() {
            self.term.queue(cursor::MoveDown(line_height))?;
        }
        self.term.queue(cursor::MoveToColumn(line_remaining_len))?;
        self.cursor_at_start = false;
        Ok(())
    }

    /// Render is designed to be called at the top of your read eval print loop. This method should only be
    /// used when writing a custom repl and neither [`run`] / [`spawn`] are being used.  
    ///
    /// # Example
    ///
    /// ```ignore
    /// repl.clear_unwanted_inputs(&mut reader).await?;
    /// repl.render(&mut command_context)?;
    /// ```
    /// [`run`]: crate::line::Repl::run
    /// [`spawn`]: crate::line::Repl::spawn
    pub fn render(&mut self, context: &mut Ctx) -> io::Result<()> {
        if std::mem::take(&mut self.uneventful) {
            return Ok(());
        }

        if std::mem::take(&mut self.command_entered) {
            // Always assume the worst case that the user wrote into the writer without entering a new line
            // resetting the current line should make it evident the user has a bug in there code, while the
            // library ensures to always be displaying an acceptable state
            self.cursor_at_start = false;
        }

        if let Some(res) = self.try_init_input_hook(context) {
            res?
        };

        let line_len = self.line_len();
        let line_len_sub_1 = line_len.saturating_sub(1);

        if !self.cursor_at_start {
            self.move_to_beginning(line_len_sub_1)?;
            self.term.queue(Clear(FromCursorDown))?;
        }

        self.term.queue(Print(&self.line))?;
        self.render_ghost_text(line_len_sub_1)?;

        self.move_to_end(line_len)?;
        self.term.queue(cursor::Show)?;

        execute!(self.term, EndSynchronizedUpdate)
    }

    fn render_ghost_text(&mut self, line_len_sub_1: u16) -> io::Result<()> {
        if !self.line.style_enabled || self.line.input.is_empty() {
            self.ghost_text = None;
            return Ok(());
        }

        // Render is only ran if the input state has changed, so lets try to update ghost text
        let Some((ghost_text, meta)) = self
            .history
            .iter()
            .find_map(|(p, prev)| {
                prev.strip_prefix(self.input())
                    .map(|str| (str, GhostTextMeta::History { pos: *p }))
            })
            .or_else(|| {
                let (recommendation, kind) = self
                    .completion
                    .recommendations
                    .first()
                    .map(|&rec| (rec, &self.completion.rec_data_from_index(0).kind))?;

                let format_as_arg = self.completion.arg_format(kind)?;
                let mut last_token = self
                    .input()
                    .rsplit_once(char::is_whitespace)
                    .map_or(self.input(), |(_, suf)| suf);

                if last_token.is_empty()
                    || format_as_arg
                        && !last_token.strip_prefix("--").is_some_and(|token| {
                            last_token = token;
                            token.chars().next().is_some_and(char::is_alphabetic)
                        })
                    || !format_as_arg && last_token.starts_with('-')
                {
                    return None;
                }

                recommendation
                    .strip_prefix(last_token)
                    .map(|str| (str, GhostTextMeta::Recommendation { len: str.len() }))
            })
        else {
            self.ghost_text = None;
            return Ok(());
        };

        self.ghost_text = Some(meta);
        self.term
            .queue(Print(format!("{DIM_WHITE}{ghost_text}{RESET}")))?;
        self.move_to_beginning(line_len_sub_1 + ghost_text.chars().count() as u16)
    }

    /// Setting uneventful will skip the next call to `render`
    #[inline]
    pub fn set_uneventful(&mut self) {
        self.uneventful = true
    }

    /// Returns if uneventful is currently set
    #[inline]
    pub fn uneventful(&self) -> bool {
        self.uneventful
    }

    /// Pushes a char onto the input line and tries to update suggestions if completion is enabled
    pub fn insert_char(&mut self, c: char) {
        self.line.input.push(c);
        self.line.len = self.line.len.saturating_add(1);
        self.update_completion();
    }

    /// Pops a char from the input line and tries to update suggestions if completion is enabled
    pub fn remove_char(&mut self) -> io::Result<()> {
        self.line.input.pop();
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        self.line.len = self.line.len.saturating_sub(1);
        self.update_completion();
        Ok(())
    }

    /// Writes the current line to the terminal and returns the user input of the line
    pub fn new_line(&mut self) -> io::Result<String> {
        self.term
            .queue(Clear(FromCursorDown))?
            .queue(Print(NEW_LINE))?;
        self.cursor_at_start = true;
        Ok(self.reset_line_state())
    }

    /// Appends "^C" (color coded if style is enabled) to the current line, writes it to the terminal,
    /// and returns the user input of the line
    pub fn ctrl_c_line(&mut self) -> io::Result<String> {
        self.term
            .queue(Print(if self.line.style_enabled {
                concat!(RED, "^C", RESET)
            } else {
                "^C"
            }))?
            .queue(Clear(FromCursorDown))?
            .queue(Print(NEW_LINE))?;
        self.cursor_at_start = true;
        Ok(self.reset_line_state())
    }

    /// Clears the current line and returns the user input of the line
    pub fn clear_line(&mut self) -> io::Result<String> {
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        Ok(self.reset_line_state())
    }

    /// Resets the internal state of the input line, last history index, and completion suggestions
    /// returning you an owned `String` of what was cleared.
    fn reset_line_state(&mut self) -> String {
        self.reset_completion();
        self.history.reset_idx();
        self.line.len = 0;
        self.line.err = false;
        self.ghost_text = None;
        std::mem::take(&mut self.line.input)
    }

    /// Changes the currently displayed user input to the given `line`, returning you an owned `String`
    /// of what was replaced.
    pub fn change_line(&mut self, line: String) -> io::Result<String> {
        let prev = self.change_line_raw(line)?;
        self.reset_completion();
        self.update_completion();
        Ok(prev)
    }

    /// For internal use when we **know** that we want to keep the same completion state, returning you
    /// an owned `String` of what was replaced.
    fn change_line_raw(&mut self, mut line: String) -> io::Result<String> {
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        self.line.len = line.chars().count() as u16;
        std::mem::swap(&mut self.line.input, &mut line);
        Ok(line)
    }

    /// Returns the current `(columns, rows)` that is stored in the `Repl`'s memory.
    pub fn terminal_size(&self) -> (u16, u16) {
        self.term_size
    }

    /// This method **must** be used to make modifications to the terminals size, otherwise line wrapping
    /// logic will not be in sync.
    pub fn set_terminal_size(&mut self, (columns, rows): (u16, u16)) -> io::Result<()> {
        self.term.queue(SetSize(columns, rows))?;
        self.term_size = (columns, rows);
        Ok(())
    }

    fn enter_command(&mut self) -> io::Result<&str> {
        self.term.queue(cursor::Hide)?;
        let cmd = self.new_line()?;
        self.add_to_history(&cmd);
        self.command_entered = true;

        Ok(self.history.last_entry().expect("just pushed into history"))
    }

    fn append_ghost_text(&mut self) -> io::Result<()> {
        let Some(meta) = self.ghost_text.take() else {
            self.set_uneventful();
            return Ok(());
        };

        match meta {
            GhostTextMeta::History { pos } => {
                self.change_line(
                    self.history
                        .get(&pos)
                        .expect("set meta `pos` is valid position")
                        .clone(),
                )?;
            }
            GhostTextMeta::Recommendation { len } => {
                let rec_len = self.completion.recommendations[0].len();
                let ghost_text = &self.completion.recommendations[0][rec_len - len..];
                self.append_to_line(ghost_text)?;
            }
        }

        Ok(())
    }

    /// If a [custom quit command] is set this will tell the read eval print loop to process the set command
    /// otherwise will return [`EventLoop::Break`]  
    ///
    /// [custom quit command]: crate::ReplBuilder::with_custom_quit_command
    /// [`EventLoop::Break`]: crate::line::EventLoop
    pub fn process_close_signal(&mut self) -> io::Result<EventLoop<Ctx, W>> {
        self.clear_line()?;
        self.term.queue(cursor::Hide)?;
        let Some(quit_cmd) = self.custom_quit.clone() else {
            return Ok(EventLoop::Break);
        };
        self.command_entered = true;
        Ok(EventLoop::TryProcessInput(Ok(quit_cmd)))
    }

    /// The main control flow for awaited events from a [`crossterm::event::EventStream`]. Works well as its
    /// own branch in a [`tokio::select!`].
    ///
    /// # Example
    ///
    /// Read eval print loop assuming we have a `Ctx`, `command_context`,  that implements [`Executor`]
    ///
    /// ```ignore
    /// let mut reader = crossterm::event::EventStream::new();
    /// let mut repl = repl_builder(std::io::stdout())
    ///     .build()
    ///     .expect("input writer accepts crossterm commands");
    ///
    /// loop {
    ///     repl.clear_unwanted_inputs(&mut reader).await?;
    ///     repl.render(&mut command_context)?;
    ///
    ///     if let Some(event_result) = reader.next().await {
    ///         match repl.process_input_event(&mut command_context, event_result?)? {
    ///             EventLoop::Continue => (),
    ///             EventLoop::Break => break,
    ///             EventLoop::AsyncCallback(callback) => {
    ///                 if let Err(err) = callback(&mut repl, &mut command_context).await {
    ///                     repl.eprintln(err)?;
    ///                     repl.remove_current_hook_by_error(&err)?;
    ///                 }
    ///             },
    ///             EventLoop::TryProcessInput(Ok(user_tokens)) => {
    ///                 match command_context.try_execute_command(&mut repl, user_tokens).await? {
    ///                     CommandHandle::Processed => (),
    ///                     CommandHandle::InsertHook(input_hook) => repl.register_input_hook(input_hook),
    ///                     CommandHandle::Exit => break,
    ///                 }
    ///             }
    ///             EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
    ///                 repl.eprintln(mismatched_quotes)?;
    ///             },
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// [`crossterm::event::EventStream`]: <https://docs.rs/crossterm/latest/crossterm/event/struct.EventStream.html>
    /// [`tokio::select!`]: <https://docs.rs/tokio/latest/tokio/macro.select.html>
    /// [`Executor`]: crate::executor::Executor
    pub fn process_input_event(
        &mut self,
        context: &mut Ctx,
        event: Event,
    ) -> io::Result<EventLoop<Ctx, W>> {
        execute!(self.term, BeginSynchronizedUpdate)?;

        if !self.input_hooks.is_empty() {
            if let Event::Key(KeyEvent {
                kind: KeyEventKind::Press,
                ..
            }) = event
            {
                let hook = self.pop_input_hook().expect("outer if");
                debug_assert!(hook.init_revert.init.is_none());
                let hook_output = (hook.event_hook)(self, context, event)?;
                match hook_output.new_state {
                    HookControl::Continue => self.input_hooks.push_front(hook),
                    HookControl::Release => self
                        .try_revert_input_hook(context, hook)
                        .unwrap_or(Ok(()))?,
                }
                return Ok(hook_output.event);
            }
        }
        match event {
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => {
                if self.input().is_empty() {
                    return self.process_close_signal();
                }
                self.ctrl_c_line()?;
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => return self.process_close_signal(),
            Event::Key(KeyEvent {
                code: KeyCode::Tab,
                kind: KeyEventKind::Press,
                ..
            }) => self.try_completion(Direction::Next)?,
            Event::Key(KeyEvent {
                code: KeyCode::BackTab,
                kind: KeyEventKind::Press,
                ..
            }) => self.try_completion(Direction::Previous)?,
            Event::Key(KeyEvent {
                code: KeyCode::Right,
                kind: KeyEventKind::Press,
                ..
            }) => self.append_ghost_text()?,
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                kind: KeyEventKind::Press,
                ..
            }) => self.insert_char(c),
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                ..
            }) => self.remove_char()?,
            Event::Key(KeyEvent {
                code: KeyCode::Up,
                kind: KeyEventKind::Press,
                ..
            }) => self.history_back()?,
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                kind: KeyEventKind::Press,
                ..
            }) => self.history_forward()?,
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                ..
            }) => {
                if !self.input().trim().is_empty() {
                    return Ok(EventLoop::TryProcessInput(
                        shellwords_split(self.enter_command()?)
                            .map_err(|_| ParseErr::MismatchedQuotes),
                    ));
                }
                self.new_line()?;
            }
            Event::Resize(x, y) => self.term_size = (x, y),
            Event::Paste(new) => self.append_to_line(&new)?,
            _ => self.set_uneventful(),
        }
        if self.uneventful() {
            execute!(self.term, EndSynchronizedUpdate)?;
        }
        Ok(EventLoop::Continue)
    }
}
