use crate::{
    ansi_code::{RED, RESET},
    line::{Repl, NEW_LINE},
};

use std::{
    fmt::Display,
    io::{self, Write},
};

use crossterm::{
    cursor, execute,
    terminal::{BeginSynchronizedUpdate, Clear, ClearType::FromCursorDown},
    QueueableCommand,
};

/// Queues a single line to be displayed on the given writer to normalize accross targets.
///
/// Internally just appends ``"\r\n"`` to the end of the given input. If you are looking to convert multiple
/// line endings at once use: [`print_lines`] or [`Repl::print_lines`]
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`Repl::println`]. If you need to display text while the repl is active see:
/// [`Repl::print_background_msg`]
///
/// If only compiling for Windows targets, the `println!` macro will display text as expected.
///
/// [`build`]: crate::line::builder::ReplBuilder::build
pub fn println<W, D>(writer: &mut W, print: D) -> io::Result<()>
where
    W: Write,
    D: Display,
{
    write!(writer, "{print}{NEW_LINE}")?;
    Ok(())
}

/// Queues a single color encoded line to be displayed on the given writer to normalize accross targets.
///
/// Only will color encode [`RED`] if the [`Repl`]'s line stylization is enabled. Appends `"\r\n"` to the
/// end of the given input.
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`Repl::eprintln`]. If you need to display text while the repl is active see:
/// [`Repl::print_background_msg`] (Does not handle color encoding)
///
/// If only compiling for Windows targets, the `println!` macro will display text as expected.
///
/// [`build`]: crate::line::builder::ReplBuilder::build
pub fn eprintln<W, D>(writer: &mut W, print: D, stylize: bool) -> io::Result<()>
where
    W: Write,
    D: Display,
{
    if !stylize {
        return println(writer, print);
    }

    write!(writer, "{RED}{print}{RESET}{NEW_LINE}")?;
    Ok(())
}

/// Queues multi line text to be displayed on the given writer to normalize accross targets.
///
/// Replaces all new line characters with `"\r\n"`. Supports printing multi-line strings. If you do not need
/// to convert multiple line endings at once use: [`println`] or [`Repl::println`]
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`Repl::println`]. If you need to display text while the repl is active see:
/// [`Repl::print_background_msg`]
///
/// If only compiling for Windows targets, the `println!` macro will display text as expected.
///
/// [`println`]: crate::line::print::println
/// [`build`]: crate::line::builder::ReplBuilder::build
pub fn print_lines<S, W>(writer: &mut W, str: S) -> io::Result<()>
where
    S: AsRef<str>,
    W: Write,
{
    let ml_fmt = str.as_ref().replace("\n", NEW_LINE);

    write!(
        writer,
        "{ml_fmt}{}",
        if !ml_fmt.ends_with(NEW_LINE) {
            NEW_LINE
        } else {
            ""
        }
    )?;

    Ok(())
}

impl<Ctx, W: Write> Repl<Ctx, W> {
    /// Makes sure background messages are displayed properly. Internally this method expects a call to render
    /// to happen directly following this call. Meaning it is only useful to be called from it's own branch in a
    /// [`select!`] macro. Internally this is what [`spawn`] does for you. If you are looking to convert multiple
    /// line endings at once use: [`Repl::print_multiline_background_msg`].
    ///
    /// If writing your own run eval print loop see [basic_custom.rs] for an example.
    ///
    /// [basic_custom.rs]: <https://github.com/WardLordRuby/repl-oxide/blob/main/examples/basic_custom.rs>
    /// [`select!`]: <https://docs.rs/tokio/latest/tokio/macro.select.html>
    /// [`spawn`]: Repl::spawn
    pub fn print_background_msg<T: Display>(&mut self, msg: T) -> io::Result<()> {
        self.background_msg_prep()?;
        self.println(msg)
    }

    /// Makes sure background messages are displayed properly. Internally this method expects a call to render
    /// to happen directly following this call. Meaning it is only useful to be called from it's own branch in a
    /// [`select!`] macro. Internally this is what [`spawn`] does for you. If you do not need to convert multiple
    /// line endings at once use: [`Repl::print_background_msg`].
    ///
    /// If writing your own run eval print loop see [basic_custom.rs] for an example.
    ///
    /// [basic_custom.rs]: <https://github.com/WardLordRuby/repl-oxide/blob/main/examples/basic_custom.rs>
    /// [`select!`]: <https://docs.rs/tokio/latest/tokio/macro.select.html>
    /// [`spawn`]: Repl::spawn
    pub fn print_multiline_background_msg<S: AsRef<str>>(&mut self, print: S) -> io::Result<()> {
        self.background_msg_prep()?;
        self.print_lines(print)
    }

    /// Queues text to be displayed on the repl's writer to normalize accross targets. Appends `"\r\n"` to the end
    /// of the given input. If you are looking to convert multiple line endings at once use:
    /// [`Repl::print_lines`].
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
    /// [`build`]: crate::line::builder::ReplBuilder::build
    #[inline]
    pub fn println<D: Display>(&mut self, print: D) -> io::Result<()> {
        println(&mut self.term, print)
    }

    /// Queues color encoded text to be displayed on the repl's writer to normalize accross targets. Only will
    /// color encode [`RED`] if the [`Repl`]'s line stylization is enabled. Appends `"\r\n"` to the end of the
    /// given input.
    ///
    /// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
    /// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
    /// text is printed as you would expect on all targets.
    ///
    /// This method is designed to only be used when the repl is busy. Eg. from within a commands definition. If
    /// you need to display text while the repl is active see: [`print_background_msg`] (Does not handle color
    /// encoding)
    ///
    /// If only compiling for Windows targets, the `println!` macro will display text as expected.
    ///
    /// [`print_background_msg`]: Self::print_background_msg
    /// [`build`]: crate::line::builder::ReplBuilder::build
    #[inline]
    pub fn eprintln<D: Display>(&mut self, print: D) -> io::Result<()> {
        eprintln(&mut self.term, print, self.line.style_enabled)
    }

    /// Queues text to be displayed on the repl's writer to normalize accross targets. Replaces all new line
    /// characters with `"\r\n"`. Supports printing multi-line strings. If you do not need to convert multiple
    /// line endings at once use: [`Repl::println`].
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
    /// [`build`]: crate::line::builder::ReplBuilder::build
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
