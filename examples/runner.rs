// The basic `run` method requires the repl-oxide feature flag "runner"
/*           cargo r --example runner --features="runner"            */

use std::io::{self, Stdout};

use clap::{CommandFactory, Parser};
use tokio::time::{sleep, Duration};

use repl_oxide::{format_for_clap, repl_builder, CommandHandle, Executor};

#[derive(Parser, Debug)]
#[command(
    name = "Example App",
    about = "Example app showing repl-oxide's async and persistant state nature"
)]
enum Command {
    /// A running total of all inputted numbers
    #[command(alias = "add")]
    Count { numbers: Option<Vec<isize>> },

    /// Simulate some async tasks
    Test,

    /// Exit the command line REPL
    #[command(alias = "exit")]
    Quit,
}

type OurCommandHandle = CommandHandle<CommandContext, Stdout>;

// Our context can store all persistent state
#[derive(Default)]
struct CommandContext {
    count: isize,
}

// Commands can be implemented on our context
impl CommandContext {
    async fn async_test() -> io::Result<OurCommandHandle> {
        println!("Performing async tasks");
        sleep(Duration::from_secs(1)).await;
        Ok(CommandHandle::Processed)
    }

    fn count(&mut self, add: Option<Vec<isize>>) -> io::Result<OurCommandHandle> {
        if let Some(numbers) = add {
            numbers.into_iter().for_each(|n| self.count += n);
        }
        println!("Total seen: {}", self.count);
        Ok(CommandHandle::Processed)
    }
}

impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        user_tokens: Vec<String>,
    ) -> io::Result<OurCommandHandle> {
        match Command::try_parse_from(format_for_clap(user_tokens)) {
            Ok(command) => match command {
                Command::Count { numbers } => self.count(numbers),
                Command::Test => CommandContext::async_test().await,
                Command::Quit => Ok(CommandHandle::Exit),
            },
            Err(err) => {
                err.print()?;
                Ok(CommandHandle::Processed)
            }
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    Command::command()
        .print_help()
        .expect("Failed to print help");

    let mut command_ctx = CommandContext::default();
    let mut repl = repl_builder(io::stdout())
        .build()
        .expect("input writer accepts crossterm commands");

    // Start repl and await to finish
    repl.run(&mut command_ctx).await?;

    // Perform cleanup / process final state
    println!("Uploaded total count: {}, to server!", command_ctx.count);

    Ok(())
}
