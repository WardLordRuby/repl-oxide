// The helper macros for writing a custom loop requires the repl-oxide feature flag "macros"
/*                   cargo r --example basic-custom --features="macros"                   */

use repl_oxide::{
    ansi_code::{RED, RESET},
    executor::{format_for_clap, CommandHandle, Executor},
    general_event_process, repl_builder, LineReader, StreamExt,
};

use std::{
    fmt::Display,
    io::{self, Stdout},
};

use clap::Parser;
use tokio::{
    sync::mpsc::Sender,
    time::{sleep, Duration},
};

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

type OurCommandHandle = CommandHandle<CommandContext, Stdout>;

// Our context can store all persistent state. Commands can also be implemented on our
// context. See: 'examples/runner.rs'
struct CommandContext;

// We can only use the library supplied `general_event_process!` macro if our `CommandContext`
// implements `Executor`
impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        _repl_handle: &mut LineReader<Self, Stdout>,
        user_tokens: Vec<String>,
    ) -> io::Result<OurCommandHandle> {
        match Command::try_parse_from(format_for_clap(user_tokens)) {
            Ok(command) => match command {
                Command::Quit => Ok(CommandHandle::Exit),
            },
            Err(err) => err.print().map(|_| CommandHandle::Processed),
        }
    }
}

// Our message type to send to the repl If we need to display text to the user outside of
// the `CommandContext` scope
struct ErrorMsg<T: Display>(T);

// We must impl `Display` for our `Message` type so the repl knows how to display the text
// Note the repl loop will take care of appending a new line character
impl<T: Display> Display for ErrorMsg<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{RED}{}{RESET}", self.0)
    }
}

fn save_cache_every(period: Duration, sender: Sender<()>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            sleep(period).await;
            if sender.send(()).await.is_err() {
                break;
            };
        }
    })
}

fn save_cache() -> Result<(), ErrorMsg<&'static str>> {
    Err(ErrorMsg("Failed to save cache"))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    // Gain access to all terminal events
    let mut event_stream = crossterm::event::EventStream::new();

    // Create our type that implements `Executor`
    let mut command_ctx = CommandContext;

    // Build a default `LineReader` to handle the repl state
    let mut repl = repl_builder(io::stdout())
        .build()
        .expect("input writer accepts crossterm commands");

    // Create a channel to tell our repl when the cache should be updated
    let (update_tx, mut update_rx) = tokio::sync::mpsc::channel(10);

    // Spawn example save cache event
    let timer_loop = save_cache_every(Duration::from_secs(10), update_tx);

    loop {
        // Disregard key inputs while user commands are being processed
        repl.clear_unwanted_inputs(&mut event_stream).await?;

        // Render the lines current state
        repl.render(&mut command_ctx)?;

        // Process async events as they happen
        tokio::select! {
            biased;

            // Process each event recieved from the event stream
            Some(event_result) = event_stream.next() => {
                // Use the library supplied default event processor
                general_event_process!(&mut repl, &mut command_ctx, event_result)
            }

            // Writing your own loop allows for awaiting custom events
            Some(_) = update_rx.recv() => {
                if let Err(err) = save_cache() {
                    repl.print_background_msg(err)?;
                }
            }
        }
    }

    // Stop timer loop
    timer_loop.abort();

    Ok(())
}
