// The basic `run` method requires the repl-oxide feature flag "runner"

use std::io::{self, Stdout};

use clap::Parser;
use crossterm::terminal;
use tokio::time::{sleep, Duration};

use repl_oxide::{format_for_clap, repl_builder, CommandHandle, Executor};

#[derive(Parser, Debug)]
enum Command {
    #[command(alias = "add")]
    Count {
        numbers: Option<Vec<isize>>,
    },
    Test,
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
    let mut repl = repl_builder()
        .terminal(io::stdout())
        .terminal_size(terminal::size()?)
        .build()
        .expect("all required inputs are provided & terminal accepts crossterm commands");

    repl.run(&mut CommandContext::default()).await?;
    Ok(())
}
