use crate::line::Repl;

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    io::{self, Write},
    ops::Range,
};

// use crate::get_debugger;

// const `split_at` (UNSTABLE)
const HELP_STR: &str = "help";
const HELP_SHORT: &str = "h";

const HELP_ARG: &str = "--help";
const HELP_ARG_SHORT: &str = "-h";

const USER_INPUT: i8 = -1;
const COMMANDS: usize = 0;
const INVALID: usize = 1;
const VALID: usize = 2;
const HELP: usize = 3;

// MARK: IMPROVE
// we could solve name-space collisions by making the data structure into a prefix-tree
// this gets increasingly problematic to continue to support short arg syntax

/// The current structure that holds all completion items and meta-data
///
/// The current implementation only works when the name-space of commands, arguments, and their aliases/shorts
/// do not overlap, overlapping names must return the exact same `RecData`, help is special cased to work as
/// both a command and argument `inner` must ALWAYS contain the same number of elements as `commands.starting_alias`
pub struct CommandScheme {
    /// command names followed by aliases
    commands: RecData,

    /// static empty node used for invalid inputs
    invalid: RecData,

    /// static empty node used for valid inputs of `RecKind::Value`
    valid: RecData,

    /// static help node used for adding help args/commands
    help: RecData,

    /// inner data shares indices with `commands.recs`
    inner: &'static [InnerScheme],
}

// MARK: TODO
// 1. Prototype user experience with builder? (`str::trim_ascii`(1.80) && `str::make_ascii_lowercase`(1.84) are const methods)
// 2. Add support for recursive commands
//    currently we only support commands that only take one command as a clap value enum
//    we should be able to have interior commands still have args/flags ect..

/// Tree node of [`CommandScheme`]
///
/// Notes:  
/// - Recommendations within `data` set as `RecKind::Value` will be flattened into a HashSet.  
///   Access to the set is provided through a separate map `value_sets` where the lookup key  
///   is the index you get back from `rec_map` when hashing the parent node  
/// - `RecKinds`: `Value` and `UserInput` must provide a `Range<usize>` of inputs that are expected to follow  
///
/// field `data` must adhere to the following  
///  - kind can not be `RecKind::Command` commands are only supported at the top level. If a command has
///    sub-commands use `RecKind::Value` Note: since the 'command' is seen as a 'value' currently it can not have
///    it's own arguments/flags ect...
///
/// field `inner` must adhere to the following
///  - if `data.kind` is `RecKind::Argument` `inner` must contain the same number of elements as `data.starting_alias`  
///  - for all other kinds `inner` must be `None`
pub struct InnerScheme {
    /// Data that describes recommendations context
    data: RecData,

    /// Inner data shares indices with `data.recs`
    inner: Option<&'static [Self]>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecData {
    /// Name of the parent entry
    parent: Option<Parent>,
    /// Required data if this node contains any aliases
    // Index of rec in `recs` -> index of alias in `recs`
    alias: Option<&'static [(usize, usize)]>,
    /// Required data if containing recs support a short arg syntax
    short: Option<&'static [(usize, &'static str)]>,
    /// Recommendations followed by recommendation aliases
    // Index of rec in `recs` -> short char
    recs: Option<&'static [&'static str]>,
    /// Kind of data stored
    pub(super) kind: RecKind,
    /// Signals this is a leaf node
    end: bool,
    /// Used to validate help arguments
    has_help: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parent {
    Root,
    Universal,
    Entry(&'static str),
}

impl CommandScheme {
    pub const fn new(commands: RecData, inner: &'static [InnerScheme]) -> Self {
        Self {
            commands,
            invalid: RecData::empty(),
            valid: RecData::empty(),
            help: RecData::help(),
            inner,
        }
    }
}

impl InnerScheme {
    pub const fn new(data: RecData, inner: Option<&'static [Self]>) -> Self {
        Self { data, inner }
    }

    /// Most of the time you will want to set `parent` after. See: [`Self::with_parent`]
    pub const fn flag() -> Self {
        Self {
            data: RecData::new(RecKind::ArgFlag),
            inner: None,
        }
    }

    /// Minimum of 1 arg is assumed  
    /// Most of the time you will want to set `parent` after. See: [`Self::with_parent`]
    pub const fn user_defined(max_args: usize) -> Self {
        Self {
            data: RecData::new(RecKind::user_defined_with_num_args(max_args)),
            inner: None,
        }
    }

    /// **Note**: parsing rules are **only** valid when kind is `RecKind::UserDefined`  
    /// Function should return `true` if `&str` is valid
    pub const fn with_parsing_rule(mut self, f: fn(&str) -> bool) -> Self {
        self.data.kind = match self.data.kind {
            RecKind::UserDefined {
                range,
                parse_fn: prev,
            } => {
                assert!(prev.is_none(), "Only one parse rule is supported");
                RecKind::UserDefined {
                    range,
                    parse_fn: Some(f),
                }
            }
            _ => panic!("Tried to add a parse rule to an unsupported `RecKind`"),
        };
        self
    }

    pub const fn end(parent: Parent) -> Self {
        Self {
            data: RecData::empty().with_parent(parent),
            inner: None,
        }
    }

    pub const fn with_parent(mut self, parent: Parent) -> Self {
        self.data.parent = Some(parent);
        self
    }

    pub const fn without_help(mut self) -> Self {
        self.data.has_help = false;
        self
    }

    pub const fn set_end(mut self) -> Self {
        self.data.end = true;
        self
    }
}

impl RecData {
    pub const fn new(kind: RecKind) -> Self {
        Self {
            parent: None,
            alias: None,
            short: None,
            recs: None,
            kind,
            end: false,
            has_help: true,
        }
    }

    /// Setting a help recommendation and modifying it will result in undefined behavior
    pub const fn help() -> Self {
        Self {
            parent: Some(Parent::Universal),
            alias: None,
            short: None,
            recs: None,
            kind: RecKind::Help,
            end: true,
            has_help: false,
        }
    }

    const fn empty() -> Self {
        Self {
            parent: None,
            alias: None,
            short: None,
            recs: None,
            kind: RecKind::Null,
            end: true,
            has_help: false,
        }
    }

    pub const fn with_parent(mut self, parent: Parent) -> Self {
        self.parent = Some(parent);
        self
    }

    pub const fn with_alias(mut self, alias: &'static [(usize, usize)]) -> Self {
        self.alias = Some(alias);
        self
    }

    pub const fn with_short(mut self, short: &'static [(usize, &'static str)]) -> Self {
        self.short = Some(short);
        self
    }

    pub const fn with_recommendations(mut self, recs: &'static [&'static str]) -> Self {
        self.recs = Some(recs);
        self
    }

    pub const fn set_end(mut self) -> Self {
        self.end = true;
        self
    }

    pub const fn without_help(mut self) -> Self {
        self.has_help = false;
        self
    }

    #[inline]
    fn rec_len(&self) -> usize {
        self.recs.map(|recs| recs.len()).unwrap_or_default()
    }

    fn unique_rec_end(&self) -> usize {
        let len = self.rec_len();
        self.alias
            .as_ref()
            .map_or(len, |alias_mapping| len - alias_mapping.len())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RecKind {
    Command,
    Argument(usize),
    ArgFlag,
    Value(Range<usize>),
    UserDefined {
        range: Range<usize>,
        parse_fn: Option<fn(&str) -> bool>,
    },
    Help,
    Null,
}

impl RecKind {
    /// Use when the command has no required number of user inputs
    pub const fn argument_with_no_required_inputs() -> Self {
        Self::Argument(0)
    }

    /// Use when the command has a required number of user inputs
    pub const fn argument_with_required_user_defined(required: usize) -> Self {
        Self::Argument(required)
    }

    /// Minimum of 1 arg is assumed
    pub const fn value_with_num_args(max: usize) -> Self {
        Self::Value(Range {
            start: 1,
            end: max.saturating_add(1),
        })
    }

    /// Minimum of 1 arg is assumed
    const fn user_defined_with_num_args(max: usize) -> Self {
        Self::UserDefined {
            range: Range {
                start: 1,
                end: max.saturating_add(1),
            },
            parse_fn: None,
        }
    }
}

pub enum Direction {
    Next,
    Previous,
}

impl Direction {
    #[inline]
    fn to_int(&self) -> i8 {
        match self {
            Direction::Next => 1,
            Direction::Previous => -1,
        }
    }
}

impl From<&'static CommandScheme> for Completion {
    fn from(value: &'static CommandScheme) -> Self {
        fn insert_index(
            map: &mut HashMap<&'static str, usize>,
            key: &'static str,
            val: usize,
            data: &'static RecData,
            list: &[&'static RecData],
        ) {
            assert!(
                match map.insert(key, val) {
                    None => true,
                    Some(j) => list[j] == data,
                },
                "duplicate recommendation entries _must_ have identical nodes"
            );
        }
        fn try_insert_rec_set(
            kind: &RecKind,
            map: &mut HashMap<usize, HashSet<&'static str>>,
            recs: Option<&'static [&'static str]>,
            at: usize,
        ) {
            if let RecKind::Value(ref r) = kind {
                assert!(
                    r.start > 0,
                    "Values must have at least one required static input"
                );
                assert!(map
                    .insert(
                        at,
                        HashSet::from_iter(
                            recs.expect("`RecKind::Value` specified but no pre-determined values were supplied")
                                .iter()
                                .copied()
                        )
                    )
                    .is_none())
            }
        }
        fn try_insert_aliases(
            map: &mut HashMap<&'static str, usize>,
            val: usize,
            data: &'static RecData,
            list: &[&'static RecData],
            mapping: Option<&'static [(usize, usize)]>,
            recs: Option<&'static [&'static str]>,
            target: usize,
        ) {
            if let Some(rec_mapping) = mapping {
                rec_mapping
                    .iter()
                    .filter(|(rec_i, _)| *rec_i == target)
                    .map(|&(_, alias_i)| {
                        recs.expect("tried to set alias when no recommendations were supplied")
                            [alias_i]
                    })
                    .for_each(|alias| {
                        insert_index(map, alias, val, data, list);
                    });
            }
        }

        fn walk_inner(
            inner: &'static InnerScheme,
            list: &mut Vec<&'static RecData>,
            map: &mut HashMap<&'static str, usize>,
            value_sets: &mut HashMap<usize, HashSet<&'static str>>,
        ) {
            match inner.data {
                RecData {
                    ref alias,
                    ref recs,
                    ref short,
                    kind: RecKind::Argument(_),
                    ..
                } => {
                    let expected_len = inner.data.unique_rec_end();
                    let inner_inner = inner.inner.expect("inner elements not described");
                    assert_eq!(
                        expected_len,
                        inner_inner.len(),
                        "invalid number of inner element descriptions"
                    );
                    for (i, (&argument, inner)) in recs
                        .expect("is some")
                        .iter()
                        .zip(inner_inner)
                        .enumerate()
                        .take(expected_len)
                    {
                        list.push(&inner.data);
                        let l_i = list.len() - 1;
                        insert_index(map, argument, l_i, &inner.data, list);
                        if let Some(&(_, short_ch)) = short.and_then(|short_mapping| {
                            short_mapping.iter().find(|(map_i, _)| *map_i == i)
                        }) {
                            assert_ne!(short_ch, HELP_SHORT, "the use of 'h' is not allowed, short arg '-h' is reserved for 'help'");
                            assert!(
                                short_ch.chars().count() == 1,
                                "Short: {short_ch}, is not a valid short format"
                            );
                            insert_index(map, short_ch, l_i, &inner.data, list);
                        }
                        try_insert_aliases(map, l_i, &inner.data, list, *alias, *recs, i);
                        try_insert_rec_set(&inner.data.kind, value_sets, inner.data.recs, l_i);
                        walk_inner(inner, list, map, value_sets);
                    }
                }
                _ => {
                    assert!(
                        inner.inner.is_none(),
                        "currently it is only valid to provide inner descriptions for arguments"
                    );
                    assert!(
                        inner.data.short.is_none(),
                        "shorts are only supported for arguments"
                    );
                }
            }
        }

        let expected_len = value.commands.unique_rec_end();
        assert_eq!(expected_len, value.inner.len());
        assert!(value.commands.short.is_none());
        let mut rec_map = HashMap::new();
        let mut value_sets = HashMap::new();
        let mut rec_list = vec![&value.commands, &value.invalid, &value.valid, &value.help];
        rec_map.insert(HELP_STR, HELP);
        for (i, (&command, inner)) in value
            .commands
            .recs
            .expect("is some")
            .iter()
            .zip(value.inner.iter())
            .enumerate()
            .take(expected_len)
        {
            rec_list.push(&inner.data);
            let l_i = rec_list.len() - 1;
            insert_index(&mut rec_map, command, l_i, &inner.data, &rec_list);
            try_insert_aliases(
                &mut rec_map,
                l_i,
                &inner.data,
                &rec_list,
                value.commands.alias,
                value.commands.recs,
                i,
            );
            try_insert_rec_set(&inner.data.kind, &mut value_sets, inner.data.recs, l_i);
            walk_inner(inner, &mut rec_list, &mut rec_map, &mut value_sets);
        }
        let mut recommendations = value
            .commands
            .recs
            .expect("`CommandScheme` supplied with no recommendations")[..expected_len]
            .to_vec();
        recommendations.push(HELP_STR);
        Self {
            recommendations,
            input: CompletionState::default(),
            rec_map,
            rec_list: rec_list.into_boxed_slice(),
            value_sets,
            indexer: Indexer::default(),
        }
    }
}

/// On startup the [`CommandScheme`] tree structure gets flattened into this structure
///
/// The goal of `Completion` is to provide efficient lookups to the correct data that should be used to
/// compute the best recommendations for the user with any given input. `Completion` also holds the current
/// line state in field `input` `CompletionState` aims to provide accurate slices into the string
/// `Repl.line.input` since this struct is nested within `Repl` we manage str slicing by indexes and lens  
#[derive(Default)]
pub struct Completion {
    pub(super) recommendations: Vec<&'static str>,
    input: CompletionState,
    indexer: Indexer,
    rec_list: Box<[&'static RecData]>,
    rec_map: HashMap<&'static str, usize>,
    value_sets: HashMap<usize, HashSet<&'static str>>,
}

/// `Indexer` keeps track of various indexes for the current suggestion state
struct Indexer {
    /// `list.0` points to the currently used [`RecData`] in [`Completion.rec_list`]  
    /// `list.1` is only used when `Self.multiple`
    ///
    /// [`Completion.rec_list`]: Completion
    list: (usize, usize),

    /// Flag meaning more than one category of recommendations are valid at the same time, and the index
    /// `Self.list.1` should be used to give accurate recommendations
    multiple: bool,

    /// Collection of indexes of entries within [`Completion.recommendations`] that were added from `Self.list.1`
    ///
    /// [`Completion.recommendations`]: Completion
    in_list_2: Vec<i8>,

    /// The index of the currently suggested recommendation within [`Completion.recommendations`]  
    /// This value is a signed int because [`USER_INPUT`] is used as marker of when it is time to loop back around
    /// and recommend the original text that the user used to start the completion chain.
    ///
    /// This index can be back traced to its [`RecData`] via [`Completion::rec_data_from_index`]
    ///
    /// [`Completion.recommendations`]: Completion
    recs: i8,
}

impl Default for Indexer {
    fn default() -> Self {
        Indexer {
            list: (COMMANDS, INVALID),
            multiple: false,
            in_list_2: Vec::new(),
            recs: USER_INPUT,
        }
    }
}

#[derive(Default)]
struct CompletionState {
    curr_command: Option<SliceData>,
    curr_argument: Option<SliceData>,
    curr_value: Option<SliceData>,
    required_input_i: Vec<usize>,
    ending: LineEnd,
}

#[derive(Clone, Copy, Default, Debug)]
/// Represents a `&str` into `LineData.input.trim_start()`
struct SliceData {
    byte_start: usize,
    slice_len: usize,
    hash_i: usize,
}

impl PartialEq for SliceData {
    fn eq(&self, other: &Self) -> bool {
        self.byte_start == other.byte_start && self.slice_len == other.slice_len
    }
}

impl Eq for SliceData {}

impl SliceData {
    fn exact_eq(&self, other: &Self) -> bool {
        self == other && self.hash_i == other.hash_i
    }

    /* -------------------------------- Debug tool --------------------------------------- */
    // fn display(&self, line_trim_start: &str) -> String {
    //     format!(
    //         "slice: '{}', hash_i: {}",
    //         self.to_slice_unchecked(line_trim_start),
    //         self.hash_i
    //     )
    // }
    /* ----------------------------------------------------------------------------------- */
}

#[derive(Default, Debug)]
struct LineEnd {
    token: String,
    open_quote: Option<(usize, char)>,
}

impl CompletionState {
    /// returns `true` if method modifies `self`
    fn check_state(&mut self, line: &str) -> bool {
        let mut state_modified = false;
        if let Some(&required_user_defined_token_idx) = self.required_input_i.last() {
            if line.trim_end().len() == required_user_defined_token_idx {
                self.required_input_i.pop();
                state_modified = true;
            }
        }
        if let Some(ref command) = self.curr_command {
            if line.len() == command.byte_start + command.slice_len {
                (self.curr_command, self.curr_argument, self.curr_value) = (None, None, None);
                return true;
            }
        }
        if let Some(ref arg) = self.curr_argument {
            if line.len() == arg.byte_start + arg.slice_len {
                (self.curr_argument, self.curr_value) = (None, None);
                return true;
            }
        }
        if let Some(ref value) = self.curr_value {
            if line.len() == value.byte_start + value.slice_len {
                self.curr_value = None;
                return true;
            }
        }
        state_modified
    }

    fn update_curr_token(&mut self, line: &str) {
        let curr_token = line
            .rsplit_once(char::is_whitespace)
            .map_or(line, |(_, suf)| suf);
        self.ending.token = if let Some((l_i, quote)) = self.ending.open_quote {
            if let Some(r_i) = line.rfind(quote) {
                if l_i < r_i {
                    self.ending.open_quote = None;
                    &line[l_i..=r_i]
                } else {
                    let str = &line[l_i..];
                    if !str.starts_with(quote) {
                        self.ending.open_quote = None;
                    }
                    str
                }
            } else {
                self.ending.open_quote = None;
                curr_token
            }
        } else {
            let starting_quote = self
                .ending
                .token
                .starts_with(['\'', '\"'])
                .then(|| self.ending.token.chars().next().expect("starts with quote"));

            let r_find_quote = match starting_quote {
                Some(quote) => line.char_indices().rfind(|&(_, c)| c == quote),
                None => line.char_indices().rfind(|&(_, c)| c == '\'' || c == '\"'),
            };

            if let Some((r_i, quote)) = r_find_quote {
                let quote_num = line.chars().filter(|&c| c == quote).count();
                if quote_num & 1 == 0 {
                    if curr_token.ends_with(quote) {
                        let l_i = line[..r_i].rfind(quote).expect("quote num even");
                        &line[l_i..=r_i]
                    } else {
                        curr_token
                    }
                } else {
                    self.ending.open_quote = Some((r_i, quote));
                    &line[r_i..]
                }
            } else {
                curr_token
            }
        }
        .to_string();
    }
    /* -------------------------------- Debug tool --------------------------------------- */
    // fn debug(&self, line: &str) -> String {
    //     let inner_fmt = |slice_data: &SliceData| slice_data.display(line);

    //     let mut output = String::new();
    //     output.push_str(&format!(
    //         "\n    curr_command: {:?}\n",
    //         self.curr_command.as_ref().map(inner_fmt)
    //     ));
    //     output.push_str(&format!(
    //         "    curr_arg: {:?}\n",
    //         self.curr_argument.as_ref().map(inner_fmt)
    //     ));
    //     output.push_str(&format!(
    //         "    curr_value: {:?}\n",
    //         self.curr_value.as_ref().map(inner_fmt)
    //     ));
    //     output.push_str(&format!(
    //         "    required_input_ct: {}, last idx: {:?}\n",
    //         self.required_input_i.len(),
    //         self.required_input_i.last()
    //     ));
    //     output.push_str(&format!("    user_input: {:?}\n", self.ending));
    //     output
    // }
    /* ----------------------------------------------------------------------------------- */
}

trait Validity {
    /// returns `true` if `Some(hash_i == INVALID)` else `false`
    fn is_some_and_invalid(&self) -> bool;
}

impl Validity for Option<&SliceData> {
    #[inline]
    fn is_some_and_invalid(&self) -> bool {
        matches!(self, Some(SliceData { hash_i, ..}) if *hash_i == INVALID)
    }
}

impl SliceData {
    /// Caller is responsible for making the given `byte_start`, `slice_len` are valid indices into the given `line`
    fn from_raw_unchecked(
        byte_start: usize,
        slice_len: usize,
        expected: &RecKind,
        line: &str,
        completion: &Completion,
        arg_count: Option<usize>,
    ) -> Self {
        let mut data = SliceData {
            byte_start,
            slice_len,
            hash_i: INVALID,
        };
        match expected {
            RecKind::Command => completion.hash_command_unchecked(line, &mut data),
            RecKind::Argument(_) => completion.hash_arg_unchecked(line, &mut data),
            RecKind::Value(range) => {
                completion.hash_value_unchecked(line, &mut data, range, arg_count.unwrap_or(1))
            }
            RecKind::UserDefined { range, parse_fn } => {
                if range.contains(&arg_count.unwrap_or(1))
                    && parse_fn.map_or(true, |valid| valid(data.to_slice_unchecked(line)))
                {
                    data.hash_i = VALID
                }
            }
            _ => (),
        }
        data
    }

    /// Caller must ensure that the input line is: `LineData.input.trim_start()` otherwise this
    /// method will panic as it performs a manual slice into the input `line`
    fn to_slice_unchecked(self, line: &str) -> &str {
        &line[self.byte_start..self.byte_start + self.slice_len]
    }
}

impl Completion {
    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.rec_list.is_empty()
    }

    /// Acquires the [`RecData`] of any [`recommendation`] via its index
    ///
    /// Note: this method is pointless to call if the given index is [`USER_INPUT`], as user input
    /// is not a recommendation, hence always returning a reference to an _invalid_ `RecData`
    ///
    /// [`recommendation`]: Completion
    pub(super) fn rec_data_from_index(&self, recommendation_i: i8) -> &RecData {
        if self.indexer.multiple && self.indexer.in_list_2.contains(&recommendation_i) {
            return self.rec_list[self.indexer.list.1];
        }
        self.rec_list[self.indexer.list.0]
    }
    #[inline]
    fn last_key(&self) -> Option<&SliceData> {
        self.curr_value()
            .or(self.curr_arg())
            .or(self.curr_command())
    }
    #[inline]
    fn trailing<'a>(&self, line_trim_start: &'a str) -> &'a str {
        self.last_key().map_or(line_trim_start, |key| {
            let trim = line_trim_start[key.byte_start + key.slice_len..].trim();
            trim.strip_prefix(HELP_ARG)
                .or(trim.strip_prefix(HELP_ARG_SHORT))
                .unwrap_or(trim)
        })
    }
    #[inline]
    fn arg_or_cmd(&self) -> Option<&SliceData> {
        self.curr_arg().or(self.curr_command())
    }
    #[inline]
    fn curr_command(&self) -> Option<&SliceData> {
        self.input.curr_command.as_ref()
    }
    #[inline]
    fn curr_arg(&self) -> Option<&SliceData> {
        self.input.curr_argument.as_ref()
    }
    #[inline]
    fn curr_value(&self) -> Option<&SliceData> {
        self.input.curr_value.as_ref()
    }

    /// expects `RecKind::Value`
    #[inline]
    fn value_valid(&self, value: &str, i: usize) -> bool {
        self.value_sets.get(&i).expect("kind value").contains(value)
    }

    #[inline]
    fn valid_rec_prefix(&self, str: &str, has_help: bool) -> bool {
        (has_help && HELP_STR.starts_with(str.trim_start_matches('-')))
            || self
                .recommendations
                .first()
                .is_some_and(|next| next.starts_with(str))
    }

    fn try_parse_token_from_end(
        &self,
        line: &str,
        expected: &RecKind,
        arg_count: Option<usize>,
    ) -> Option<SliceData> {
        if self.input.ending.open_quote.is_none() {
            let line_trim_end = line.trim_end();
            let start = if line_trim_end.ends_with(['\'', '\"']) {
                let quote = line_trim_end.chars().next_back().expect("outer if");
                line_trim_end[..line_trim_end.len() - quote.len_utf8()].rfind(quote)
            } else {
                Some(
                    line_trim_end
                        .char_indices()
                        .rev()
                        .find_map(|(i, ch)| ch.is_whitespace().then(|| i + ch.len_utf8()))
                        .unwrap_or_default(),
                )
            };
            if let Some(byte_start) = start {
                let len = line_trim_end.len() - byte_start;
                return (len > 0).then(|| {
                    SliceData::from_raw_unchecked(byte_start, len, expected, line, self, arg_count)
                });
            }
        }
        None
    }

    /// only counts values until `count_till` is found, if `count_till` is not found it will return the last registered token in the
    /// input `slice`. `SliceData` is _only_ ever `None` if their are 0 tokens in the input `slice`, `Some(SliceData)` does not guarantee  
    /// the containing `SliceData` is of `RecKind`: `count_till` or has a valid `hash_i` - in the case that the first token is returned  
    ///
    /// NOTES:  
    ///  - unexpected behavior is _guaranteed_ for returned `SliceData` if the input `slice` has been sliced from the beginning,  
    ///    the start of `slice`, must align with the start of `line_trim_start`  
    ///  - if you only desire the count of vals, trim input `slice` to include slice of all vals to be counted plus the beginning 'root' token  
    ///    then use `RecKind::Null` to avoid hashing counted tokens  
    fn count_vals_in_slice(&self, slice: &str, count_till: &RecKind) -> (Option<SliceData>, usize) {
        let mut nvals = 0;
        let mut prev_token = None;
        let mut end_i = slice.len();
        let last_valid_token = self.last_key();

        while let Some(token) = self.try_parse_token_from_end(&slice[..end_i], count_till, None) {
            if token.hash_i != INVALID {
                return (Some(token), nvals);
            } else if last_valid_token.is_some_and(|&known_valid| token == known_valid) {
                // here we copy the last valid_token in the case that `last_valid_token`'s `RecKind` != the `count_till` `RecKind`
                // and the incorrect hasher was used on the curr `token`
                return (last_valid_token.copied(), nvals);
            } else {
                nvals += 1;
                end_i = token.byte_start;
            }
            prev_token = Some(token)
        }
        (prev_token, nvals.saturating_sub(1))
    }

    /// Caller must ensure that the given line is `LineData.input.trim_start()` as internally
    /// [SliceData::to_slice_unchecked] is called
    fn hash_command_unchecked(&self, line: &str, command: &mut SliceData) {
        let command_str = command.to_slice_unchecked(line);
        if command_str.starts_with('-') {
            return;
        }
        let mut option = None;
        let hash_str = command_str
            .chars()
            .next()
            .filter(|c| c.is_uppercase())
            .map(|c| {
                option = Some(format!(
                    "{}{}",
                    c.to_ascii_lowercase(),
                    &command_str[c.len_utf8()..]
                ));
                option.as_deref().unwrap()
            })
            .unwrap_or(command_str);

        if let Some(&i) = self.rec_map.get(hash_str) {
            if let Some(Parent::Root | Parent::Universal) = self.rec_list[i].parent {
                command.hash_i = i;
            }
        }
    }

    /// Caller must ensure that the given line is `LineData.input.trim_start()` as internally
    /// [SliceData::to_slice_unchecked] is called
    fn hash_arg_unchecked(&self, line_trim_start: &str, arg: &mut SliceData) {
        let arg_str = arg.to_slice_unchecked(line_trim_start);
        if !arg_str.starts_with('-') {
            return;
        }
        if let Some(i) =
            self.trimmed_arg_valid_i_unchecked(arg_str.trim_start_matches('-'), line_trim_start)
        {
            arg.hash_i = i;
        }
    }

    fn trimmed_arg_valid_i_unchecked(&self, arg: &str, line_trim_start: &str) -> Option<usize> {
        let cmd = self
            .curr_command()
            .expect("can only set arg if command is valid")
            .to_slice_unchecked(line_trim_start);
        let i = self.rec_map.get(arg).copied()?;
        if match self.rec_list[i].parent {
            // `hash_command_unchecked` _only_ provides case leeway for 'Pascal Case' commands
            // making it fine to ignore all case here
            Some(Parent::Entry(p)) => p.eq_ignore_ascii_case(cmd),
            Some(Parent::Universal) => true,
            _ => false,
        } {
            return Some(i);
        }
        None
    }

    /// Caller must ensure that the given line is `LineData.input.trim_start()` as internally
    /// [SliceData::to_slice_unchecked] is called
    fn hash_value_unchecked(
        &self,
        line: &str,
        value: &mut SliceData,
        range: &Range<usize>,
        count: usize,
    ) {
        if !range.contains(&count) {
            return;
        }

        let val_str = value.to_slice_unchecked(line);
        if val_str.starts_with('-') {
            return;
        }

        let parent = self
            .arg_or_cmd()
            .expect("can only set value if cmd or arg is some");

        if self.value_valid(val_str, parent.hash_i) {
            value.hash_i = VALID;
        }
    }

    /// Returns `Some(true)` or `Some(false)` if the given `kind` should be formatted as an argument or  
    /// `None` if there is no applicable formatting. eg. `RecKind::UserDefined` or `RecKind::Null`
    pub(super) fn arg_format(&self, recommendation: &str, kind: &RecKind) -> Option<bool> {
        if recommendation == HELP_STR {
            return Some(self.curr_command().is_some());
        }

        match kind {
            RecKind::Argument(_) => Some(true),
            RecKind::Value(_) | RecKind::Command => Some(false),
            RecKind::Help => Some(self.curr_command().is_some()),
            RecKind::UserDefined { .. } | RecKind::ArgFlag | RecKind::Null => None,
        }
    }

    /// Will panic if `self.completion.is_empty()`
    fn set_default_recommendations_unchecked(&mut self) {
        let commands = self.rec_list[COMMANDS];
        self.recommendations = commands.recs.as_ref().expect("commands is not empty")
            [..commands.unique_rec_end()]
            .to_vec();
        self.recommendations.push(HELP_STR);
    }
}

fn strip_dashes(str: &str) -> (usize, Option<&str>) {
    let dashes = str.chars().take_while(|&c| c == '-').count();
    (dashes, str.get(dashes..).filter(|r| !r.is_empty()))
}

impl<Ctx, W: Write> Repl<Ctx, W> {
    #[inline]
    fn curr_token(&self) -> &str {
        &self.completion.input.ending.token
    }
    #[inline]
    fn open_quote(&self) -> Option<&(usize, char)> {
        self.completion.input.ending.open_quote.as_ref()
    }

    fn kind_err_conditions(&self, hash: usize, curr_token: &str, trailing: &str) -> bool {
        let rec = self.completion.rec_list[hash];
        let has_help = if hash == VALID {
            self.completion
                .curr_command()
                .map(|cmd| self.completion.rec_list[cmd.hash_i].has_help)
                .expect("valid can only be set once a base entry is provided")
        } else {
            rec.has_help
        };
        match rec.kind {
            RecKind::Argument(required) => {
                match required.cmp(
                    &(self.completion.input.required_input_i.len()
                        + !curr_token.is_empty() as usize),
                ) {
                    Ordering::Greater => true,
                    Ordering::Equal => false,
                    Ordering::Less => {
                        match strip_dashes(curr_token) {
                            (0, Some(_)) => true,
                            (0..3, None) => false,
                            (1, Some(HELP_SHORT)) => {
                                let parent_hash = self
                                    .completion
                                    .curr_command()
                                    .expect("can only set arg if command is valid")
                                    .hash_i;
                                !self.completion.rec_list[parent_hash].has_help
                            }
                            (1, Some(input)) if input.chars().nth(1).is_none() => self
                                .completion
                                // input line is calculated the same way it is within `update_completion`
                                .trimmed_arg_valid_i_unchecked(input, self.line.input.trim_start())
                                .is_none(),
                            (2, Some(input)) => !self.completion.valid_rec_prefix(input, has_help),
                            _ => true,
                        }
                    }
                }
            }
            _ if curr_token.starts_with('-') => match strip_dashes(curr_token) {
                (0..3, None) | (1, Some(HELP_SHORT)) | (2, Some(HELP_STR)) => !has_help,
                (2, Some(input)) => !self.completion.valid_rec_prefix(input, has_help),
                _ => true,
            },
            RecKind::Command => !self.completion.valid_rec_prefix(curr_token, has_help),
            // other `Value` and `UserDefined` errors do not need to be checked since `update_completion`
            // will set `curr_value` to an invalid instance returning the error condition prior this fn call
            RecKind::UserDefined { parse_fn, .. } => {
                trailing.is_empty() || parse_fn.is_some_and(|valid| !valid(curr_token))
            }
            RecKind::Value(_) => {
                trailing.is_empty() || !self.completion.valid_rec_prefix(curr_token, has_help)
            }
            RecKind::ArgFlag | RecKind::Help | RecKind::Null => !trailing.is_empty(),
        }
    }

    fn check_value_err(&self, line_trim_start: &str) -> bool {
        let rec_list = [
            self.completion.indexer.list.0,
            self.completion.indexer.list.1,
        ];
        let curr_token = self.curr_token();
        let trailing = self.completion.trailing(line_trim_start);
        let mut errs = [false, false];
        for (err, hash) in errs.iter_mut().zip(rec_list) {
            *err = self.kind_err_conditions(hash, curr_token, trailing);
            if !self.completion.indexer.multiple {
                return *err;
            }
        }
        if curr_token.starts_with('-')
            && matches!(
                self.completion.rec_list[rec_list[1]].kind,
                RecKind::Argument(_)
            )
        {
            return errs[1];
        }
        errs[0] && errs[1]
    }

    #[inline]
    fn add_help(&self, recs: [(&RecData, usize); 2], line_trim_start: &str) -> bool {
        let last_rec = recs[0].0;

        last_rec.has_help
            && (!matches!(
                last_rec.kind,
                RecKind::Value(_) | RecKind::UserDefined { .. }
            ) || {
                let trailing = self.completion.trailing(line_trim_start);
                trailing.is_empty()
                    || !self.kind_err_conditions(recs[0].1, self.curr_token(), trailing)
            })
            || recs[1].0.has_help
                && (self.completion.indexer.multiple
                    || self
                        .completion
                        .input
                        .curr_value
                        .is_some_and(|v| v.hash_i == VALID))
    }

    fn check_for_errors(&self, line_trim_start: &str) -> bool {
        self.completion.curr_command().is_some_and_invalid()
            || self.completion.curr_arg().is_some_and_invalid()
            || self.completion.curr_value().is_some_and_invalid()
            || self.check_value_err(line_trim_start)
    }

    fn try_get_forward_arg_or_val(
        &self,
        line_trim_start: &str,
        command_kind: &RecKind,
    ) -> Option<SliceData> {
        let (kind_match, nvals) = self
            .completion
            .count_vals_in_slice(line_trim_start, command_kind);

        if let Some(ref starting_token) = kind_match {
            let start_token_meta = self.completion.rec_list[starting_token.hash_i];

            if starting_token.hash_i == INVALID || nvals == 0 {
                return kind_match;
            }

            if let RecData {
                kind: RecKind::Value(range) | RecKind::UserDefined { range, .. },
                end: false,
                ..
            } = start_token_meta
            {
                if range.contains(&nvals) {
                    return None;
                }
            }
        }

        self.completion
            .try_parse_token_from_end(line_trim_start, command_kind, Some(nvals))
    }

    /// Updates the suggestions for the current user input
    pub fn update_completion(&mut self) {
        if !self.line.comp_enabled {
            return;
        }

        let line_trim_start = self.line.input.trim_start();
        if line_trim_start.is_empty() {
            // `comp_enabled` can only be set when `!Completion.is_empty()` via checks in`enable_completion`
            // and `ReplBuilder::build`. Making it safe to call `default_recommendations` here
            self.completion.set_default_recommendations_unchecked();
            self.line.err = false;
            self.completion.input.ending = LineEnd::default();
            return;
        }

        self.completion.input.update_curr_token(line_trim_start);
        let state_changed = self.completion.input.check_state(line_trim_start);

        if !state_changed && self.completion.indexer.list.0 == INVALID {
            self.line.err = self.check_for_errors(line_trim_start);
            return;
        }

        let multiple_switch_kind = self.completion.indexer.multiple
            && line_trim_start.ends_with(char::is_whitespace)
            && line_trim_start
                .split_whitespace()
                .next_back()
                .is_some_and(|end_token| end_token.starts_with('-'));

        if multiple_switch_kind {
            self.completion.indexer.multiple = false;
        }

        self.completion.indexer.recs = USER_INPUT;

        if self.completion.curr_command().is_none() && self.open_quote().is_none() {
            self.completion.input.curr_command = line_trim_start
                .split_once(char::is_whitespace)
                .map(|(pre, _)| {
                    SliceData::from_raw_unchecked(
                        0,
                        pre.len(),
                        &RecKind::Command,
                        line_trim_start,
                        &self.completion,
                        None,
                    )
                });
        }

        if let Some((cmd_kind, cmd_suf)) = self.completion.curr_command().and_then(|cmd| {
            if self.open_quote().is_some()
                || self.completion.curr_value().is_some()
                || (self.completion.curr_arg().is_some() && !multiple_switch_kind)
            {
                return None;
            }

            let command_kind = &self.completion.rec_list[cmd.hash_i].kind;
            let cmd_suf = line_trim_start[cmd.slice_len..].trim_start();
            (matches!(command_kind, RecKind::Argument(_) | RecKind::Value(_))
                && !cmd_suf.is_empty())
            .then_some((command_kind, cmd_suf))
        }) {
            let mut new = if cmd_suf.ends_with(char::is_whitespace) {
                self.try_get_forward_arg_or_val(line_trim_start, cmd_kind)
            } else {
                // make sure we set prev arg when backspacing
                let (kind_match, nvals) = self.completion.count_vals_in_slice(
                    &line_trim_start[..line_trim_start.len() - self.curr_token().len()],
                    cmd_kind,
                );

                kind_match.filter(|starting_token| {
                    !starting_token.exact_eq(self.completion.curr_command().expect("outer if"))
                        && match &self.completion.rec_list[starting_token.hash_i].kind {
                            RecKind::Value(range) | RecKind::UserDefined { range, .. }
                                if range.contains(&(nvals + 1)) =>
                            {
                                self.completion.indexer.multiple = nvals >= range.start;
                                true
                            }
                            _ => self.completion.rec_list[starting_token.hash_i].end,
                        }
                })
            };
            let kind = new
                .as_mut()
                .and_then(|token| {
                    // can call into `to_slice_unchecked` since the above slice input to `try_parse_token_from_end` and
                    // `count_vals_in_slice` both use `line_trim_start` and the beginning of `line_trim_start` was not sliced
                    let token_slice = token.to_slice_unchecked(line_trim_start);
                    (token_slice == HELP_ARG || token_slice == HELP_ARG_SHORT).then(|| {
                        token.hash_i = HELP;
                        &RecKind::Argument(0)
                    })
                })
                .unwrap_or(cmd_kind);

            match kind {
                &RecKind::Argument(required) => {
                    // track the position of user defined user required inputs for the current command
                    match new {
                        Some(SliceData {
                            hash_i: INVALID,
                            byte_start,
                            slice_len,
                        }) if self.completion.input.required_input_i.len() < required => self
                            .completion
                            .input
                            .required_input_i
                            .push(byte_start + slice_len),
                        _ => self.completion.input.curr_argument = new,
                    }
                }
                RecKind::Value(_) => self.completion.input.curr_value = new,
                _ => unreachable!("by outer if"),
            }
        }

        if let Some(end) = self.completion.curr_arg().and_then(|arg| {
            let rec_data = self.completion.rec_list[arg.hash_i];
            (rec_data.kind == RecKind::ArgFlag).then_some(rec_data.end)
        }) {
            // boolean flag found, ok to move on
            if !end {
                self.completion.input.curr_argument = None;
            }
        } else if let Some((cmd_kind, arg_slice, range, arg_suf)) =
            self.completion.curr_command().and_then(|cmd| {
                if cmd.hash_i == INVALID
                    || self.completion.curr_value().is_some()
                    || self.open_quote().is_some()
                {
                    return None;
                }

                self.completion.curr_arg().and_then(|arg| {
                    let (RecKind::Value(range) | RecKind::UserDefined { range, .. }) =
                        &self.completion.rec_list[arg.hash_i].kind
                    else {
                        return None;
                    };
                    let cmd_kind = &self.completion.rec_list[cmd.hash_i].kind;
                    let arg_suf = line_trim_start[arg.byte_start + arg.slice_len..].trim_start();
                    (arg.hash_i != INVALID && !arg_suf.is_empty())
                        .then_some((cmd_kind, arg, range, arg_suf))
                })
            })
        {
            if arg_suf.ends_with(char::is_whitespace) {
                let arg_data = self.completion.rec_list[arg_slice.hash_i];

                if let Some(token) =
                    self.completion
                        .try_parse_token_from_end(line_trim_start, &arg_data.kind, None)
                {
                    if token.hash_i != INVALID && !arg_data.end {
                        let (kind_match, nvals) = self
                            .completion
                            .count_vals_in_slice(line_trim_start, cmd_kind);
                        debug_assert!(kind_match.unwrap().exact_eq(arg_slice));

                        if range.contains(&(nvals + 1)) {
                            self.completion.indexer.multiple = true;
                        } else {
                            self.completion.indexer.multiple = false;
                            self.completion.input.curr_argument = None;
                        }
                    } else {
                        self.completion.input.curr_value = Some(token);
                    }
                }
            } else {
                // make sure we set multiple to false when backspacing
                let (kind_match, nvals) = self.completion.count_vals_in_slice(
                    &line_trim_start[..line_trim_start.len() - self.curr_token().len()],
                    cmd_kind,
                );
                debug_assert!(kind_match.unwrap().exact_eq(arg_slice));
                self.completion.indexer.multiple = range.contains(&nvals);
            }
        }

        // writeln!(
        //     get_debugger(),
        //     "{}",
        //     self.completion.input.debug(line_trim_start)
        // )
        // .unwrap();

        self.completion.indexer.list = match (
            self.completion.curr_command(),
            self.completion.curr_arg(),
            self.completion.curr_value(),
        ) {
            (_, Some(&SliceData { hash_i: j, .. }), Some(&SliceData { hash_i: k, .. })) => (k, j),
            (Some(&SliceData { hash_i: i, .. }), None, Some(&SliceData { hash_i: k, .. })) => {
                (k, i)
            }
            (Some(&SliceData { hash_i: i, .. }), Some(&SliceData { hash_i: j, .. }), None) => {
                (j, i)
            }
            (Some(&SliceData { hash_i: i, .. }), None, None) => (i, INVALID),
            (None, None, None) if line_trim_start.split_whitespace().count() <= 1 => {
                (COMMANDS, INVALID)
            }
            _ => (INVALID, INVALID),
        };

        if self.completion.indexer.list.1 == INVALID {
            self.completion.indexer.multiple = false;
        }

        let rec_data = [
            self.completion.indexer.list.0,
            self.completion.indexer.list.1,
        ]
        .map(|hash| (self.completion.rec_list[hash], hash));
        let (rec_data_1, rec_data_2) = (rec_data[0].0, rec_data[1].0);

        let add_help = self.add_help(rec_data, line_trim_start);

        if self.curr_token().is_empty() {
            if let Some(recs) = rec_data_1.recs {
                self.completion.recommendations = recs[..rec_data_1.unique_rec_end()].to_vec();
            } else {
                self.completion.recommendations.clear();
            }

            if self.completion.indexer.multiple {
                if let Some(recs2) = rec_data_2.recs {
                    let rec_len = self.completion.recommendations.len() as i8;
                    let recs2 = &recs2[..rec_data_2.unique_rec_end()];
                    let rec_2_end = if add_help {
                        rec_len + recs2.len() as i8 + 1
                    } else {
                        rec_len + recs2.len() as i8
                    };
                    self.completion.indexer.in_list_2 = (rec_len..rec_2_end).collect();
                    self.completion.recommendations.extend(recs2);
                }
            }
            if add_help {
                self.completion.recommendations.push(HELP_STR);
            }
            self.line.err = self.check_for_errors(line_trim_start);
            return;
        }

        let input_lower = self.curr_token().trim_start_matches('-').to_lowercase();

        let rec_1 = (!self.curr_token().starts_with('-')
            || matches!(rec_data_1.kind, RecKind::Argument(_)))
        .then(|| rec_data_1.recs.map(|recs| recs.iter()))
        .flatten();

        let rec_2 = (self.completion.indexer.multiple
            && (!self.curr_token().starts_with('-')
                || matches!(rec_data_2.kind, RecKind::Argument(_))))
        .then(|| rec_data_2.recs.map(|recs| recs.iter()))
        .flatten();

        let add_help = add_help.then_some([HELP_STR].iter());

        let mut recommendations = std::iter::empty()
            .chain(rec_1.unwrap_or_default())
            .chain(rec_2.unwrap_or_default())
            .chain(add_help.unwrap_or_default())
            .filter(|rec| rec.contains(&input_lower))
            .copied()
            .collect::<Vec<_>>();

        recommendations.sort_unstable_by(|a, b| {
            let a_starts = a.starts_with(&input_lower);
            let b_starts = b.starts_with(&input_lower);
            b_starts.cmp(&a_starts)
        });

        if self.completion.indexer.multiple {
            if let Some(recs2) = rec_data_2.recs {
                self.completion.indexer.in_list_2 = recommendations
                    .iter()
                    .enumerate()
                    .filter(|&(_, rec)| recs2.contains(rec) || *rec == HELP_STR)
                    .map(|(i, _)| i as i8)
                    .collect();
            }
        }

        self.completion.recommendations = recommendations;
        self.line.err = self.check_for_errors(line_trim_start);
    }

    /// Changes the current user input to either `Next` or `Previous` suggestion depending on the given direction
    pub fn try_completion(&mut self, direction: Direction) -> io::Result<()> {
        if !self.line.comp_enabled
            || self.completion.recommendations.is_empty()
            || self.completion.last_key().is_some_and_invalid()
            || (self.completion.recommendations.len() == 1
                && match self.completion.rec_data_from_index(0).kind {
                    RecKind::Value(_) => {
                        self.curr_token() == self.completion.recommendations[0]
                            && self.curr_token() != HELP_STR
                    }
                    RecKind::Argument(_) => self
                        .curr_token()
                        .strip_prefix("--")
                        .is_some_and(|user_input| user_input == self.completion.recommendations[0]),
                    _ => self.curr_token() == self.completion.recommendations[0],
                })
        {
            self.set_uneventful();
            return Ok(());
        }

        let recommendation = loop {
            self.completion.indexer.recs += direction.to_int();

            match self.completion.indexer.recs {
                i if i >= USER_INPUT && i < self.completion.recommendations.len() as i8 => (),
                i if i < USER_INPUT => {
                    self.completion.indexer.recs = self.completion.recommendations.len() as i8 - 1
                }
                _ => self.completion.indexer.recs = USER_INPUT,
            }

            if self.completion.indexer.recs == USER_INPUT {
                break self.curr_token();
            } else {
                let next = self.completion.recommendations[self.completion.indexer.recs as usize];
                if match self
                    .completion
                    .rec_data_from_index(self.completion.indexer.recs)
                    .kind
                {
                    RecKind::Value(_) => self.curr_token() != next || self.curr_token() == HELP_STR,
                    RecKind::Argument(_) => !self
                        .curr_token()
                        .strip_prefix("--")
                        .is_some_and(|user_input| user_input == next),
                    _ => self.curr_token() != next,
                } {
                    break next;
                }
            };
        };

        let format_line = |rec_is_arg| {
            self.line
                .input
                .rsplit_once(char::is_whitespace)
                .map_or_else(
                    || recommendation.to_string(),
                    |(pre, _)| {
                        format!(
                            "{pre} {}{recommendation}",
                            if rec_is_arg
                                && !recommendation.is_empty()
                                && self.completion.indexer.recs != USER_INPUT
                            {
                                "--"
                            } else {
                                ""
                            }
                        )
                    },
                )
        };

        let kind = if recommendation == HELP_STR {
            &self.completion.rec_list[HELP].kind
        } else if self.completion.indexer.recs == USER_INPUT {
            // Set as `Command` because we do not need additional formatting below in the `USER_INPUT` case
            &RecKind::Command
        } else {
            &self
                .completion
                .rec_data_from_index(self.completion.indexer.recs)
                .kind
        };

        let new_line = format_line(
            self.completion
                .arg_format(recommendation, kind)
                .expect("guard clause covers `UserInput` and `Null`"),
        );

        self.line.err = if self.completion.indexer.recs == USER_INPUT {
            self.check_value_err(&new_line)
        } else {
            false
        };

        self.change_line_raw(new_line)?;
        Ok(())
    }

    /// Clears all state found by the completion module
    pub(super) fn reset_completion(&mut self) {
        self.line.err = false;
        if self.completion.is_empty() {
            self.completion.input.ending = LineEnd::default();
            return;
        }
        self.completion.set_default_recommendations_unchecked();
        self.completion.input = CompletionState::default();
        self.completion.indexer = Indexer::default();
    }
}
