// The `spawn` method requires the repl-oxide feature flag "spawner"
/*         cargo r --example spawner --features="spawner"          */

use std::{
    fmt::Display,
    io::{self, Stdout},
};

use clap::Parser;
use tokio::{
    sync::mpsc::Sender,
    time::{sleep, Duration},
};

use repl_oxide::{
    ansi_code::{GREEN, RED, RESET},
    executor::{format_for_clap, CommandHandle, Executor},
    repl_builder,
};

#[derive(Parser)]
#[command(
    name = "", // Leaving name empty will give us more accurate clap help and error messages
    about = "Example app demonstrating repl-oxide's spawner feature"
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

impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
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
enum Message {
    Err(String),
    Info(String),
}

// We must impl `Display` for our `Message` type so the repl knows how to display the text
// Note the repl loop will take care of appending a new line character
impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (line_color, msg): (&str, &str) = match self {
            Message::Info(msg) => (GREEN, msg),
            Message::Err(msg) => (RED, msg),
        };
        write!(f, "{line_color}{msg}{RESET}")
    }
}

fn print_timer(sender: Sender<Message>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(2)).await;
            if sender.send(Message::Info("Timer".into())).await.is_err() {
                break;
            };
        }
    })
}

async fn check_for_update() -> Result<(), &'static str> {
    sleep(Duration::from_secs(1)).await;
    Err("Bad response from server")
}

#[tokio::main]
async fn main() -> io::Result<()> {
    // Spawn repl on the current runtime
    let (repl_task, message_sender) = repl_builder(io::stdout())
        .build()
        .expect("input writer accepts crossterm commands")
        .spawn(CommandContext);

    // Spawn example background printer
    let timer_loop = print_timer(message_sender.clone());

    // Simulate some async tcp request
    if let Err(err) = check_for_update().await {
        let _ = message_sender.send(Message::Err(err.into())).await;
    }

    // Await repl to finish
    repl_task.await??;

    // Stop timer loop
    timer_loop.abort();

    Ok(())
}
