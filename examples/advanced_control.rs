// Example usage demonstrating advanced customization features and callback types
/*                      cargo r --example advanced-control                     */

use std::io::{self, Stdout};

use clap::Parser;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use repl_oxide::{
    ansi_code::{RED, WHITE},
    executor::*,
    repl_builder, EventLoop, HookControl, HookedEvent, InputEventHook, InputHook, ModLineState,
    StreamExt,
};

#[derive(Parser, Debug)]
#[command(
    name = "Example App",
    about = "Example app demonstrating repl-oxide's advanced customization features and callback types"
)]
enum Command {
    /// Exit the command line REPL
    #[command(alias = "exit")]
    Quit,
}

// Our context can store all persistent state. Commands can also be implemented on our
// context See 'examples/runner.rs'
struct CommandContext;

fn quit() -> io::Result<CommandHandle<CommandContext, Stdout>> {
    // Change the line state as soon as we return our new `InputHook`
    let init: Box<ModLineState<CommandContext, Stdout>> = Box::new(|handle| {
        handle.set_prompt_and_separator("Are you sure? (y/n)".to_string(), ": ");
        Ok(())
    });

    // Revert the line state if the user chooses not to quit
    let revert: Box<ModLineState<CommandContext, Stdout>> = Box::new(|handle| {
        handle.set_default_prompt_and_separator();
        Ok(())
    });

    // Define how our `InputEventHook` reacts to `KeyEvent`s of `KeyEventKind::Press`
    // This could also easily be set up to only react apon enter, for simplicity we will just react apon press
    let input_hook: Box<InputEventHook<CommandContext, Stdout>> =
        Box::new(|_repl_handle, event| match event {
            Event::Key(
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Char('d'),
                    modifiers: KeyModifiers::CONTROL,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Char('y'),
                    ..
                },
            ) => HookedEvent::new(EventLoop::Break, HookControl::Release),
            _ => HookedEvent::release_hook(),
        });

    // Since our `input_hook` does not return any `EventLoop::Callback` `EventLoop::AsyncCallback`
    // we can use `with_new_uid` here. If we wanted to modify the state of our `CommandContext` within
    // our `input_hook` we could use either callback type to do so. If said callback could error we would
    // have to ensure that the error has the same `UID` and the outer `InputHook`
    Ok(CommandHandle::InsertHook(InputHook::with_new_uid(
        InputHook::new_hook_states(init, revert),
        None,
        input_hook,
    )))
}

// Implement `Executor` so we can use `try_execute_command`
impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        user_tokens: Vec<String>,
    ) -> io::Result<CommandHandle<CommandContext, Stdout>> {
        match Command::try_parse_from(format_for_clap(user_tokens)) {
            Ok(command) => match command {
                Command::Quit => quit(),
            },
            Err(err) => err.print().map(|_| CommandHandle::Processed),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    // Gain access to all terminal events
    let mut event_stream = crossterm::event::EventStream::new();

    // Create our type that implements `Executor`
    let mut command_ctx = CommandContext;

    // Build a `LineReader` with a custom quit command (quit), to handle the repl state
    let mut repl = repl_builder(io::stdout())
        .with_custom_quit_command("quit")
        .build()
        .expect("input writer accepts crossterm commands");

    loop {
        // Disregard key inputs while user commands are being processed
        repl.clear_unwanted_inputs(&mut event_stream).await?;

        // Render the lines current state
        repl.render()?;

        // Await an Event from the stream
        if let Some(event_result) = event_stream.next().await {
            match repl.process_input_event(event_result?)? {
                EventLoop::Continue => (),
                EventLoop::Break => break,
                EventLoop::Callback(_) | EventLoop::AsyncCallback(_) => {
                    unreachable!(
                        "our only input hook within `quit` never outputs any `Callback` or `AsyncCallback`"
                    )
                }
                EventLoop::TryProcessInput(Ok(user_tokens)) => {
                    match command_ctx.try_execute_command(user_tokens).await? {
                        CommandHandle::Processed => (),
                        CommandHandle::InsertHook(input_hook) => {
                            repl.register_input_hook(input_hook)
                        }
                        CommandHandle::Exit => break,
                    }
                }
                EventLoop::TryProcessInput(Err(mismatched_quotes)) => {
                    eprintln!("{RED}{mismatched_quotes}{WHITE}")
                }
            }
        }
    }

    Ok(())
}
