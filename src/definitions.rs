/// Collection of ansi color codes
pub mod ansi_code {
    pub const RED: &str = "\x1b[31m";
    pub const YELLOW: &str = "\x1b[38;5;220m";
    pub const GREEN: &str = "\x1b[92m";
    pub const BLUE: &str = "\x1b[38;5;38m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const GREY: &str = "\x1b[2;37m";
    pub const DIM_WHITE: &str = "\x1b[90m";
    pub const RESET: &str = "\x1b[0m";
}

/// Collection of callbacks that allow for deeper library control
pub mod callback {
    use crate::line::{CallbackErr, HookedEvent, Repl};

    use std::{
        future::Future,
        io::{self, Write},
        pin::Pin,
    };

    use crossterm::event::Event;

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

    /// Constructor and deconstructor for an [`InputHook`]
    ///
    /// A pair of this callback type can be constructed via [`HookStates::new`], then passed to
    /// either `InputHook` constructor ([`new`] / [`with_new_uid`]) for assignment.
    ///
    /// [`HookStates::new`]: crate::line::HookStates::new
    /// [`InputHook`]: crate::line::InputHook
    /// [`new`]: crate::line::InputHook::new
    /// [`with_new_uid`]: crate::line::InputHook::with_new_uid
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
    /// [`HookedEvent`]: crate::line::HookedEvent
    /// [`InputHook`]: crate::line::InputHook
    pub trait AsyncCallback<Ctx, W: Write>:
        for<'a> FnOnce(
            &'a mut Repl<Ctx, W>,
            &'a mut Ctx,
        )
            -> Pin<Box<dyn Future<Output = Result<(), CallbackErr>> + Send + 'a>>
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
}
