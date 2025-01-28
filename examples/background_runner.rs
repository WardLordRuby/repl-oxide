// The `background-run` method requires the repl-oxide feature flag "background-runner"
/*        cargo r --example background-runner --features="background-runner"         */

use std::{
    fmt::Display,
    io::{self, Stdout},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use clap::Parser;
use crossterm::terminal;
use tokio::{
    sync::mpsc::Sender,
    time::{sleep, Duration},
};

use repl_oxide::{
    ansi_code::{GREEN, RED, WHITE},
    background_runner::flatten_join,
    format_for_clap, repl_builder, CommandHandle, Executor,
};

#[derive(Parser, Debug)]
enum Command {
    #[command(alias = "exit")]
    Quit,
}

type OurCommandHandle = CommandHandle<CommandContext, Stdout>;

// Our context can store all persistent state. Commands can also be implemented on our
// context See 'examples/runner.rs'
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
            Err(err) => {
                err.print()?;
                Ok(CommandHandle::Processed)
            }
        }
    }
}

// Our message type to send to the repl If we need to display text to the user outside of
// the `CommandContext` scope
enum Message {
    Err(String),
    Info(String),
}

// We must impl Display for our `Message` type so the repl knows how to display the text
// Note the repl loop will take care of appending a new line character
impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Message::Info(msg) => format!("{GREEN}{msg}{WHITE}"),
                Message::Err(msg) => format!("{RED}{msg}{WHITE}"),
            }
        )
    }
}

fn print_timer(active: Arc<AtomicBool>, sender: Sender<Message>) {
    tokio::spawn(async move {
        while active.load(Ordering::Relaxed) {
            sleep(Duration::from_secs(2)).await;
            if sender.send(Message::Info("Timer".into())).await.is_err() {
                break;
            };
        }
    });
}

// Simulate some async tcp request
async fn check_for_update() -> Result<(), &'static str> {
    sleep(Duration::from_secs(1)).await;
    Err("Bad response from server")
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let (repl_thread_handle, message_sender) = repl_builder()
        .terminal(io::stdout())
        .terminal_size(terminal::size()?)
        .build()
        .expect("all required inputs are provided & terminal accepts crossterm commands")
        .background_run(CommandContext);

    let repl_active = Arc::new(AtomicBool::new(true));

    print_timer(Arc::clone(&repl_active), message_sender.clone());

    if let Err(err) = check_for_update().await {
        let _ = message_sender.send(Message::Err(err.into())).await;
    }

    flatten_join(repl_thread_handle).await?;

    // Stop timer loop
    repl_active.store(false, Ordering::SeqCst);

    Ok(())
}
