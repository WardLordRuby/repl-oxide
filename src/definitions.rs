/// Collection of ansi color codes
pub mod ansi_code {
    pub const RED: &str = "\x1b[31m";
    pub const YELLOW: &str = "\x1b[38;5;220m";
    pub const GREEN: &str = "\x1b[92m";
    pub const BLUE: &str = "\x1b[38;5;38m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const GREY: &str = "\x1b[38;5;238m";
    pub const RESET: &str = "\x1b[0m";
}

/// Collection of callbacks that allow for deeper library control
pub mod callback {
    use crate::line::{HookedEvent, InputHookErr, LineReader};

    use crossterm::event::Event;
    use std::{future::Future, io, pin::Pin};

    // MARK: TODO
    // add documentation for these types

    pub type InputEventHook<Context> =
        dyn Fn(&mut LineReader<Context>, Event) -> io::Result<HookedEvent<Context>> + Send;
    pub type ModLineState<Context> = dyn FnOnce(&mut LineReader<Context>) -> io::Result<()> + Send;
    pub type Callback<Context> = dyn Fn(&mut Context) -> Result<(), InputHookErr> + Send;
    pub type AsyncCallback<Context> = dyn for<'a> FnOnce(
            &'a mut Context,
        )
            -> Pin<Box<dyn Future<Output = Result<(), InputHookErr>> + Send + 'a>>
        + Send;
}
