use crate::{
    ansi_code::{BLUE, GREY, RESET, YELLOW},
    line::LineData,
};
use crossterm::style::{Color, Stylize};
use std::fmt::Display;

const QUOTES: [char; 2] = ['\'', '\"'];
const QUOTE_LEN: usize = QUOTES[0].len_utf8();

#[derive(Default, PartialEq, Eq)]
enum TextColor {
    #[default]
    Yellow,
    Blue,
    Grey,
    White,
}

impl TextColor {
    fn to_str(&self) -> &'static str {
        match self {
            TextColor::Yellow => YELLOW,
            TextColor::Blue => BLUE,
            TextColor::Grey => GREY,
            TextColor::White => RESET,
        }
    }
}

#[derive(Default)]
struct FormatState {
    curr_color: TextColor,
    open_quote: Option<(char, usize, usize)>,
    white_space_start: usize,
    output: String,
}

impl FormatState {
    fn new() -> Self {
        FormatState {
            output: String::from(TextColor::default().to_str()),
            ..Default::default()
        }
    }

    #[inline]
    fn set_color(&mut self, color: TextColor) {
        self.output.push_str(color.to_str());
        self.curr_color = color;
    }

    #[inline]
    fn push(&mut self, str: &str) {
        self.output.push_str(str);
    }

    #[inline]
    fn open_quote(&mut self, quote: char, total_len: usize) {
        self.open_quote = Some((quote, self.white_space_start, total_len));
        self.white_space_start += total_len;
    }

    #[inline]
    fn add_quote_len(&mut self, len: usize) {
        debug_assert!(self.open_quote.is_some());
        self.open_quote = self
            .open_quote
            .map(|(ch, start, prev)| (ch, start, prev + len));
    }

    #[inline]
    fn close_quote(&mut self, input: &str, start: usize, token_len: usize) {
        self.push(&input[start..start + token_len]);
        self.open_quote = None;
        self.set_color(TextColor::White);
    }
}

impl Display for LineData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{RESET}")?;
        if !self.style_enabled {
            return write!(f, "{}{} {}", self.prompt, self.prompt_separator, self.input);
        }
        let (stylized_input, mismatched_quotes) = stylize_input(&self.input);
        write!(
            f,
            "{}{} {stylized_input}",
            self.prompt.as_str().bold(),
            self.prompt_separator.as_str().bold().stylize().with(
                if self.err || mismatched_quotes {
                    Color::Red
                } else {
                    Color::White
                }
            )
        )
    }
}

fn stylize_input(input: &str) -> (String, bool) {
    let mut ctx = FormatState::new();

    for mut token in input.split_whitespace() {
        let white_space_len = input[ctx.white_space_start..]
            .find(token)
            .expect("already found");

        let push_ws = |ctx: &mut FormatState| {
            ctx.push(&input[ctx.white_space_start..ctx.white_space_start + white_space_len]);
            ctx.white_space_start += white_space_len;
        };

        if let Some((quote, start, len)) = ctx.open_quote {
            if token.ends_with(quote) {
                ctx.close_quote(input, start, white_space_len + len + token.len());
            } else {
                ctx.add_quote_len(white_space_len + token.len());
            }
            ctx.white_space_start += white_space_len + token.len();
            continue;
        }

        if let Some(slice_data) = parse_quoted_token(token) {
            if ctx.curr_color == TextColor::White {
                if slice_data.starts_with_quote {
                    ctx.set_color(TextColor::Blue);
                } else if slice_data.contains_quote.starts_with('-') {
                    ctx.set_color(TextColor::Grey);
                }
            }

            if let Some(quote) = slice_data.open_quote {
                let token_len = slice_data.remainder.map_or(token.len(), |rem| {
                    push_ws(&mut ctx);
                    ctx.push(slice_data.contains_quote);
                    ctx.white_space_start += slice_data.contains_quote.len();
                    ctx.set_color(TextColor::White);
                    rem.len()
                });
                ctx.open_quote(quote, token_len + white_space_len);
                continue;
            }

            push_ws(&mut ctx);
            ctx.push(slice_data.contains_quote);
            ctx.white_space_start += slice_data.contains_quote.len();

            if ctx.curr_color != TextColor::White {
                ctx.set_color(TextColor::White);
            }

            token = slice_data.remainder.unwrap_or_default();
        } else {
            push_ws(&mut ctx);

            if ctx.curr_color == TextColor::White && token.starts_with('-') {
                ctx.set_color(TextColor::Grey);
            }
        }

        ctx.push(token);
        ctx.white_space_start += token.len();

        if ctx.curr_color != TextColor::White {
            ctx.set_color(TextColor::White);
        }
    }

    if let Some((_, start, _)) = ctx.open_quote {
        ctx.push(&input[start..]);
    }

    if ctx.curr_color != TextColor::White {
        ctx.set_color(TextColor::White);
    }

    (ctx.output, ctx.open_quote.is_some())
}

struct QuoteSlice<'a> {
    /// Contains entire slice or through the last consecutive closed quote
    contains_quote: &'a str,
    /// Char indice of the last found open quote
    open_quote: Option<char>,
    /// Remainder of slice after the last closed quote if their is no hanging open quote
    remainder: Option<&'a str>,
    /// If the token starts with any quote
    starts_with_quote: bool,
}

/// Only returns `Some` if a quote is found
fn parse_quoted_token(token: &str) -> Option<QuoteSlice> {
    let starts_with_quote = token.starts_with(QUOTES);
    let mut quote_found = starts_with_quote;
    let mut consecutive = starts_with_quote;
    let mut open =
        starts_with_quote.then(|| token.char_indices().next().expect("starts with quotes"));
    let mut consecutive_closed = None;

    for (i, ch) in token.char_indices().skip(1) {
        match open {
            Some((_, quote)) => {
                if ch == quote {
                    open = None;
                    if consecutive {
                        consecutive_closed = Some((i, ch));
                    }
                }
            }
            None => match consecutive_closed {
                Some((j, _)) => {
                    consecutive = if QUOTES.iter().any(|&quote| ch == quote) {
                        open = Some((i, ch));
                        consecutive && i == j + QUOTE_LEN
                    } else {
                        false
                    }
                }
                None => {
                    if QUOTES.iter().any(|&quote| ch == quote) {
                        open = Some((i, ch));
                        quote_found = true;
                    }
                }
            },
        }
    }

    quote_found.then(|| {
        if !starts_with_quote || (consecutive && open.is_some()) {
            return QuoteSlice {
                contains_quote: token,
                open_quote: open.map(|(_, quote)| quote),
                starts_with_quote,
                remainder: None,
            };
        }
        match consecutive_closed {
            Some(closed_quote) => QuoteSlice {
                contains_quote: &token[..=closed_quote.0],
                open_quote: open.map(|(_, quote)| quote),
                starts_with_quote,
                remainder: token[closed_quote.0..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| &token[closed_quote.0 + i..]),
            },
            None => {
                debug_assert!(open.is_some());
                QuoteSlice {
                    contains_quote: token,
                    open_quote: open.map(|(_, quote)| quote),
                    starts_with_quote,
                    remainder: None,
                }
            }
        }
    })
}
