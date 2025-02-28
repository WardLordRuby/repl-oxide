// The basic `run` method requires the repl-oxide feature flag "runner"
/*           cargo r --example runner --features="runner"            */

use repl_oxide::{
    executor::{format_for_clap, CommandHandle, Executor},
    println, repl_builder, LineReader,
};

use std::io::{self, Stdout};

use clap::{CommandFactory, Parser};
use tokio::time::{sleep, Duration};

#[derive(Parser)]
#[command(
    name = "", // Leaving name empty will give us more accurate clap help and error messages
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
    async fn async_test(
        repl_handle: &mut LineReader<Self, Stdout>,
    ) -> io::Result<OurCommandHandle> {
        repl_handle.println("Performing async tasks")?;

        // MARK: XXX
        // What is the prefered method to access the handle over thread bounds?
        // Is it acceptable to force the user to supply the exclusive reference to
        // the repl's writer?

        let t_1 = tokio::spawn(async {
            sleep(Duration::from_secs(1)).await;
            println(&mut io::stdout(), "Finished task 1")
        });
        let t_2 = tokio::spawn(async {
            sleep(Duration::from_secs(2)).await;
            println(&mut io::stdout(), "Finished task 2")
        });
        let (r_1, r_2) = tokio::try_join!(t_1, t_2).unwrap();

        r_1?;
        r_2?;

        Ok(CommandHandle::Processed)
    }

    fn count(
        &mut self,
        repl_handle: &mut LineReader<Self, Stdout>,
        add: Option<Vec<isize>>,
    ) -> io::Result<OurCommandHandle> {
        if let Some(numbers) = add {
            numbers.into_iter().for_each(|n| self.count += n);
        }
        repl_handle.println(format!("Total seen: {}", self.count))?;
        Ok(CommandHandle::Processed)
    }
}

impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        repl_handle: &mut LineReader<Self, Stdout>,
        user_tokens: Vec<String>,
    ) -> io::Result<OurCommandHandle> {
        match Command::try_parse_from(format_for_clap(user_tokens)) {
            Ok(command) => match command {
                Command::Count { numbers } => self.count(repl_handle, numbers),
                Command::Test => CommandContext::async_test(repl_handle).await,
                Command::Quit => Ok(CommandHandle::Exit),
            },
            Err(err) => repl_handle
                .println(err.render().ansi().to_string())
                .map(|_| CommandHandle::Processed),
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
    repl.println(format!(
        "Uploaded total count: {}, to server!",
        command_ctx.count
    ))?;

    Ok(())
}
