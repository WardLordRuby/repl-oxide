mod history;
mod style;

pub(crate) mod builder;

/// Collection of types used for auto completion of user input
pub mod completion;

use crate::{
    ansi_code::{DIM_WHITE, RED, RESET},
    callback::{AsyncCallback, HookLifecycle, InputEventHook},
    line::{
        completion::{Completion, Direction},
        history::History,
    },
};

use std::{
    borrow::Cow,
    collections::VecDeque,
    fmt::Display,
    io::{self, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

use constcat::concat;
use crossterm::{
    cursor,
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::Print,
    terminal::{BeginSynchronizedUpdate, Clear, ClearType::FromCursorDown, EndSynchronizedUpdate},
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

static HOOK_UID: AtomicUsize = AtomicUsize::new(0);

/// Holds all context for REPL events
pub struct LineReader<Ctx, W: Write> {
    pub(crate) completion: Completion,
    pub(crate) line: LineData,
    pub(crate) history: History,
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

impl<Ctx, W: Write> Drop for LineReader<Ctx, W> {
    fn drop(&mut self) {
        execute!(self.term, cursor::Show).expect("Still accepting commands");
        crossterm::terminal::disable_raw_mode().expect("enabled on creation");
    }
}

/// Queues text to be displayed on the given writer to normalize accross targets.
///
/// Replaces all new line characters with "\r\n". Supports printing multi-line strings.
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`LineReader::println`]. If you need to display text while the repl is active see:
/// [`LineReader::print_background_msg`]
///
/// If only compiling for Windows targets, the `println!` macro will display text as expected.
///
/// [`build`]: crate::builder::LineReaderBuilder::build
pub fn println<S, W>(writer: &mut W, str: S) -> io::Result<()>
where
    S: AsRef<str>,
    W: Write,
{
    writer.queue(Print(format!(
        "{}{NEW_LINE}",
        str.as_ref().replace("\n", NEW_LINE)
    )))?;
    Ok(())
}

/// Powerful type that allows customization of library default implementations
///
/// `InputHook` gives you access to customize how [`Event`]'s are processed and how the [`LineReader`]
/// behaves.
///
/// Hooks can be initialized with a [`HookLifecycle`] that allows for a place to modify the current state
/// of the [`LineReader`] and/or the users generic `Ctx`. To do so use [`new_hook_states`], note you must
/// also supply a seperate callback to revert the changes back to your desired state when the `InputHook`
/// is dropped.
///
/// Otherwise use [`no_state_change`] to not specify new and previous states.
///
/// Hooks require a [`InputEventHook`] this callback can be is entirely responsible for controlling _all_
/// reactions to [`KeyEvent`]'s of kind: [`KeyEventKind::Press`]. This will act as a manual overide of the
/// libraries event processor. You will have access to manually determine what methods are called on the
/// [`LineReader`]. See: [callbacks.rs]
///
/// [callbacks.rs]: <https://github.com/WardLordRuby/repl-oxide/blob/main/examples/callbacks.rs>
/// [`Event`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.Event.html>
/// [`KeyEvent`]: <https://docs.rs/crossterm/latest/crossterm/event/struct.KeyEvent.html>
/// [`KeyEventKind::Press`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.KeyEventKind.html>
/// [`conditionally_remove_hook`]: LineReader::conditionally_remove_hook
/// [`new_hook_states`]: InputHook::new_hook_states
/// [`no_state_change`]: InputHook::no_state_change
pub struct InputHook<Ctx, W: Write> {
    uid: HookUID,
    init_revert: HookStates<Ctx, W>,
    event_hook: Box<InputEventHook<Ctx, W>>,
}

/// Holds the constructor and deconstructor of an [`InputHook`]
///
/// Can hold 2 unique [`HookLifecycle`] callbacks. This type's constructor is a method on
/// [`InputHook::new_hook_states`]
pub struct HookStates<Ctx, W: Write> {
    init: Option<Box<HookLifecycle<Ctx, W>>>,
    revert: Option<Box<HookLifecycle<Ctx, W>>>,
}

impl<Ctx, W: Write> Default for HookStates<Ctx, W> {
    /// A Default `HookStates` will not make any modifications to the surrounding [`InputHook`] or
    /// the users generic `Ctx`
    #[inline]
    fn default() -> Self {
        Self {
            init: None,
            revert: None,
        }
    }
}

impl<Ctx, W: Write> InputHook<Ctx, W> {
    /// For use when creating an `InputHook` that contains an [`AsyncCallback`] that can error, else use
    /// [`with_new_uid`]. Ensure that the `InputHook` and [`CallbackErr`] share the same [`HookUID`]
    /// obtained through [`HookUID::new`].
    ///
    /// The library supplied repl runners ([`run`] / [`spawn`]) or event processor macro [`general_event_process`]
    /// will call [`conditionally_remove_hook`] when any callback errors. When writing your own repl it is
    /// recomended to implement this logic.  
    ///
    /// [`with_new_uid`]: Self::with_new_uid
    /// [`conditionally_remove_hook`]: LineReader::conditionally_remove_hook
    /// [`general_event_process`]: crate::general_event_process
    /// [`run`]: crate::line::LineReader::run
    /// [`spawn`]: crate::line::LineReader::spawn
    pub fn new(
        uid: HookUID,
        init_revert: HookStates<Ctx, W>,
        event_hook: Box<InputEventHook<Ctx, W>>,
    ) -> Self {
        assert!(uid.0 < HOOK_UID.load(Ordering::SeqCst));
        InputHook {
            uid,
            init_revert,
            event_hook,
        }
    }

    /// For use when creating an `InputHook` that does not contain an [`AsyncCallback`] that can error, else use
    /// [`new`].
    ///
    /// [`new`]: Self::new
    pub fn with_new_uid(
        init_revert: HookStates<Ctx, W>,
        event_hook: Box<InputEventHook<Ctx, W>>,
    ) -> Self {
        InputHook {
            uid: HookUID::new(),
            init_revert,
            event_hook,
        }
    }

    /// For use when creating an `InputHook` that doesn't need to change any state on construction or
    /// deconstruction. Equivalent to [`HookStates::default`]
    #[inline]
    pub fn no_state_change() -> HookStates<Ctx, W> {
        HookStates::default()
    }

    /// For use when creating an `InputHook` that changes the state of the [`LineReader`] or the user
    /// supplied generic `Ctx` on construction and deconstruction.
    pub fn new_hook_states(
        init: Box<HookLifecycle<Ctx, W>>,
        revert: Box<HookLifecycle<Ctx, W>>,
    ) -> HookStates<Ctx, W> {
        HookStates {
            init: Some(init),
            revert: Some(revert),
        }
    }
}

/// Unique linking identifier used for Error handling
///
/// `HookUID` links an [`InputEventHook`] to all it's spawned [`AsyncCallback`]. This provides a system for
/// dynamic [`InputHook`] termination. For more information see: [`conditionally_remove_hook`]
///
/// [`conditionally_remove_hook`]: LineReader::conditionally_remove_hook
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct HookUID(usize);

impl Default for HookUID {
    #[inline]
    fn default() -> Self {
        Self(HOOK_UID.fetch_add(1, Ordering::SeqCst))
    }
}

impl HookUID {
    /// New will always return a unique value
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
}

/// The error type callbacks must return
#[derive(Debug)]
pub struct CallbackErr {
    uid: HookUID,
    err: Cow<'static, str>,
}

impl CallbackErr {
    /// Ensure `uid` is the same [`HookUID`] you pass to [`InputHook::new`]
    #[inline]
    pub fn new<T: Into<Cow<'static, str>>>(uid: HookUID, err: T) -> Self {
        CallbackErr {
            uid,
            err: err.into(),
        }
    }
}

impl Display for CallbackErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.err)
    }
}

#[derive(Default)]
pub(crate) struct LineData {
    pub(crate) prompt: String,
    pub(crate) prompt_separator: String,
    pub(crate) input: String,
    pub(crate) comp_enabled: bool,
    pub(crate) style_enabled: bool,
    pub(crate) err: bool,
    len: u16,
    prompt_len: u16,
}

impl LineData {
    pub(crate) fn new(
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

    #[inline]
    pub(crate) fn found_err(&mut self, found: bool) {
        self.err = found
    }
}

#[derive(Clone, Copy)]
enum GhostTextMeta {
    History { p: usize },
    Recomendation { len: usize },
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

/// Communicates the state of an [`InputHook`]
///
/// Marker to tell [`process_input_event`] to keep the [`InputEventHook`] active, (`Continue`), or to drop
/// it and run the [`HookStates`] revert callback if one was set when creating the `InputHook`, (`Release`).
///
/// [`process_input_event`]: LineReader::process_input_event
pub enum HookControl {
    Continue,
    Release,
}

/// Details ouput information for custom event processing.
///
/// `HookedEvent` is the return type of [`InputEventHook`]. Contains both the instructions for the read eval
/// print loop and the new state of [`InputEventHook`]. A `InputEventHook`'s set deconstructor, will allways
/// execute prior to set [`EventLoop`] instructions if `HookControl::Release` is specified. All `HookedEvent`
/// constructors can not fail. They are always wrapped in `Ok` to reduce boilerplate
pub struct HookedEvent<Ctx, W: Write> {
    event: EventLoop<Ctx, W>,
    new_state: HookControl,
}

impl<Ctx, W: Write> HookedEvent<Ctx, W> {
    /// Constructor can not fail, output is wrapped in `Ok` to reduce boilerplate
    #[inline]
    pub fn new(event: EventLoop<Ctx, W>, new_state: HookControl) -> io::Result<Self> {
        Ok(Self { event, new_state })
    }

    /// Will tell the read eval print loop to continue and keep the current [`InputEventHook`] active.  
    /// Constructor can not fail, output is wrapped in `Ok` to reduce boilerplate
    #[inline]
    pub fn continue_hook() -> io::Result<Self> {
        Ok(Self {
            event: EventLoop::Continue,
            new_state: HookControl::Continue,
        })
    }

    /// Will tell the read eval print loop to continue and drop the current [`InputEventHook`].  
    /// Constructor can not fail, output is wrapped in `Ok` to reduce boilerplate
    #[inline]
    pub fn release_hook() -> io::Result<Self> {
        Ok(Self {
            event: EventLoop::Continue,
            new_state: HookControl::Release,
        })
    }

    /// Will tell the read eval print loop to break and drop the current [`InputEventHook`].  
    /// Constructor can not fail, output is wrapped in `Ok` to reduce boilerplate
    #[inline]
    pub fn break_repl() -> io::Result<Self> {
        Ok(Self {
            event: EventLoop::Break,
            new_state: HookControl::Release,
        })
    }
}

/// Communicates to the REPL how it should react to input events
///
/// `EventLoop` enum acts as a control router for how your read eval print loop should react to input events.
/// It provides mutable access back to your `Ctx` both synchronously and asynchronously. If your callback
/// can error the [`conditionally_remove_hook`] method can restore the intial state of the `LineReader` as
/// well as remove the queued input hook that was responsible for spawning the callback that resulted in an
/// error.  
///
/// `TryProcessInput` uses [`shellwords::split`] to parse user input into common shell tokens.
///
/// [`conditionally_remove_hook`]: LineReader::conditionally_remove_hook
/// [`shellwords::split`]: <https://docs.rs/shell-words/latest/shell_words/fn.split.html>
pub enum EventLoop<Ctx, W: Write> {
    Continue,
    Break,
    AsyncCallback(Box<AsyncCallback<Ctx, W>>),
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

    /// Run the reset state callback if present
    #[inline]
    fn try_run_reset_callback(
        &mut self,
        context: &mut Ctx,
        to: HookStates<Ctx, W>,
    ) -> io::Result<()> {
        let Some(reset) = to.revert else {
            return Ok(());
        };
        reset(self, context)
    }

    /// Makes sure the current `input_hook`'s initializer has been executed
    fn try_init_input_hook(&mut self, context: &mut Ctx) -> Option<io::Result<()>> {
        let callback = self.input_hooks.front_mut()?;
        let init = callback.init_revert.init.take()?;
        Some(init(self, context))
    }

    /// Queues an [`InputHook`] for execution
    #[inline]
    pub fn register_input_hook(&mut self, input_hook: InputHook<Ctx, W>) {
        self.input_hooks.push_back(input_hook);
    }

    /// Removes the currently active [`InputEventHook`] and calls its destructor if the hooks UID matches the
    /// UID of the provided error. Return values:
    /// - `Err(io::Error)` hook removed and destructor returned err
    /// - `Ok(true)` hook removed and destructor succeeded
    /// - `Ok(false)` no hook to remove or queued hook UID does not match the UID of the given `err`
    ///
    /// Eg:
    /// ```ignore
    /// EventLoop::AsyncCallback(callback) => {
    ///     if let Err(err) = callback(&mut line_handle, &mut command_context).await {
    ///         line_handle.println(err.to_string())?;
    ///         line_handle.conditionally_remove_hook(&err)?;
    ///     }
    /// },
    /// ```
    pub fn conditionally_remove_hook(
        &mut self,
        context: &mut Ctx,
        err: &CallbackErr,
    ) -> io::Result<bool> {
        if self
            .next_input_hook()
            .is_some_and(|hook| hook.uid == err.uid)
        {
            let hook = self
                .pop_input_hook()
                .expect("`next_input_hook` & `pop_input_hook` both look at first queued hook");
            return self
                .try_run_reset_callback(context, hook.init_revert)
                .map(|_| true);
        }
        Ok(false)
    }

    /// Pops the first queued `input_hook`
    #[inline]
    fn pop_input_hook(&mut self) -> Option<InputHook<Ctx, W>> {
        self.input_hooks.pop_front()
    }

    /// References the first queued `input_hook`
    #[inline]
    fn next_input_hook(&mut self) -> Option<&InputHook<Ctx, W>> {
        self.input_hooks.front()
    }

    /// Makes sure background messages are displayed properly. Internally this method expects a call to render
    /// to happen directly following this call. Meaning it is only useful to be called from it's own branch in a
    /// [`select!`] macro. Internally this is what [`spawn`] does for you. If writing your own run eval print loop
    /// see [basic_custom.rs] for an example.
    ///
    /// [basic_custom.rs]: <https://github.com/WardLordRuby/repl-oxide/blob/main/examples/basic_custom.rs>
    /// [`select!`]: <https://docs.rs/tokio/latest/tokio/macro.select.html>
    /// [`spawn`]: LineReader::spawn
    pub fn print_background_msg<T: Display>(&mut self, msg: T) -> io::Result<()> {
        execute!(self.term, BeginSynchronizedUpdate)?;
        self.term.queue(cursor::Hide)?;
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        self.println(msg.to_string())
    }

    /// Queues text to be displayed on the repl's writer to normalize accross targets. Replaces all new line
    /// characters with "\r\n". Supports printing multi-line strings.
    ///
    /// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
    /// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
    /// text is printed as you would expect on all targets.
    ///
    /// This method is designed to only be used when the repl is busy. Eg. from within a commands definition. If
    /// you need to display text while the repl is active see: [`print_background_msg`]
    ///
    /// If only compiling for Windows targets, the `println!` macro will display text as expected.
    ///
    /// [`print_background_msg`]: Self::print_background_msg
    /// [`build`]: crate::builder::LineReaderBuilder::build
    #[inline]
    pub fn println<S: AsRef<str>>(&mut self, str: S) -> io::Result<()> {
        println(&mut self.term, str)
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
        self.update_completeion();
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

    pub(crate) fn move_to_beginning(&mut self, from: u16) -> io::Result<()> {
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
    /// Eg:
    /// ```ignore
    /// line_handle.clear_unwanted_inputs(&mut reader).await?;
    /// line_handle.render()?;
    /// ```
    /// [`run`]: crate::line::LineReader::run
    /// [`spawn`]: crate::line::LineReader::spawn
    pub fn render(&mut self, context: &mut Ctx) -> io::Result<()> {
        if std::mem::take(&mut self.uneventful) {
            return Ok(());
        }

        if std::mem::take(&mut self.command_entered) {
            // We can not use the `position` command on UNIX
            // It will have to be clear to unix/cross-platform users that they will be requred
            // to always use the staging solution for printing any messages to the console
            #[cfg(windows)]
            {
                self.cursor_at_start = cursor::position()?.0 == 0;
            }
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
                    .map(|str| (str, GhostTextMeta::History { p: *p }))
            })
            .or_else(|| {
                let (recomendation, kind) = self
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

                recomendation
                    .strip_prefix(last_token)
                    .map(|str| (str, GhostTextMeta::Recomendation { len: str.len() }))
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
        self.update_completeion();
    }

    /// Pops a char from the input line and tries to update suggestions if completion is enabled
    pub fn remove_char(&mut self) -> io::Result<()> {
        self.line.input.pop();
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        self.line.len = self.line.len.saturating_sub(1);
        self.update_completeion();
        Ok(())
    }

    /// Writes the current line to the terminal and returns the user input of the line
    pub fn new_line(&mut self) -> io::Result<String> {
        self.term
            .queue(Clear(FromCursorDown))?
            .queue(Print(NEW_LINE))?;
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

    /// Changes the currently displayed user input to the given `line`
    pub fn change_line(&mut self, line: String) -> io::Result<()> {
        self.change_line_raw(line)?;
        self.reset_completion();
        self.update_completeion();
        Ok(())
    }

    /// For internal use when we **know** that we want to keep the same completion state
    pub(crate) fn change_line_raw(&mut self, line: String) -> io::Result<()> {
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        self.line.len = line.chars().count() as u16;
        self.line.input = line;
        Ok(())
    }

    fn enter_command(&mut self) -> io::Result<&str> {
        self.term.queue(cursor::Hide)?;
        let cmd = self.new_line()?;
        self.add_to_history(&cmd);
        self.command_entered = true;

        Ok(self
            .history
            .last()
            .expect("just pushed into `prev_entries`"))
    }

    fn append_ghost_text(&mut self) -> io::Result<()> {
        let Some(meta) = self.ghost_text.take() else {
            self.set_uneventful();
            return Ok(());
        };

        match meta {
            GhostTextMeta::History { p } => self.change_line(
                self.history
                    .get(&p)
                    .expect("set meta `p` is valid position")
                    .clone(),
            )?,
            GhostTextMeta::Recomendation { len } => {
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
    /// [custom quit command]: crate::LineReaderBuilder::with_custom_quit_command
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

    /// The main control flow for awaited events from a `crossterm::event::EventStream`. Works well as its
    /// own branch in a `tokio::select!`.
    ///
    /// Example read eval print loop assuming we have a `Ctx`, `command_context`,  that implements
    /// [`Executor`]
    ///
    /// See: [`process_async_callback!`], for reducing boilerplate for callbacks if you plan to use the tracing
    /// crate for error logging
    ///
    /// ```ignore
    /// let mut reader = crossterm::event::EventStream::new();
    /// let mut line_handle = repl_builder(std::io::stdout())
    ///     .build()
    ///     .expect("input writer accepts crossterm commands");
    ///
    /// loop {
    ///     line_handle.clear_unwanted_inputs(&mut reader).await?;
    ///     line_handle.render()?;
    ///
    ///     if let Some(event_result) = reader.next().await {
    ///         match line_handle.process_input_event(&mut command_context, event_result?)? {
    ///             EventLoop::Continue => (),
    ///             EventLoop::Break => break,
    ///             EventLoop::AsyncCallback(callback) => {
    ///                 if let Err(err) = callback(&mut line_handle, &mut command_context).await {
    ///                     line_handle.println(err.to_string())?;
    ///                     line_handle.conditionally_remove_hook(&err)?;
    ///                 }
    ///             },
    ///             EventLoop::TryProcessInput(Ok(user_tokens)) => {
    ///                 match command_context.try_execute_command(user_tokens).await? {
    ///                     CommandHandle::Processed => (),
    ///                     CommandHandle::InsertHook(input_hook) => line_handle.register_input_hook(input_hook),
    ///                     CommandHandle::Exit => break,
    ///                 }
    ///             }
    ///             EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
    ///                 line_handle.println(mismatched_quotes.to_string())?;
    ///             },
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// [`Executor`]: crate::executor::Executor
    /// [`process_async_callback!`]: crate::process_async_callback
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
                    HookControl::Release => {
                        self.try_run_reset_callback(context, hook.init_revert)?
                    }
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
                if !self.line.input.trim().is_empty() {
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
