use crate::{
    callback::{HookLifecycle, InputEventHook},
    line::{EventLoop, LineReader},
};

use std::{
    borrow::Cow,
    fmt::Display,
    io::{self, Write},
    sync::atomic::{AtomicUsize, Ordering},
};

static HOOK_UID: AtomicUsize = AtomicUsize::new(0);

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
    pub(super) init_revert: HookStates<Ctx, W>,
    pub(super) event_hook: Box<InputEventHook<Ctx, W>>,
}

/// Holds the constructor and deconstructor of an [`InputHook`]
///
/// Can hold 2 unique [`HookLifecycle`] callbacks. This type's constructor is a method on
/// [`InputHook::new_hook_states`]
pub struct HookStates<Ctx, W: Write> {
    pub(super) init: Option<Box<HookLifecycle<Ctx, W>>>,
    pub(super) revert: Option<Box<HookLifecycle<Ctx, W>>>,
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
    /// [`AsyncCallback`]: crate::callback::AsyncCallback
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
    /// [`AsyncCallback`]: crate::callback::AsyncCallback
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
/// [`AsyncCallback`]: crate::callback::AsyncCallback
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

impl<Ctx, W: Write> LineReader<Ctx, W> {
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
    pub(crate) fn pop_input_hook(&mut self) -> Option<InputHook<Ctx, W>> {
        self.input_hooks.pop_front()
    }

    /// References the first queued `input_hook`
    #[inline]
    fn next_input_hook(&mut self) -> Option<&InputHook<Ctx, W>> {
        self.input_hooks.front()
    }

    /// Run the reset state callback if present
    #[inline]
    pub(super) fn try_run_reset_callback(
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
    pub(super) fn try_init_input_hook(&mut self, context: &mut Ctx) -> Option<io::Result<()>> {
        let callback = self.input_hooks.front_mut()?;
        let init = callback.init_revert.init.take()?;
        Some(init(self, context))
    }
}
