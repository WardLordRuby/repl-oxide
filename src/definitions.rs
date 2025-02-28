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
    use crate::line::{CallbackErr, HookedEvent, LineReader};

    use crossterm::event::Event;
    use std::{future::Future, io, pin::Pin};

    /// Callback used by [`InputHook`] for determining how [`Event`]'s are processed
    ///
    /// [`InputHook`]: crate::line::InputHook
    /// [`Event`]: <https://docs.rs/crossterm/latest/crossterm/event/enum.Event.html>
    pub type InputEventHook<Ctx, W> =
        dyn Fn(&mut LineReader<Ctx, W>, &mut Ctx, Event) -> io::Result<HookedEvent<Ctx, W>> + Send;

    /// Constructor and deconstructor for an [`InputHook`]
    ///
    /// [`InputHook`]: crate::line::InputHook
    pub type HookLifecycle<Ctx, W> =
        dyn FnOnce(&mut LineReader<Ctx, W>, &mut Ctx) -> io::Result<()> + Send;

    /// Callback to be used when you need to await operations on your generic `Ctx`
    ///
    /// Can be created from within an [`InputHook`] or communicated to the run eval process loop to be
    /// immediately executed as an `FnOnce`
    ///
    /// [`InputHook`]: crate::line::InputHook
    pub type AsyncCallback<Ctx, W> = dyn for<'a> FnOnce(
            &mut LineReader<Ctx, W>,
            &'a mut Ctx,
        )
            -> Pin<Box<dyn Future<Output = Result<(), CallbackErr>> + Send + 'a>>
        + Send;
}
