mod builder;
mod line;
mod style;

/// Collection of types used for auto completion of user input
pub mod completion;

/// Collection of types used to implment the `Executor` trait
pub mod executor;

pub use crate::builder::*;
pub use crate::line::*;

#[cfg(feature = "macros")]
#[doc(hidden)]
pub mod macros;

#[cfg(feature = "runner")]
#[doc(hidden)]
pub mod runner;

#[cfg(feature = "spawner")]
#[doc(hidden)]
pub mod spawner;

/// Re-export of [`strip_ansi`] the ported chalk regex
///
/// [`strip_ansi`]: <https://docs.rs/strip_ansi/latest/strip_ansi/fn.strip_ansi.html>
pub use strip_ansi::strip_ansi;

/// Re-export of [`StreamExt`] from tokio_stream
///
/// [`StreamExt`]: <https://docs.rs/tokio-stream/latest/tokio_stream/trait.StreamExt.html>
pub use tokio_stream::StreamExt;

/// Collection of ansi color codes
pub mod ansi_code {
    pub const RED: &str = "\x1b[31m";
    pub const YELLOW: &str = "\x1b[38;5;220m";
    pub const GREEN: &str = "\x1b[92m";
    pub const BLUE: &str = "\x1b[38;5;38m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const GREY: &str = "\x1b[38;5;238m";
    pub const WHITE: &str = "\x1b[0m";
}
