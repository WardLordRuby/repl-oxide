// Currently completion suggestions must be manually mapped out in a CONST context
/*               cargo r --example completion --features="runner"               */

use std::io::{self, Stdout};

use clap::{Args, CommandFactory, Parser, ValueEnum};
use rand::Rng;

use repl_oxide::{
    completion::{CommandScheme, InnerScheme, RecData, RecKind, ROOT},
    executor::{format_for_clap, CommandHandle, Executor},
    repl_builder,
};

#[derive(Parser)]
#[command(
    name = "Example App",
    about = "Example app showing repl-oxide's auto completion feature set"
)]
enum Command {
    /// Repeats user input with optional transformations
    #[command(alias = "Echo")]
    Echo {
        #[clap(flatten)]
        args: EchoArgs,
    },

    /// Rolls a dice with an optional argument to specify the number of sides
    #[command(alias = "Roll")]
    Roll {
        /// Set your dice preference that the roll command should use
        #[arg(long, short)]
        sides: Option<usize>,
    },

    /// Exit the command line REPL
    #[command(aliases = ["Quit", "exit", "Exit"] )]
    Quit,
}

#[derive(Args)]
struct EchoArgs {
    /// Required string to echo
    input: String,

    /// Specify what case the input string should display as
    #[arg(long, short, value_enum)]
    case: Option<CaseOptions>,

    /// Flag to reverse the input string
    #[arg(long, short)]
    reverse: bool,
}

#[derive(ValueEnum, Clone, Copy)]
enum CaseOptions {
    #[value(alias = "Lower")]
    Lower,
    #[value(alias = "Uower")]
    Upper,
}

const COMPLETION: CommandScheme = init_command_scheme();

const fn init_command_scheme() -> CommandScheme {
    CommandScheme::new(
        RecData::command_set(
            // Specify our command alias map
            Some(&COMMANDS_ALIAS),
            // Specify our commands to recomend
            Some(&COMMAND_RECS),
            // Discribe that commands is not an end node
            false,
        ),
        &COMMAND_INNER,
    )
}

// Array of our commands followed by their aliases
// Note: any alias must follow the last command
const COMMAND_RECS: [&str; 4] = ["echo", "roll", "quit", "exit"];

// Mapping of our command to it's aliases (eg. "quit" index -> "exit" index)
const COMMANDS_ALIAS: [(usize, usize); 1] = [(2, 3)];

// Array of our recomendations for our "echo" command. Any aliases would follow as they did in `COMMAND_RECS`
const ECHO_RECS: [&str; 2] = ["case", "reverse"];

// Mapping of our echo arguments to their short counter part
const ECHO_SHORT: [(usize, &str); 2] = [(0, "c"), (1, "r")];

// Array of our recomendations for our `CaseOptions`
const ECHO_CASE_RECS: [&str; 2] = ["lower", "upper"];

const ROLL_RECS: [&str; 1] = ["sides"];
const ROLL_SHORT: [(usize, &str); 1] = [(0, "s")];

// All command recomendations must have `ROOT` set as their parent
const COMMAND_INNER: [InnerScheme; 3] = [
    // echo
    InnerScheme::new(
        RecData::new(
            Some(ROOT),
            // `ECHO_RECS` has no aliased names
            None,
            // Link to `ECHO_RECS` short counter parts
            Some(&ECHO_SHORT),
            // Specify the "echo" commands recommendations
            Some(&ECHO_RECS),
            // Discribe the recomendation kind as arguments
            RecKind::Argument,
            // List as not the end of the recomendation tree
            false,
        ),
        // Link to interior recomendations for the "echo" command
        Some(&ECHO_INNER),
    ),
    // roll
    InnerScheme::new(
        RecData::new(
            Some(ROOT),
            // `ROLL_RECS` has no aliased names
            None,
            // Link to `ROLL_RECS` short counter parts
            Some(&ROLL_SHORT),
            // Specify the "roll" commands recommendations
            Some(&ROLL_RECS),
            // Discribe the recomendation kind as arguments
            RecKind::Argument,
            // List as not the end of the recomendation tree
            false,
        ),
        // Link to interior recomendations for the "roll" command
        Some(&ROLL_INNER),
    ),
    // quit
    // Discribe "quit" as an end node
    InnerScheme::end(ROOT),
];

const ECHO_INNER: [InnerScheme; 2] = [
    // case
    InnerScheme::new(
        RecData::new(
            // Link to the parent command "echo"
            Some(COMMAND_RECS[0]),
            // `ECHO_CASE_RECS` has no aliased names
            None,
            // `ECHO_CASE_RECS` has no short counter parts
            None,
            // Specify the "case" argument recomendations
            Some(&ECHO_CASE_RECS),
            // Discribe the recomendation kind as `value` (set enum) with max input of 1
            RecKind::value_with_num_args(1),
            // List as not the end of the recomendation tree
            false,
        ),
        None,
    ),
    // reverse
    // List the reverse command as a flag and is also not the end of the recomendation tree
    // since it doesn't matter the position of any 3 "echo" arguments
    InnerScheme::flag(COMMAND_RECS[0], false),
];

const ROLL_INNER: [InnerScheme; 1] = [
    // sides
    InnerScheme::empty_with(
        // Link to the parent command "roll"
        COMMAND_RECS[1],
        // Discribe the recomendation kind as `UserDefinded` with max input of 1
        RecKind::user_defined_with_num_args(1),
        // List as not the end of the recomendation tree
        false,
    ),
];

type OurCommandHandle = CommandHandle<CommandContext, Stdout>;

// Our context can store all default/persistent state
struct CommandContext {
    dice_sides: usize,
}

impl Default for CommandContext {
    fn default() -> Self {
        Self { dice_sides: 6 }
    }
}

// Commands can be implemented on our context
impl CommandContext {
    fn roll(&mut self, new_sides: Option<usize>) -> io::Result<OurCommandHandle> {
        if let Some(side_count) = new_sides {
            if side_count != self.dice_sides {
                self.dice_sides = side_count;
                println!("Updated dice side preference to {side_count}");
            }
        }

        println!(
            "You rolled a {}",
            rand::rng().random_range(1..self.dice_sides)
        );

        Ok(CommandHandle::Processed)
    }

    fn echo(mut args: EchoArgs) -> io::Result<OurCommandHandle> {
        if args.reverse {
            args.input = args.input.chars().rev().collect()
        }

        if let Some(case_option) = args.case {
            args.input = match case_option {
                CaseOptions::Lower => args.input.to_lowercase(),
                CaseOptions::Upper => args.input.to_uppercase(),
            }
        }

        println!("{}", args.input);
        Ok(CommandHandle::Processed)
    }
}

// Implement `Executor` so we can use `run`
impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        user_tokens: Vec<String>,
    ) -> io::Result<OurCommandHandle> {
        match Command::try_parse_from(format_for_clap(user_tokens)) {
            Ok(command) => match command {
                Command::Echo { args } => CommandContext::echo(args),
                Command::Roll { sides } => self.roll(sides),
                Command::Quit => Ok(CommandHandle::Exit),
            },
            Err(err) => err.print().map(|_| CommandHandle::Processed),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    Command::command().print_help()?;

    // Build and run a new repl with our const `CommandScheme` structure
    repl_builder(io::stdout())
        .with_completion(&COMPLETION)
        .build()
        .expect("input writer accepts crossterm commands")
        .run(&mut CommandContext::default())
        .await
}
