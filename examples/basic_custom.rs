// The helper macros for writing a custom loop requires the repl-oxide feature flag "macros"
/*                   cargo r --example basic-custom --features="macros"                   */

use repl_oxide::{
    clap::try_parse_from,
    executor::{CommandHandle, Executor},
    general_event_process, Repl, StreamExt,
};

use std::io::{self, Stdout};

use clap::Parser;
use tokio::time::{interval, Duration};

#[derive(Parser)]
#[command(
    name = "", // Leaving name empty will give us more accurate clap help and error messages
    about = "Example app demonstrating repl-oxide's macros feature flag"
)]
enum Command {
    /// Exit the command line REPL
    #[command(alias = "exit")]
    Quit,
}

// Our context can store all persistent state. Commands can also be implemented on our
// context. See: 'examples/runner.rs'
struct CommandContext;

// We can only use the library supplied `general_event_process!` macro if our `CommandContext`
// implements `Executor`
impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        repl_handle: &mut Repl<Self, Stdout>,
        user_tokens: Vec<String>,
    ) -> io::Result<CommandHandle<Self, Stdout>> {
        match try_parse_from(&user_tokens) {
            Ok(command) => match command {
                Command::Quit => Ok(CommandHandle::Exit),
            },
            Err(err) => repl_handle.print_clap_err(err),
        }
    }
}

fn save_cache() -> Result<(), &'static str> {
    Err("Failed to save cache")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    // Gain access to all terminal events
    let mut event_stream = crossterm::event::EventStream::new();

    // Create our type that implements `Executor`
    let mut command_ctx = CommandContext;

    // Build a default `Repl` to handle the repl state
    let mut repl = Repl::new(io::stdout())
        .build()
        .expect("input writer accepts crossterm commands");

    // Create a interval to schedule when a background event should happen
    let mut cache_updater = interval(Duration::from_secs(10));

    loop {
        // Disregard key inputs while user commands are being processed
        repl.clear_unwanted_inputs(&mut event_stream).await?;

        // Render the lines current state
        repl.render(&mut command_ctx)?;

        // Process async events as they happen
        tokio::select! {
            biased;

            // Process each event received from the event stream
            Some(event_result) = event_stream.next() => {
                // Use the library supplied default event processor
                general_event_process!(&mut repl, &mut command_ctx, event_result)
            }

            // Writing your own loop allows for awaiting custom events
            _ = cache_updater.tick() => {
                if let Err(err) = save_cache() {
                    repl.eprintln(err)?;
                }
            }
        }
    }

    Ok(())
}
