use crate::line::InputHook;
use std::{future::Future, io::Write};

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
///     async fn try_execute_command(&mut self, user_tokens: Vec<String>) -> CommandHandle<Self, Stdout> {
///         match UserCommand::try_parse_from(
///             std::iter::once(String::new()).chain(user_tokens.into_iter()),
///         ) {
///             Ok(cli) => match cli.command {
///                 /*
///                     Route to command functions that return `CommandHandle`
///                 */
///                 Command::Version => self.print_version(),
///                 Command::Quit => self.quit().await,
///             },
///             Err(err) => {
///                 if let Err(prt_err) = err.print() {
///                     eprintln!("{err} {prt_err}");
///                 }
///                 CommandHandle::Processed
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
    ) -> impl Future<Output = CommandHandle<Self, W>>;
}
