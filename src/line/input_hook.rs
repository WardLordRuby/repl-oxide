use crate::line::{EventLoop, Repl};

use std::{
    borrow::Cow,
    fmt::Display,
    future::Future,
    io::{self, Write},
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};

use crossterm::event::Event;

static HOOK_UID: AtomicUsize = AtomicUsize::new(0);

/// Callback used by [`InputHook`] for determining how [`Event`]'s are processed
///
/// This callback can be constructed via either `InputHook` constructor ([`new`] /
/// [`with_new_uid`]).
///
/// [`InputHook`]: crate::line::InputHook
/// [`new`]: crate::line::InputHook::new
/// [`with_new_uid`]: crate::line::InputHook::with_new_uid
/// [`Event`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.Event.html>
pub trait InputEventHook<Ctx, W: Write>:
    Fn(&mut Repl<Ctx, W>, &mut Ctx, Event) -> io::Result<HookedEvent<Ctx, W>> + Send
{
}

impl<Ctx, W: Write, T> InputEventHook<Ctx, W> for T where
    T: Fn(&mut Repl<Ctx, W>, &mut Ctx, Event) -> io::Result<HookedEvent<Ctx, W>> + Send
{
}

/// Constructor and destructor for an [`InputHook`]
///
/// A pair of this callback type can be constructed via [`HookStates::new`], then passed to
/// either `InputHook` constructor ([`new`] / [`with_new_uid`]) for assignment.
///
/// [`HookStates::new`]: crate::line::input_hook::HookStates::new
/// [`InputHook`]: crate::line::input_hook::InputHook
/// [`new`]: crate::line::input_hook::InputHook::new
/// [`with_new_uid`]: crate::line::input_hook::InputHook::with_new_uid
pub trait HookLifecycle<Ctx, W: Write>:
    FnOnce(&mut Repl<Ctx, W>, &mut Ctx) -> io::Result<()> + Send
{
}

impl<Ctx, W: Write, T> HookLifecycle<Ctx, W> for T where
    T: FnOnce(&mut Repl<Ctx, W>, &mut Ctx) -> io::Result<()> + Send
{
}

/// Callback to be used when you need to await operations on your generic `Ctx`
///
/// Can be returned as the [`HookedEvent`] from within an [`InputHook`] and then awaited on
/// by the run eval process loop. This callback can be constructed via
/// [`EventLoop::new_async_callback`].
///
/// [`EventLoop::new_async_callback`]: crate::line::EventLoop::new_async_callback
/// [`HookedEvent`]: crate::line::input_hook::HookedEvent
/// [`InputHook`]: crate::line::InputHook
pub trait AsyncCallback<Ctx, W: Write>:
    for<'a> FnOnce(
        &'a mut Repl<Ctx, W>,
        &'a mut Ctx,
    ) -> Pin<Box<dyn Future<Output = Result<(), CallbackErr>> + Send + 'a>>
    + Send
{
}

impl<Ctx, W: Write, T> AsyncCallback<Ctx, W> for T where
    T: for<'a> FnOnce(
            &'a mut Repl<Ctx, W>,
            &'a mut Ctx,
        )
            -> Pin<Box<dyn Future<Output = Result<(), CallbackErr>> + Send + 'a>>
        + Send
{
}

/// Powerful type that allows customization of library default implementations
///
/// `InputHook` gives you access to customize how [`Event`]'s are processed and how the [`Repl`]
/// behaves.
///
/// Hooks can be initialized with a [`HookLifecycle`] that allows for a place to modify the current state
/// of the [`Repl`] and/or the users generic `Ctx`. To do so use [`HookStates::new`], note you must also
/// supply a separate callback to revert the changes back to your desired state when the `InputHook` is dropped.
///
/// Otherwise use [`HookStates::no_change`] to not specify new and previous states.
///
/// Hooks require a [`InputEventHook`] this callback can be is entirely responsible for controlling _all_
/// reactions to [`KeyEvent`]'s of kind: [`KeyEventKind::Press`]. This will act as a manual override of the
/// libraries event processor. You will have access to manually determine what methods are called on the
/// [`Repl`]. See: [callbacks.rs]
///
/// [callbacks.rs]: <https://github.com/WardLordRuby/repl-oxide/blob/main/examples/callbacks.rs>
/// [`Event`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.Event.html>
/// [`KeyEvent`]: <https://docs.rs/crossterm/latest/crossterm/event/struct.KeyEvent.html>
/// [`KeyEventKind::Press`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.KeyEventKind.html>
/// [`conditionally_remove_hook`]: Repl::conditionally_remove_hook
pub struct InputHook<Ctx, W: Write> {
    uid: HookUID,
    pub(super) init_revert: HookStates<Ctx, W>,
    pub(super) event_hook: Box<dyn InputEventHook<Ctx, W>>,
}

/// Holds the constructor and destructor of an [`InputHook`]
///
/// Can hold 2 unique [`HookLifecycle`] callbacks.
pub struct HookStates<Ctx, W: Write> {
    pub(super) init: Option<Box<dyn HookLifecycle<Ctx, W>>>,
    pub(super) revert: Option<Box<dyn HookLifecycle<Ctx, W>>>,
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

impl<Ctx, W: Write> HookStates<Ctx, W> {
    /// For use when creating an `InputHook` that doesn't need to change any state on construction or
    /// deconstruction. Equivalent to [`HookStates::default`]
    #[inline]
    pub fn no_change() -> Self {
        HookStates::default()
    }

    /// For use when creating an `InputHook` that changes the state of the [`Repl`] or the user supplied
    /// generic `Ctx` on construction and deconstruction via unique [`HookLifecycle`] callbacks.
    pub fn new<F1, F2>(init: F1, revert: F2) -> Self
    where
        F1: HookLifecycle<Ctx, W> + 'static,
        F2: HookLifecycle<Ctx, W> + 'static,
    {
        HookStates {
            init: Some(Box::new(init)),
            revert: Some(Box::new(revert)),
        }
    }
}

impl<Ctx, W: Write> EventLoop<Ctx, W> {
    /// Create a new [`AsyncCallback`] for the run eval process loop to execute. Ensure the surrounding [`InputHook`]
    /// has the same `uid` as the assigned [`CallbackErr`].
    pub fn new_async_callback<F>(f: F) -> Self
    where
        F: AsyncCallback<Ctx, W> + 'static,
    {
        EventLoop::AsyncCallback(Box::new(f))
    }
}

impl<Ctx, W: Write> InputHook<Ctx, W> {
    /// For use when creating an `InputHook` that contains an [`AsyncCallback`] that can error, else use
    /// [`with_new_uid`]. Ensure that the `InputHook` and [`CallbackErr`] share the same [`HookUID`]
    /// obtained through [`HookUID::new`].
    ///
    /// The library supplied repl runners ([`run`] / [`spawn`]) or event processor macro [`general_event_process`]
    /// will call [`conditionally_remove_hook`] when any callback errors. When writing your own repl it is
    /// recommended to implement this logic.  
    ///
    /// [`AsyncCallback`]: crate::line::input_hook::AsyncCallback
    /// [`with_new_uid`]: Self::with_new_uid
    /// [`conditionally_remove_hook`]: Repl::conditionally_remove_hook
    /// [`general_event_process`]: crate::general_event_process
    /// [`run`]: crate::line::Repl::run
    /// [`spawn`]: crate::line::Repl::spawn
    pub fn new<F>(uid: HookUID, init_revert: HookStates<Ctx, W>, event_hook: F) -> Self
    where
        F: InputEventHook<Ctx, W> + 'static,
    {
        assert!(uid.0 < HOOK_UID.load(Ordering::SeqCst));
        Self {
            uid,
            init_revert,
            event_hook: Box::new(event_hook),
        }
    }

    /// For use when creating an `InputHook` that does not contain an [`AsyncCallback`] that can error, else use
    /// [`new`].
    ///
    /// [`AsyncCallback`]: crate::line::input_hook::AsyncCallback
    /// [`new`]: Self::new
    pub fn with_new_uid<F>(init_revert: HookStates<Ctx, W>, event_hook: F) -> Self
    where
        F: InputEventHook<Ctx, W> + 'static,
    {
        Self {
            uid: HookUID::new(),
            init_revert,
            event_hook: Box::new(event_hook),
        }
    }
}

/// Unique linking identifier used for Error handling
///
/// `HookUID` links an [`InputEventHook`] to all it's spawned [`AsyncCallback`]. This provides a system for
/// dynamic [`InputHook`] termination. For more information see: [`conditionally_remove_hook`]
///
/// [`AsyncCallback`]: crate::line::input_hook::AsyncCallback
/// [`conditionally_remove_hook`]: Repl::conditionally_remove_hook
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
        Self {
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

/// Communicates the state of an [`InputHook`]
///
/// Marker to tell [`process_input_event`] to keep the [`InputEventHook`] active, (`Continue`), or to drop
/// it and run the [`HookStates`] revert callback if one was set when creating the `InputHook`, (`Release`).
///
/// [`process_input_event`]: Repl::process_input_event
pub enum HookControl {
    Continue,
    Release,
}

/// Details output information for custom event processing.
///
/// `HookedEvent` is the return type of [`InputEventHook`]. Contains both the instructions for the read eval
/// print loop and the new state of [`InputEventHook`]. A `InputEventHook`'s set destructor, will always
/// execute prior to set [`EventLoop`] instructions if `HookControl::Release` is specified. All `HookedEvent`
/// constructors can not fail. They are always wrapped in `Ok` to reduce boilerplate
pub struct HookedEvent<Ctx, W: Write> {
    pub(super) event: EventLoop<Ctx, W>,
    pub(super) new_state: HookControl,
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

impl<Ctx, W: Write> Repl<Ctx, W> {
    /// Queues an [`InputHook`] for execution
    #[inline]
    pub fn register_input_hook(&mut self, input_hook: InputHook<Ctx, W>) {
        self.input_hooks.push_back(input_hook);
    }

    /// Removes the currently active [`InputEventHook`] and calls its destructor if the hooks UID matches the
    /// UID of the provided error. Return values:
    /// - `Err(io::Error)` hook removed and destructor returned err
    /// - `Ok(true)` hook removed and destructor succeeded or input hook had no destructor set
    /// - `Ok(false)` no hook to remove or queued hook UID does not match the UID of the given `err`
    ///
    /// # Example
    ///
    /// ```ignore
    /// EventLoop::AsyncCallback(callback) => {
    ///     if let Err(err) = callback(&mut repl, &mut command_context).await {
    ///         repl.eprintln(err)?;
    ///         repl.conditionally_remove_hook(&err)?;
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
                .try_revert_input_hook(context, hook)
                .unwrap_or(Ok(()))
                .map(|_| true);
        }
        Ok(false)
    }

    /// Pops the first queued `input_hook`
    #[inline]
    pub(super) fn pop_input_hook(&mut self) -> Option<InputHook<Ctx, W>> {
        self.input_hooks.pop_front()
    }

    /// References the first queued `input_hook`
    #[inline]
    fn next_input_hook(&mut self) -> Option<&InputHook<Ctx, W>> {
        self.input_hooks.front()
    }

    /// Run the revert state callback on the given `InputHook` if present
    #[inline]
    pub(super) fn try_revert_input_hook(
        &mut self,
        context: &mut Ctx,
        hook: InputHook<Ctx, W>,
    ) -> Option<io::Result<()>> {
        let revert = hook.init_revert.revert?;
        Some(revert(self, context))
    }

    /// Makes sure the current `input_hook`'s initializer has been executed
    pub(super) fn try_init_input_hook(&mut self, context: &mut Ctx) -> Option<io::Result<()>> {
        let callback = self.input_hooks.front_mut()?;
        let init = callback.init_revert.init.take()?;
        Some(init(self, context))
    }
}
