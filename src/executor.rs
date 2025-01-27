use crate::line::InputHook;
use std::{
    future::Future,
    io::{self, Write},
};

/// Format tokens into what Clap's [`Parser`](https://docs.rs/clap/latest/clap/trait.Parser.html) trait expects
#[inline]
pub fn format_for_clap(
    tokens: Vec<String>,
) -> std::iter::Chain<std::iter::Once<std::string::String>, std::vec::IntoIter<std::string::String>>
{
    std::iter::once(String::new()).chain(tokens)
}

/// The suggested return type for commands
pub enum CommandHandle<Ctx, W: Write> {
    Processed,
    InsertHook(InputHook<Ctx, W>),
    Exit,
}

/// The `Executor` trait provides a optional way to structure how commands are handled through your
/// generic `Ctx` struct.
///
/// Example using `Stdout` writer and `try_parse_from` via `clap_derive::Parser`
/// ```ignore
/// impl Executor<Stdout> for CommandContext {
///     async fn try_execute_command(&mut self, user_tokens: Vec<String>) -> io::Result<CommandHandle<Self, Stdout>> {
///         match Command::try_parse_from(format_for_clap(user_tokens)) {
///             Ok(command) => match command {
///                 /*
///                     Route to command functions that return `io::Result<CommandHandle>`
///                 */
///                 Command::Version => self.print_version(),
///                 Command::Quit => self.quit().await,
///             },
///             Err(err) => {
///                 err.print()?;
///                 Ok(CommandHandle::Processed)
///             }
///         }
///     }
/// }
/// ```
///
/// Then within your main loop requires some boilerplate to match against the returned `CommandHandle`
/// ```ignore
/// Ok(EventLoop::TryProcessInput(Ok(user_tokens))) => {
///     match command_context.try_execute_command(user_tokens).await {
///         CommandHandle::Processed => (),
///         CommandHandle::InsertHook(input_hook) => line_reader.register_input_hook(input_hook),
///         CommandHandle::Exit => break,
///     }
/// }
/// ```
pub trait Executor<W: Write>: std::marker::Sized {
    fn try_execute_command(
        &mut self,
        user_tokens: Vec<String>,
    ) -> impl Future<Output = io::Result<CommandHandle<Self, W>>>;
}
