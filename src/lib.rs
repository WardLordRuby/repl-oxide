mod definitions;
mod line;

/// Collection of types used to implment the `Executor` trait
pub mod executor;

pub use crate::definitions::*;
pub use line::builder::*;
pub use line::*;

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
