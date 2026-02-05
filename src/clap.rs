use crate::{executor::CommandHandle, line::Repl};

use std::{
    ffi::OsString,
    io::{self, Write},
    iter,
};

use clap::{Error, Parser};

/// Helper function to easily adapt to types that impl [`clap_derive::Parser`]
///
/// Internally calls calls `T::try_parse_from` with `tokens` formatted for how clap's Parser expects
///
/// [`clap_derive::Parser`]: <https://docs.rs/clap/latest/clap/trait.Parser.html>
#[inline]
pub fn try_parse_from<T, I, S>(tokens: I) -> Result<T, Error>
where
    T: Parser,
    I: IntoIterator<Item = S>,
    S: Into<OsString> + Clone,
{
    T::try_parse_from(iter::once(OsString::new()).chain(tokens.into_iter().map(Into::into)))
}

impl<Ctx, W: Write> Repl<Ctx, W> {
    /// Helper method that calls [`Self::print_lines`] with the given clap `err` to ensure clap
    /// errors are printed correctly on all targets. Maps a successful print to [`CommandHandle::Processed`]
    #[inline]
    pub fn print_clap_err(&mut self, err: clap::Error) -> io::Result<CommandHandle<Ctx, W>> {
        self.print_lines(err.render().ansi().to_string())
            .map(|_| CommandHandle::Processed)
    }
}
