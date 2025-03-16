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

/// Queues a single line to be displayed on the given writer to normalize across targets.
///
/// Internally just appends ``"\r\n"`` to the end of the given input. If you are looking to convert multiple
/// line endings at once use: [`print_lines`] or [`Repl::print_lines`]
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`Repl::println`].
///
/// If only compiling for Windows targets, the `println!` macro will display text as expected as long as it
/// is _only_ used when the repl is busy.
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

/// Queues a single color encoded line to be displayed on the given writer to normalize across targets.
///
/// Only will color encode [`RED`] if the [`Repl`]'s line stylization is enabled. Appends `"\r\n"` to the
/// end of the given input.
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`Repl::eprintln`].
///
/// If only compiling for Windows targets, the `eprintln!` macro will display text as expected as long as it
/// is _only_ used when the repl is busy.
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

/// Queues multi line text to be displayed on the given writer to normalize across targets.
///
/// Replaces all new line characters with `"\r\n"`. Supports printing multi-line strings. If you do not need
/// to convert multiple line endings at once use: [`println`] or [`Repl::println`]
///
/// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
/// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
/// text is printed as you would expect on all targets.
///
/// This function is designed to only be used when the repl is busy and you do not have access to the repl
/// handle prefer: [`Repl::print_lines`].
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
    /// Queues text to be displayed on the repl's writer to normalize across targets. Appends `"\r\n"` to the end
    /// of the given input. If you are looking to convert multiple line endings at once use:
    /// [`Repl::print_lines`].
    ///
    /// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
    /// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
    /// text is printed as you would expect on all targets.
    ///
    /// If only compiling for Windows targets, the `println!` macro will display text as expected as long as it
    /// is _only_ used when the repl is busy.
    ///
    /// [`build`]: crate::line::builder::ReplBuilder::build
    pub fn println<D: Display>(&mut self, print: D) -> io::Result<()> {
        if !self.cursor_at_start {
            self.prep_for_background_msg()?;
        }
        println(&mut self.term, print)
    }

    /// Queues color encoded text to be displayed on the repl's writer to normalize across targets. Only will
    /// color encode [`RED`] if the [`Repl`]'s line stylization is enabled. Appends `"\r\n"` to the end of the
    /// given input.
    ///
    /// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
    /// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
    /// text is printed as you would expect on all targets.
    ///
    /// If only compiling for Windows targets, the `eprintln!` macro will display text as expected as long as it
    /// is _only_ used when the repl is busy.
    ///
    /// [`build`]: crate::line::builder::ReplBuilder::build
    pub fn eprintln<D: Display>(&mut self, print: D) -> io::Result<()> {
        if !self.cursor_at_start {
            self.prep_for_background_msg()?;
        }
        eprintln(&mut self.term, print, self.line.style_enabled)
    }

    /// Queues text to be displayed on the repl's writer to normalize across targets. Replaces all new line
    /// characters with `"\r\n"`. Supports printing multi-line strings. If you do not need to convert multiple
    /// line endings at once use: [`Repl::println`].
    ///
    /// Since repl-oxide requires full control over the terminal driver and enforces "Raw Mode" via [`build`],
    /// [`std::println!`] on UNIX systems does not display text as it normally would. This function will ensure
    /// text is printed as you would expect on all targets.
    ///
    /// If only compiling for Windows targets, the `println!` macro will display text as expected as long as it
    /// is _only_ used when the repl is busy.
    ///
    /// [`build`]: crate::line::builder::ReplBuilder::build
    pub fn print_lines<S: AsRef<str>>(&mut self, str: S) -> io::Result<()> {
        if !self.cursor_at_start {
            self.prep_for_background_msg()?;
        }
        print_lines(&mut self.term, str)
    }

    /// In almost all cases this is not the method you are looking for, [`Repl::println`], [`Repl::eprintln`], and
    /// [`Repl::print_lines`] all take care of this for you. In the rare case you want to write into the repl manually
    /// you can use this method, however you will run into undefined behavior if what is written into the `Repl` does
    /// not end in a new line by the time [`Repl::render`] is called again.
    #[inline(always)]
    pub fn prep_for_background_msg(&mut self) -> io::Result<()> {
        execute!(self.term, BeginSynchronizedUpdate)?;
        self.term.queue(cursor::Hide)?;
        self.move_to_beginning(self.line_len())?;
        self.term.queue(Clear(FromCursorDown))?;
        Ok(())
    }
}
