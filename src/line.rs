use crate::completion::{Completion, Direction};
use crossterm::{
    cursor,
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::Stylize,
    terminal::{Clear, ClearType::FromCursorDown},
    QueueableCommand,
};
use shellwords::split as shellwords_split;
use std::{
    borrow::Cow,
    collections::VecDeque,
    fmt::Display,
    future::Future,
    io::{self, Write},
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};
use strip_ansi::strip_ansi;
use tokio::time::{timeout, Duration};
use tokio_stream::StreamExt;

// MARK: TODOS
// 1. Add a .run() method to LineReader that is a awaitable for users who just need a basic REPL
// 2. Add a .background_run::<T: Print>() method that is spawnable and gives the user access to a Sender<T> to print background messages
// 3. Make the basic use cases as easy to set up as possible
// 4. Finish docs + create README.md + add examples for all use cases

pub type InputEventHook<Ctx, W> =
    dyn Fn(&mut LineReader<Ctx, W>, Event) -> io::Result<HookedEvent<Ctx>>;
pub type InitLineCallback<Ctx, W> = dyn FnOnce(&mut LineReader<Ctx, W>) -> io::Result<()>;
pub type Callback<Ctx> = dyn Fn(&mut Ctx) -> Result<(), InputHookErr>;
pub type AsyncCallback<Ctx> =
    dyn for<'a> FnOnce(&'a mut Ctx) -> Pin<Box<dyn Future<Output = Result<(), InputHookErr>> + 'a>>;

pub(crate) const PROMPT_END: &str = "> ";
const DEFAULT_PROMPT: &str = ">";

/// Allows the library user to decide how background messages should be printed
pub trait Print {
    fn print(&self);
}

/// Holds all context for REPL events
pub struct LineReader<Ctx, W: Write> {
    pub(crate) completion: Completion,
    pub(crate) line: LineData,
    history: History,
    term: W,
    /// (columns, rows)
    term_size: (u16, u16),
    uneventful: bool,
    custom_quit: Option<Vec<String>>,
    cursor_at_start: bool,
    command_entered: bool,
    input_hooks: VecDeque<InputHook<Ctx, W>>,
}

impl<Ctx, W: Write> Drop for LineReader<Ctx, W> {
    fn drop(&mut self) {
        crossterm::terminal::disable_raw_mode().expect("enabled on creation")
    }
}

/// `InputHook` gives you access to customize how `crossterm::event:Event`'s are processed and how the
/// [`LineReader`] behaves.
pub struct InputHook<Ctx, W: Write> {
    uid: HookUID,
    init: Option<Box<InitLineCallback<Ctx, W>>>,
    on_callback_err: Option<Box<Callback<Ctx>>>,
    event_hook: Box<InputEventHook<Ctx, W>>,
}

static CALLBACK_UID: AtomicUsize = AtomicUsize::new(0);

/// Unique identifier used to link [`InputHook`]'s to the `Callback`'s they spawn
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct HookUID(usize);

impl HookUID {
    #[inline]
    pub fn new() -> Self {
        Self(CALLBACK_UID.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for HookUID {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// The error type callbacks must return
#[derive(Debug)]
pub struct InputHookErr {
    uid: HookUID,
    err: Cow<'static, str>,
}

impl InputHookErr {
    #[inline]
    pub fn new<T: Into<Cow<'static, str>>>(uid: HookUID, err: T) -> Self {
        InputHookErr {
            uid,
            err: err.into(),
        }
    }
}

impl Display for InputHookErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.err)
    }
}

impl<Ctx, W: Write> InputHook<Ctx, W> {
    /// For use when creating an `InputHook` that contains a callback that can error, else use
    /// [`with_new_uid`](Self::with_new_uid). Ensure that the `InputHook` and [`InputHookErr`] share the
    /// same [`HookUID`] obtained through [`HookUID::new`].
    pub fn new(
        uid: HookUID,
        init: Option<Box<InitLineCallback<Ctx, W>>>,
        on_callback_err: Option<Box<Callback<Ctx>>>,
        event_hook: Box<InputEventHook<Ctx, W>>,
    ) -> Self {
        assert!(uid.0 < CALLBACK_UID.load(Ordering::SeqCst));
        InputHook {
            uid,
            init,
            on_callback_err,
            event_hook,
        }
    }

    /// For use when creating an `InputHook` that does not contain a callback that can error, else use
    /// [`new`](Self::new).
    pub fn with_new_uid(
        init: Option<Box<InitLineCallback<Ctx, W>>>,
        on_callback_err: Option<Box<Callback<Ctx>>>,
        event_hook: Box<InputEventHook<Ctx, W>>,
    ) -> Self {
        InputHook {
            uid: HookUID::new(),
            init,
            on_callback_err,
            event_hook,
        }
    }
}

#[derive(Default)]
pub(crate) struct LineData {
    inital_prompt: String,
    pub(crate) prompt: String,
    pub(crate) prompt_separator: &'static str,
    prompt_len: u16,
    pub(crate) input: String,
    len: u16,
    pub(crate) comp_enabled: bool,
    pub(crate) err: bool,
}

impl LineData {
    pub(crate) fn new(
        prompt: Option<String>,
        prompt_separator: Option<&'static str>,
        completion_enabled: bool,
    ) -> Self {
        let prompt = prompt.unwrap_or_else(|| String::from(DEFAULT_PROMPT));
        LineData {
            prompt_len: LineData::prompt_len(&prompt),
            prompt_separator: prompt_separator.unwrap_or(PROMPT_END),
            inital_prompt: prompt.clone(),
            prompt,
            comp_enabled: completion_enabled,
            ..Default::default()
        }
    }

    #[inline]
    fn prompt_len(prompt: &str) -> u16 {
        let stripped = strip_ansi(prompt);
        stripped.chars().count() as u16 + PROMPT_END.chars().count() as u16
    }

    #[inline]
    pub(crate) fn found_err(&mut self, found: bool) {
        self.err = found
    }
}

#[derive(Default)]
struct History {
    temp_top: String,
    prev_entries: Vec<String>,
    curr_index: usize,
}

// MARK: TODO
// Add support for a movable cursor
// currently `CompletionState` only supports char events at line end
// `CompletionState` will have to be carefully mannaged if cursor is moveable

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

/// Marker to tell [`process_input_event`](LineReader::process_input_event) to keep the [`InputEventHook`]
/// active (`Continue`), or to drop it and run [`return_to_initial_state`](LineReader::return_to_initial_state)
/// (`Release`).
pub enum HookControl {
    Continue,
    Release,
}

/// Return type of [`InputEventHook`]. Contains ouput information for custom event processing.
///
/// All `HookedEvent` constructors can not fail. They are always wrapped in `Ok` to reduce boilerplate
pub struct HookedEvent<Ctx> {
    event: EventLoop<Ctx>,
    new_state: HookControl,
}

impl<Ctx> HookedEvent<Ctx> {
    /// Constructor can not fail, output is wrapped in `Ok` to reduce boilerplate
    #[inline]
    pub fn new(event: EventLoop<Ctx>, new_state: HookControl) -> io::Result<Self> {
        Ok(Self { event, new_state })
    }

    /// Will tell the main loop to continue and keep the current [`InputEventHook`] active.  
    /// Constructor can not fail, output is wrapped in `Ok` to reduce boilerplate
    #[inline]
    pub fn continue_hook() -> io::Result<Self> {
        Ok(Self {
            event: EventLoop::Continue,
            new_state: HookControl::Continue,
        })
    }

    /// Will tell the main loop to continue and drop the current [`InputEventHook`].  
    /// Constructor can not fail, output is wrapped in `Ok` to reduce boilerplate
    #[inline]
    pub fn release_hook() -> io::Result<Self> {
        Ok(Self {
            event: EventLoop::Continue,
            new_state: HookControl::Release,
        })
    }
}

/// The `EventLoop` enum acts as a control router for how your main loops code should react to input events.
/// It provides mutable access back to your `Ctx` both synchronously and asynchronously. If your callback
/// can error the [`conditionally_remove_hook`](LineReader::conditionally_remove_hook) method can restore
/// the intial state of the `LineReader` as well as remove the queued input hook that was responsible for
/// spawning the callback that resulted in an error.  
///
/// `TryProcessInput` uses `shellwords::split` to parse user input into common shell tokens.
pub enum EventLoop<Ctx> {
    Continue,
    Break,
    AsyncCallback(Box<AsyncCallback<Ctx>>),
    Callback(Box<Callback<Ctx>>),
    TryProcessInput(Result<Vec<String>, ParseErr>),
}

impl<Ctx, W: Write> LineReader<Ctx, W> {
    #[inline]
    pub(crate) fn new(
        line: LineData,
        term: W,
        term_size: (u16, u16),
        custom_quit: Option<Vec<String>>,
        completion: Completion,
    ) -> Self {
        LineReader {
            line,
            history: History::default(),
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

    /// It is recommended to call this method at the top of your main loop see: [`render`](Self::render)  
    /// This method will insure all user input events are disregarded when a command is being processed
    pub async fn clear_unwanted_inputs(
        &mut self,
        stream: &mut crossterm::event::EventStream,
    ) -> io::Result<()> {
        if !std::mem::take(&mut self.command_entered) {
            return Ok(());
        }

        let _ = timeout(Duration::from_millis(10), async {
            while stream.fuse().next().await.is_some() {}
        })
        .await;
        self.term.queue(cursor::Show)?;
        Ok(())
    }

    /// Returns the state to display the initially set prompt and completion state
    pub fn return_to_initial_state(&mut self) {
        if self.line.prompt != self.line.inital_prompt {
            self.set_prompt(self.line.inital_prompt.clone());
        }
        self.enable_completion();
    }

    /// Makes sure the current `input_hook`'s initializer has been executed
    fn try_init_input_hook(&mut self) -> Option<io::Result<()>> {
        let callback = self.input_hooks.front_mut()?;
        let init = callback.init.take()?;
        Some(init(self))
    }

    /// Queues an [`InputHook`] for execution
    #[inline]
    pub fn register_input_hook(&mut self, input_hook: InputHook<Ctx, W>) {
        self.input_hooks.push_back(input_hook);
    }

    /// Removes the currently active input hook if its UID matches the UID of the provided error, then returns
    /// the [`InputHook`]'s `on_callback_err` if one was set.
    ///
    /// Eg:
    /// ```ignore
    /// Ok(EventLoop::AsyncCallback(callback)) => {
    ///     if let Err(err) = callback(&mut command_context).await {
    ///         eprintln!("{err}");
    ///         if let Some(on_err_callback) = line_handle.conditionally_remove_hook(&err) {
    ///             on_err_callback(&mut command_context).unwrap_or_else(|err| eprintln!("{err}"))
    ///         };
    ///     }
    /// },
    /// ```
    pub fn conditionally_remove_hook(&mut self, err: &InputHookErr) -> Option<Box<Callback<Ctx>>> {
        if self
            .next_input_hook()
            .is_some_and(|hook| hook.uid == err.uid)
        {
            self.return_to_initial_state();
            return self
                .pop_input_hook()
                .expect("`next_input_hook` & `pop_input_hook` both look at first queued hook")
                .on_callback_err;
        }
        None
    }

    /// Pops the first queued `input_hook`
    #[inline]
    pub fn pop_input_hook(&mut self) -> Option<InputHook<Ctx, W>> {
        self.input_hooks.pop_front()
    }

    /// References the first queued `input_hook`
    #[inline]
    pub fn next_input_hook(&mut self) -> Option<&InputHook<Ctx, W>> {
        self.input_hooks.front()
    }

    /// Makes sure background messages are displayed properly
    pub fn print_background_msg(&mut self, msg: impl Print) -> io::Result<()> {
        let res = self.move_to_beginning(self.line_len());
        msg.print();
        res
    }

    /// Returns if completion is currently enabled
    #[inline]
    pub fn completion_enabled(&self) -> bool {
        self.line.comp_enabled
    }

    /// Enables completion as long as the set [`CommandScheme`](crate::completion::CommandScheme) is not empty
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

    /// Sets the currently displayed prompt, prompt will revert back to initial state if an [`InputHook`] is
    /// [`Released`](HookControl)
    pub fn set_prompt(&mut self, prompt: String) {
        self.line.prompt_len = LineData::prompt_len(&prompt);
        self.line.prompt = prompt;
    }

    /// Returns a reference to the current user input
    #[inline]
    pub fn input(&self) -> &str {
        &self.line.input
    }

    /// Calls `mem::take` on the user input
    #[inline]
    pub fn take_input(&mut self) -> String {
        self.line.len = 0;
        self.line.err = false;
        std::mem::take(&mut self.line.input)
    }

    /// Gets the number of lines wrapped
    #[inline]
    pub fn line_height(&self, line_len: u16) -> u16 {
        line_len / self.term_size.0
    }

    /// Gets the total length of the line (prompt + user input)
    #[inline]
    pub fn line_len(&self) -> u16 {
        self.line.prompt_len.saturating_add(self.line.len)
    }

    #[inline]
    fn line_remainder(&self, line_len: u16) -> u16 {
        line_len % self.term_size.0
    }

    pub(crate) fn move_to_beginning(&mut self, from: u16) -> io::Result<()> {
        let line_height = self.line_height(from);
        if line_height != 0 {
            self.term.queue(cursor::MoveUp(line_height))?;
        }
        self.term
            .queue(cursor::MoveToColumn(0))?
            .queue(Clear(FromCursorDown))?;

        self.cursor_at_start = true;
        Ok(())
    }

    fn move_to_line_end(&mut self, line_len: u16) -> io::Result<()> {
        let line_remaining_len = self.line_remainder(line_len);
        if line_remaining_len == 0 {
            writeln!(self.term)?;
        }
        self.term.queue(cursor::MoveToColumn(line_remaining_len))?;
        self.cursor_at_start = false;
        Ok(())
    }

    /// Render is designed to be called at the top of your main loop  
    /// Eg:
    /// ```ignore
    /// break_if_err!(line_handle.clear_unwanted_inputs(&mut reader).await);
    /// break_if_err!(line_handle.render());
    /// ```
    /// Where:
    /// ```
    /// macro_rules! break_if_err {
    ///     ($expr:expr) => {
    ///         if let Err(err) = $expr {
    ///             eprintln!("{err}");
    ///             break;
    ///         }
    ///     };
    /// }
    /// ```
    /// A macro like `break_if_err!` can be helpful if you want to have a graceful shutdown procedure
    pub fn render(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.uneventful) {
            return Ok(());
        }
        if let Some(res) = self.try_init_input_hook() {
            res?
        };

        let line_len = self.line_len();
        if !self.cursor_at_start {
            self.move_to_beginning(line_len.saturating_sub(1))?;
        }

        write!(self.term, "{}", self.line)?;

        self.move_to_line_end(line_len)?;
        self.term.flush()
    }

    /// Setting uneventul will skip the next call to `render`
    pub fn set_unventful(&mut self) {
        self.uneventful = true
    }

    /// Pushes a char onto the input line and tries to update suggestions if completion is enabled
    pub fn insert_char(&mut self, c: char) {
        self.line.input.push(c);
        self.line.len = self.line.len.saturating_add(1);
        if self.line.comp_enabled {
            self.update_completeion();
        }
    }

    /// Pops a char from the input line and tries to update suggestions if completion is enabled
    pub fn remove_char(&mut self) -> io::Result<()> {
        self.line.input.pop();
        self.move_to_beginning(self.line_len())?;
        self.line.len = self.line.len.saturating_sub(1);
        if self.line.comp_enabled {
            self.update_completeion();
        }
        Ok(())
    }

    /// Writes the current line to the terminal and then calls [`clear_line`](Self::clear_line)
    pub fn new_line(&mut self) -> io::Result<()> {
        writeln!(self.term)?;
        self.clear_line()
    }

    /// Appends "^C" in red to the current line and writes it to the terminal and then calls
    /// [`clear_line`](Self::clear_line)
    pub fn ctrl_c_line(&mut self) -> io::Result<()> {
        writeln!(self.term, "{}", "^C".red())?;
        self.clear_line()
    }

    /// Resets the internal state of the input line, last history index, and completion suggestions
    pub fn clear_line(&mut self) -> io::Result<()> {
        self.reset_line_data();
        self.move_to_beginning(self.line_len())?;
        self.reset_completion();
        self.history.curr_index = self.history.prev_entries.len();
        Ok(())
    }

    fn reset_line_data(&mut self) {
        self.line.input.clear();
        self.line.len = 0;
        self.line.err = false;
    }

    pub(crate) fn change_line(&mut self, line: String) -> io::Result<()> {
        self.move_to_beginning(self.line_len())?;
        self.line.len = line.chars().count() as u16;
        self.line.input = line;
        Ok(())
    }

    fn enter_command(&mut self) -> io::Result<&str> {
        self.history
            .prev_entries
            .push(std::mem::take(&mut self.line.input));
        self.new_line()?;
        execute!(self.term, cursor::Hide)?;
        self.command_entered = true;

        Ok(self
            .history
            .prev_entries
            .last()
            .expect("just pushed into `prev_entries`"))
    }

    /// Changes the current line to the previous history entry if available
    pub fn history_back(&mut self) -> io::Result<()> {
        if self.history.curr_index == 0 || self.history.prev_entries.is_empty() {
            return Ok(());
        }
        if !self.history.prev_entries.contains(&self.line.input)
            && self.history.curr_index == self.history.prev_entries.len()
        {
            self.history.temp_top = std::mem::take(&mut self.line.input);
        }
        self.history.curr_index -= 1;
        self.change_line(self.history.prev_entries[self.history.curr_index].clone())
    }

    /// Changes the current line to the next history entry if available
    pub fn history_forward(&mut self) -> io::Result<()> {
        if self.history.curr_index == self.history.prev_entries.len() {
            return Ok(());
        }
        let new_line = if self.history.curr_index == self.history.prev_entries.len() - 1 {
            self.history.curr_index = self.history.prev_entries.len();
            std::mem::take(&mut self.history.temp_top)
        } else {
            self.history.curr_index += 1;
            self.history.prev_entries[self.history.curr_index].clone()
        };
        self.change_line(new_line)
    }

    /// If a [custom quit command](crate::LineReaderBuilder::with_custom_quit_command) is set this will tell the
    /// main loop to process the set command otherwise will return `EventLoop::Break`  
    pub fn process_close_signal(&mut self) -> io::Result<EventLoop<Ctx>> {
        let Some(quit_cmd) = self.custom_quit.clone() else {
            return Ok(EventLoop::Break);
        };
        execute!(self.term, cursor::Hide)?;
        self.command_entered = true;
        Ok(EventLoop::TryProcessInput(Ok(quit_cmd)))
    }

    /// The main control flow for awaited events from a `crossterm::event::EventStream`. Works well as its
    /// own branch in a `tokio::select!`.
    ///
    /// Example main loop assuming we have a `Ctx`, `command_context`,  that implements [`Executor`](crate::Executor)
    ///
    /// ```ignore
    /// let mut reader = crossterm::event::EventStream::new();
    /// let mut line_handle = LineReaderBuilder::new()
    ///     .terminal(std::io::stdout())
    ///     .terminal_size(crossterm::terminal::size()?)
    ///     .build()
    ///     .expect("all required inputs are provided & terminal accepts crossterm commands");
    ///
    /// loop {
    ///     line_handle.clear_unwanted_inputs(&mut reader).await?;
    ///     line_handle.render()?;
    ///
    ///     if let Some(event_result) = reader.next().await {
    ///         match line_handle.process_input_event(event_result?) {
    ///             Ok(EventLoop::Continue) => (),
    ///             Ok(EventLoop::Break) => break,
    ///             Ok(EventLoop::Callback(callback)) => {
    ///                 if let Err(err) = callback(&mut command_context) {
    ///                     eprintln!("{err}");
    ///                     if let Some(on_err_callback) = line_handle.conditionally_remove_hook(&err) {
    ///                         on_err_callback(&mut command_context).unwrap_or_else(|err| eprintln!("{err}"))
    ///                     };
    ///                 }
    ///             },
    ///             Ok(EventLoop::AsyncCallback(callback)) => {
    ///                 if let Err(err) = callback(&mut command_context).await {
    ///                     eprintln!("{err}");
    ///                     if let Some(on_err_callback) = line_handle.conditionally_remove_hook(&err) {
    ///                         on_err_callback(&mut command_context).unwrap_or_else(|err| eprintln!("{err}"))
    ///                     };
    ///                 }
    ///             },
    ///             Ok(EventLoop::TryProcessInput(Ok(user_tokens))) => {
    ///                 match command_context.try_execute_command(user_tokens).await {
    ///                     CommandHandle::Processed => (),
    ///                     CommandHandle::InsertHook(input_hook) => line_handle.register_input_hook(input_hook),
    ///                     CommandHandle::Exit => break,
    ///                 }
    ///             }
    ///             Ok(EventLoop::TryProcessInput(Err(mismatched_quotes))) => {
    ///                 eprintln!("{mismatched_quotes}")
    ///             },
    ///             Err(err) => {
    ///                 eprintln!("{err}");
    ///                 break;
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn process_input_event(&mut self, event: Event) -> io::Result<EventLoop<Ctx>> {
        if !self.input_hooks.is_empty() {
            if let Event::Key(KeyEvent {
                kind: KeyEventKind::Press,
                ..
            }) = event
            {
                let hook = self.pop_input_hook().expect("outer if");
                debug_assert!(hook.init.is_none());
                let hook_output = (hook.event_hook)(self, event)?;
                match hook_output.new_state {
                    HookControl::Continue => self.input_hooks.push_front(hook),
                    HookControl::Release => self.return_to_initial_state(),
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
                let line_was_empty = self.line.input.is_empty();
                self.ctrl_c_line()?;
                if line_was_empty {
                    return self.process_close_signal();
                }
                Ok(EventLoop::Continue)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                kind: KeyEventKind::Press,
                modifiers: KeyModifiers::CONTROL,
                ..
            }) => {
                self.clear_line()?;
                self.process_close_signal()
            }
            Event::Key(KeyEvent {
                code: KeyCode::Tab,
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.try_completion(Direction::Next)?;
                Ok(EventLoop::Continue)
            }
            Event::Key(KeyEvent {
                code: KeyCode::BackTab,
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.try_completion(Direction::Previous)?;
                Ok(EventLoop::Continue)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.insert_char(c);
                Ok(EventLoop::Continue)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.remove_char()?;
                Ok(EventLoop::Continue)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Up,
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.history_back()?;
                Ok(EventLoop::Continue)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                kind: KeyEventKind::Press,
                ..
            }) => {
                self.history_forward()?;
                Ok(EventLoop::Continue)
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                kind: KeyEventKind::Press,
                ..
            }) => {
                if self.line.input.trim().is_empty() {
                    self.new_line()?;
                    return Ok(EventLoop::Continue);
                }
                Ok(EventLoop::TryProcessInput(
                    shellwords_split(self.enter_command()?).map_err(|_| ParseErr::MismatchedQuotes),
                ))
            }
            Event::Resize(x, y) => {
                self.term_size = (x, y);
                Ok(EventLoop::Continue)
            }
            _ => {
                self.uneventful = true;
                Ok(EventLoop::Continue)
            }
        }
    }
}
