// Currently completion suggestions must be manually mapped out in a CONST context
/*               cargo r --example completion --features="runner"               */

use repl_oxide::{
    clap::try_parse_from,
    completion::{CommandScheme, InnerScheme, Parent, RecData, RecKind},
    executor::{CommandHandle, Executor},
    repl_builder, Repl,
};

use std::io::{self, Stdout};

use clap::{value_parser, Args, CommandFactory, Parser, ValueEnum};
use rand::Rng;

#[derive(Parser)]
#[command(
    name = "", // Leaving name empty will give us more accurate clap help and error messages
    about = "Example app showing repl-oxide's auto completion feature set \n\
            Use the 'tab' key to predict or walk through available commands"
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
        #[arg(long, short, value_parser = value_parser!(u8).range(2..=120))]
        sides: Option<u8>,
    },

    /// Exit the command line REPL
    #[command(aliases = ["Quit", "exit", "Exit"] )]
    Quit,
}

#[derive(Args)]
struct EchoArgs {
    /// Required string to echo
    string: String,

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
    #[value(alias = "Upper")]
    Upper,
}

const COMPLETION: &CommandScheme = &init_command_scheme();

// Array of our commands followed by their aliases
// Note: any alias must follow the last command
const COMMAND_RECS: [&str; 4] = ["echo", "roll", "quit", "exit"];

const fn init_command_scheme() -> CommandScheme {
    CommandScheme::new(
        RecData::new(RecKind::Command)
            .with_recommendations(&COMMAND_RECS)
            // Mapping of our command to it's aliases (eg. "quit" index -> "exit" index)
            .with_alias(&[(2, 3)]),
        COMMAND_INNER,
    )
}

// All command recommendations must have `ROOT` set as their parent
const COMMAND_INNER: &[InnerScheme; 3] = &[
    // echo
    InnerScheme::new(
        // Describe the recommendation kind as arguments where "echo" has one required input
        RecData::new(RecKind::argument_with_required_user_defined(1))
            .with_parent(Parent::Root)
            .with_recommendations(&["case", "reverse"])
            // Mapping of our echo arguments to their short counter part
            .with_short(&[(0, "c"), (1, "r")]),
        // Link to interior recommendation for the "echo" command
        Some(ECHO_INNER),
    ),
    // roll
    InnerScheme::new(
        // Describe the recommendation kind as arguments where "roll" has no required inputs
        RecData::new(RecKind::argument_with_no_required_inputs())
            .with_parent(Parent::Root)
            .with_recommendations(&["sides"])
            .with_short(&[(0, "s")]),
        // Link to interior recommendation for the "roll" command
        Some(ROLL_INNER),
    ),
    // quit
    // Describe "quit" as an end node
    InnerScheme::end(Parent::Root),
];

const ECHO_INNER: &[InnerScheme; 2] = &[
    // case
    InnerScheme::new(
        // Describe the recommendation kind as `value` (set enum) with max input of 1
        RecData::new(RecKind::value_with_num_args(1))
            .with_parent(Parent::Entry(COMMAND_RECS[0]))
            .with_recommendations(&["lower", "upper"]),
        None,
    ),
    // reverse
    // List the reverse command as a flag and is also not the end of the recommendation tree
    // since it doesn't matter the position of any 3 "echo" arguments
    InnerScheme::flag().with_parent(Parent::Entry(COMMAND_RECS[0])),
];

fn invalid_side_ct(value: &str) -> bool {
    value
        .parse::<u8>()
        .map_or(true, |v| !(2..=120).contains(&v))
}

const ROLL_INNER: &[InnerScheme; 1] = &[
    // sides
    // Set max user defined input of 1 and link to the parent command "roll"
    InnerScheme::user_defined(1)
        .with_parent(Parent::Entry(COMMAND_RECS[1]))
        .with_parsing_rule(invalid_side_ct),
];

// Our context can store all default/persistent state
struct CommandContext {
    dice_sides: u8,
}

impl Default for CommandContext {
    fn default() -> Self {
        Self { dice_sides: 6 }
    }
}

// Commands can be implemented on our context
impl CommandContext {
    fn roll(
        &mut self,
        repl_handle: &mut Repl<Self, Stdout>,
        input_dice: Option<u8>,
    ) -> io::Result<CommandHandle<Self, Stdout>> {
        if let Some(side_count) = input_dice {
            if side_count != self.dice_sides {
                self.dice_sides = side_count;
                repl_handle.println(format!("Updated dice side preference to {side_count}"))?;
            }
        }

        repl_handle.println(format!(
            "You rolled a {}",
            rand::rng().random_range(1..=self.dice_sides),
        ))?;

        Ok(CommandHandle::Processed)
    }

    fn echo(
        repl_handle: &mut Repl<Self, Stdout>,
        mut args: EchoArgs,
    ) -> io::Result<CommandHandle<Self, Stdout>> {
        if args.reverse {
            args.string = args.string.chars().rev().collect()
        }

        if let Some(case_option) = args.case {
            args.string = match case_option {
                CaseOptions::Lower => args.string.to_lowercase(),
                CaseOptions::Upper => args.string.to_uppercase(),
            }
        }

        repl_handle.println(args.string)?;
        Ok(CommandHandle::Processed)
    }
}

// Implement `Executor` so we can use `run`
impl Executor<Stdout> for CommandContext {
    async fn try_execute_command(
        &mut self,
        repl_handle: &mut Repl<Self, Stdout>,
        user_tokens: Vec<String>,
    ) -> io::Result<CommandHandle<Self, Stdout>> {
        match try_parse_from(&user_tokens) {
            Ok(command) => match command {
                Command::Echo { args } => CommandContext::echo(repl_handle, args),
                Command::Roll { sides } => self.roll(repl_handle, sides),
                Command::Quit => Ok(CommandHandle::Exit),
            },
            Err(err) => repl_handle.print_clap_err(err),
        }
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    Command::command().print_help()?;

    // Build and run a new repl with our const `CommandScheme` structure
    repl_builder(io::stdout())
        .with_completion(COMPLETION)
        .build()
        .expect("input writer accepts crossterm commands")
        .run(&mut CommandContext::default())
        .await
}
