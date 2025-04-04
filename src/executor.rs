use crate::line::{input_hook::InputHook, Repl};

use std::{
    future::Future,
    io::{self, Write},
};

/// The suggested return type for commands
///
/// This is enforced inside the [`Executor`] trait. Provides straight forward returns and a option to insert a custom
/// [`InputHook`] to take control over [`KeyEvent`] processing
///
/// [`KeyEvent`]: <https://docs.rs/crossterm/latest/crossterm/event/struct.KeyEvent.html>
pub enum CommandHandle<Ctx, W: Write> {
    Processed,
    InsertHook(InputHook<Ctx, W>),
    Exit,
}

/// Required trait to implement for either pre-made REPL runner. [`run`] / [`spawn`]
///
/// The `Executor` trait provides a optional way to structure how commands are handled through your generic
/// `Ctx` struct.
///
/// # Example
///
/// [`Stdout`] writer and [`try_parse_from`] via [`clap_derive::Parser`]
/// ```ignore
/// impl Executor<Stdout> for CommandContext {
///     async fn try_execute_command(
///         &mut self,
///         repl_handle: Repl<Self, Stdout>,
///         user_tokens: Vec<String>
///     ) -> io::Result<CommandHandle<Self, Stdout>> {
///         match repl_oxide::clap::try_parse_from(&user_tokens) {
///             Ok(command) => match command {
///                 /*
///                     Route to command functions that return `io::Result<CommandHandle>`
///                 */
///                 Command::Version => self.print_version(),
///                 Command::Quit => self.quit().await,
///             },
///             Err(err) => repl_handle.print_clap_err(err),
///         }
///     }
/// }
/// ```
///
/// # Within manual repl implementation  
///
/// Then within your read eval print loop requires some boilerplate to match against the returned [`CommandHandle`]
/// ```ignore
/// EventLoop::TryProcessInput(Ok(user_tokens)) => {
///     match command_context.try_execute_command(&mut repl, user_tokens).await {
///         CommandHandle::Processed => (),
///         CommandHandle::InsertHook(input_hook) => repl.register_input_hook(input_hook),
///         CommandHandle::Exit => break,
///     }
/// }
/// ```
/// [`Stdout`]: std::io::Stdout
/// [`run`]: crate::line::Repl::run
/// [`spawn`]: crate::line::Repl::spawn
/// [`try_parse_from`]: <https://docs.rs/clap/latest/clap/trait.Parser.html#method.try_parse_from>
/// [`clap_derive::Parser`]: <https://docs.rs/clap/latest/clap/trait.Parser.html>
pub trait Executor<W: Write>: Sized {
    fn try_execute_command(
        &mut self,
        repl_handle: &mut Repl<Self, W>,
        user_tokens: Vec<String>,
    ) -> impl Future<Output = io::Result<CommandHandle<Self, W>>> + Send;
}
