mod builder;
mod executor;
mod line;
mod style;

pub mod completion;
pub use crate::builder::*;
pub use crate::executor::*;
pub use crate::line::*;

#[cfg(feature = "macros")]
pub mod macros;

#[cfg(feature = "runner")]
pub mod runner;

/// Re-export of [`strip_ansi`](https://docs.rs/strip_ansi/latest/strip_ansi/fn.strip_ansi.html)
/// the ported chalk regex
pub use strip_ansi::strip_ansi;

pub mod ansi_code {
    pub const RED: &str = "\x1b[31m";
    pub const YELLOW: &str = "\x1b[38;5;220m";
    pub const GREEN: &str = "\x1b[92m";
    pub const BLUE: &str = "\x1b[38;5;38m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const GREY: &str = "\x1b[38;5;238m";
    pub const WHITE: &str = "\x1b[0m";
}
