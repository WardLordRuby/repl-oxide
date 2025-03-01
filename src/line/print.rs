use crate::line::{LineReader, NEW_LINE};

use std::{
    fmt::Display,
    io::{self, Write},
};

use crossterm::{
    cursor, execute,
    style::Print,
    terminal::{BeginSynchronizedUpdate, Clear, ClearType::FromCursorDown},
    QueueableCommand,
};

/// Queues a single line to be displayed on the given writer to normalize accross targets.
///
/// Internally just appends ``"\r\n"`` to the end of the given input. If you are looking to convert multiple
/// line endings at once use: [`print_lines`] or [`LineReader::print_lines`]
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`LineReader::println`]. If you need to display text while the repl is active see:
/// [`LineReader::print_background_msg`]
///
/// If only compiling for Windows targets, the `println!` macro will display text as expected.
///
/// [`build`]: crate::line::builder::LineReaderBuilder::build
pub fn println<W, D>(writer: &mut W, print: D) -> io::Result<()>
where
    W: Write,
    D: Display,
{
    writer.queue(Print(print))?.queue(Print(NEW_LINE))?;
    Ok(())
}

/// Queues multi line text to be displayed on the given writer to normalize accross targets.
///
/// Replaces all new line characters with `"\r\n"`. Supports printing multi-line strings. If you do not need
/// to convert multiple line endings at once use: [`println`] or [`LineReader::println`]
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`LineReader::println`]. If you need to display text while the repl is active see:
/// [`LineReader::print_background_msg`]
///
/// If only compiling for Windows targets, the `println!` macro will display text as expected.
///
/// [`println`]: crate::line::print::println
/// [`build`]: crate::line::builder::LineReaderBuilder::build
pub fn print_lines<S, W>(writer: &mut W, str: S) -> io::Result<()>
where
    S: AsRef<str>,
    W: Write,
{
    let mut ml_fmt = str.as_ref().replace("\n", NEW_LINE);
    if !ml_fmt.ends_with(NEW_LINE) {
        ml_fmt.push_str(NEW_LINE);
    }

    writer.queue(Print(ml_fmt))?;
    Ok(())
}

impl<Ctx, W: Write> LineReader<Ctx, W> {
    /// Makes sure background messages are displayed properly. Internally this method expects a call to render
    /// to happen directly following this call. Meaning it is only useful to be called from it's own branch in a
    /// [`select!`] macro. Internally this is what [`spawn`] does for you. If you are looking to convert multiple
    /// line endings at once use: [`LineReader::print_multiline_background_msg`].
    ///
    /// If writing your own run eval print loop see [basic_custom.rs] for an example.
    ///
    /// [basic_custom.rs]: <https://github.com/WardLordRuby/repl-oxide/blob/main/examples/basic_custom.rs>
    /// [`select!`]: <https://docs.rs/tokio/latest/tokio/macro.select.html>
    /// [`spawn`]: LineReader::spawn
    pub fn print_background_msg<T: Display>(&mut self, msg: T) -> io::Result<()> {
        self.background_msg_prep()?;
        self.println(msg)
    }

    /// Makes sure background messages are displayed properly. Internally this method expects a call to render
    /// to happen directly following this call. Meaning it is only useful to be called from it's own branch in a
    /// [`select!`] macro. Internally this is what [`spawn`] does for you. If you do not need to convert multiple
    /// line endings at once use: [`LineReader::print_background_msg`].
    ///
    /// If writing your own run eval print loop see [basic_custom.rs] for an example.
    ///
    /// [basic_custom.rs]: <https://github.com/WardLordRuby/repl-oxide/blob/main/examples/basic_custom.rs>
    /// [`select!`]: <https://docs.rs/tokio/latest/tokio/macro.select.html>
    /// [`spawn`]: LineReader::spawn
    pub fn print_multiline_background_msg<S: AsRef<str>>(&mut self, print: S) -> io::Result<()> {
        self.background_msg_prep()?;
        self.print_lines(print)
    }

    /// Queues text to be displayed on the repl's writer to normalize accross targets. Appends `"\r\n"` to the end
    /// of the given input. If you are looking to convert multiple line endings at once use:
    /// [`LineReader::print_lines`].
    ///
    /// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
    /// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
    /// text is printed as you would expect on all targets.
    ///
    /// This method is designed to only be used when the repl is busy. Eg. from within a commands definition. If
    /// you need to display text while the repl is active see: [`print_background_msg`]
    ///
    /// If only compiling for Windows targets, the `println!` macro will display text as expected.
    ///
    /// [`print_background_msg`]: Self::print_background_msg
    /// [`build`]: crate::line::builder::LineReaderBuilder::build
    #[inline]
    pub fn println<D: Display>(&mut self, print: D) -> io::Result<()> {
        println(&mut self.term, print)
    }

    /// Queues text to be displayed on the repl's writer to normalize accross targets. Replaces all new line
    /// characters with `"\r\n"`. Supports printing multi-line strings. If you do not need to convert multiple
    /// line endings at once use: [`LineReader::println`].
    ///
    /// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
    /// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
    /// text is printed as you would expect on all targets.
    ///
    /// This method is designed to only be used when the repl is busy. Eg. from within a commands definition. If
    /// you need to display text while the repl is active see: [`print_background_msg`]
    ///
    /// If only compiling for Windows targets, the `println!` macro will display text as expected.
    ///
    /// [`print_background_msg`]: Self::print_background_msg
    /// [`build`]: crate::line::builder::LineReaderBuilder::build
    #[inline]
    pub fn print_lines<S: AsRef<str>>(&mut self, str: S) -> io::Result<()> {
        print_lines(&mut self.term, str)
    }

    #[inline(always)]
    fn background_msg_prep(&mut self) -> io::Result<()> {
        execute!(self.term, BeginSynchronizedUpdate)?;
        self.term.queue(cursor::Hide)?;
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        Ok(())
    }
}
