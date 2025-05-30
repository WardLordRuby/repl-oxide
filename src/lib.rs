#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod line;

/// Collection of types used to implement the `Executor` trait
pub mod executor;

/// Collection of helpers to easily interact with Clap
///
/// This module helps to reduce boilerplate when working with Clap derive. [Repl::print_clap_err] is also included
/// in this module. The `"clap"` feature flag is included by default with repl_oxide.
#[cfg(feature = "clap")]
pub mod clap;

#[cfg(feature = "macros")]
#[doc(hidden)]
pub mod macros;

#[cfg(feature = "runner")]
#[doc(hidden)]
pub mod runner;

#[cfg(feature = "spawner")]
#[doc(hidden)]
pub mod spawner;

pub use line::style::ansi_code;
pub use line::*;

/// Re-export of [`strip_ansi`] the ported chalk regex
///
/// [`strip_ansi`]: <https://docs.rs/strip_ansi/latest/strip_ansi/fn.strip_ansi.html>
pub use strip_ansi::strip_ansi;

/// Re-export of [`StreamExt`] from tokio_stream
///
/// [`StreamExt`]: <https://docs.rs/tokio-stream/latest/tokio_stream/trait.StreamExt.html>
pub use tokio_stream::StreamExt;
