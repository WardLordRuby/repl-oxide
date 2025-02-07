// The basic `run` method requires the repl-oxide feature flag "runner"
/*           cargo r --example runner --features="runner"            */

use std::io;

use clap::{CommandFactory, Parser};
use tokio::time::{sleep, Duration};

use repl_oxide::{
    executor::{format_for_clap, CommandHandle, Executor},
    println, repl_builder,
};

#[derive(Parser)]
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

// Our context can store all persistent state
#[derive(Default)]
struct CommandContext {
    count: isize,
}

// Commands can be implemented on our context
impl CommandContext {
    async fn async_test() -> io::Result<CommandHandle<CommandContext>> {
        println("Performing async tasks")?;
        let t_1 = tokio::spawn(async {
            sleep(Duration::from_secs(1)).await;
            println("Finished task 1")
        });
        let t_2 = tokio::spawn(async {
            sleep(Duration::from_secs(2)).await;
            println("Finished task 2")
        });
        let (res_1, res_2) = tokio::try_join!(t_1, t_2)?;
        res_1?;
        res_2?;

        Ok(CommandHandle::Processed)
    }

    fn count(&mut self, add: Option<Vec<isize>>) -> io::Result<CommandHandle<CommandContext>> {
        if let Some(numbers) = add {
            numbers.into_iter().for_each(|n| self.count += n);
        }
        println(format!("Total seen: {}", self.count))?;
        Ok(CommandHandle::Processed)
    }
}

impl Executor for CommandContext {
    async fn try_execute_command(
        &mut self,
        user_tokens: Vec<String>,
    ) -> io::Result<CommandHandle<CommandContext>> {
        match Command::try_parse_from(format_for_clap(user_tokens)) {
            Ok(command) => match command {
                Command::Count { numbers } => self.count(numbers),
                Command::Test => CommandContext::async_test().await,
                Command::Quit => Ok(CommandHandle::Exit),
            },
            Err(err) => println(err.render().ansi().to_string()).map(|_| CommandHandle::Processed),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    Command::command().print_help()?;

    let mut command_ctx = CommandContext::default();
    let mut repl = repl_builder(io::stdout())
        .build()
        .expect("input writer accepts crossterm commands");

    // Start repl and await to finish
    repl.run(&mut command_ctx).await?;

    // Perform cleanup / process final state
    println(format!(
        "Uploaded total count: {}, to server!",
        command_ctx.count
    ))
}
