// Example usage demonstrating the use of the available callback types
/*         cargo r --example callbacks --features="runner"          */

use repl_oxide::{
    executor::{format_for_clap, CommandHandle, Executor},
    input_hook::{HookStates, HookedEvent, InputHook},
    repl_builder, Repl,
};

use std::io::{self, Stdout};

use clap::Parser;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

#[derive(Parser)]
#[command(
    name = "", // Leaving name empty will give us more accurate clap help and error messages
    about = "Example app demonstrating repl-oxide's callback types"
)]
enum Command {
    /// Exit the command line REPL
    #[command(alias = "exit")]
    Quit,
}

// Our context can store all persistent state. Commands can also be implemented on our
// context. See: 'examples/runner.rs'
struct CommandContext;

fn quit() -> io::Result<CommandHandle<CommandContext, Stdout>> {
    let line_changes = HookStates::new(
        // Change the line state as soon as we return our new `InputHook`
        |repl_handle, _command_context| {
            repl_handle.disable_line_stylization();
            repl_handle.set_prompt_and_separator("Are you sure? (y/n)", ":");
            Ok(())
        },
        // Revert the line state if the user chooses not to quit
        |repl_handle, _command_context| {
            repl_handle.enable_line_stylization();
            repl_handle.set_default_prompt_and_separator();
            Ok(())
        },
    );

    // Since our `event_hook` does not return any `EventLoop::AsyncCallback` event we can use `with_new_uid`
    // here. If we wanted to modify the state of our `CommandContext` within our `input_hook` we could use
    // either callback type to do so. If said callback could error we would have to ensure that the error
    // has the same `UID` and the outer `InputHook`
    let input_event_hook = InputHook::with_new_uid(
        line_changes,
        // Define how our `InputEventHook` reacts to `KeyEvent`s of `KeyEventKind::Press`. This could also
        // easily be set up to only react apon enter, for simplicity we will just react apon press
        |repl_handle, _command_context, event| match event {
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
            ) => {
                repl_handle.clear_line()?;
                HookedEvent::break_repl()
            }
            _ => HookedEvent::release_hook(),
        },
    );

    Ok(CommandHandle::InsertHook(input_event_hook))
}

// Implement `Executor` so we can use `run`
impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        repl_handle: &mut Repl<Self, Stdout>,
        user_tokens: Vec<String>,
    ) -> io::Result<CommandHandle<Self, Stdout>> {
        match Command::try_parse_from(format_for_clap(&user_tokens)) {
            Ok(command) => match command {
                Command::Quit => quit(),
            },
            Err(err) => repl_handle
                .print_lines(err.render().ansi().to_string())
                .map(|_| CommandHandle::Processed),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // Build and run a new `Repl` with a custom quit command "quit" (given string will be ran through
    // `try_execute_command`) to be ran if the user tries to quit with 'ctrl + c' (when the line is empty)
    // or 'ctrl + d'
    repl_builder(io::stdout())
        .with_custom_quit_command("quit")
        .build()
        .expect("input writer accepts crossterm commands")
        .run(&mut CommandContext)
        .await
}
