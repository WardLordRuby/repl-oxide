use crate::{completion::Direction, line::LineReader};
use std::{
    collections::{BTreeMap, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Write},
};

#[derive(Default)]
pub(crate) struct History {
    prev_entries: BTreeMap<usize, String>,
    value_order_map: HashMap<u64, usize>,
    top: usize,
    temp_top: String,
    curr_pos: usize,
}

impl History {
    /// Caller must gaurentee there are available entries in the given direction
    fn get_unchecked(&mut self, direction: Direction) -> &str {
        let (&pos, entry) = match direction {
            Direction::Previous => self.prev_entries.range(..self.curr_pos).next_back(),
            Direction::Next => self.prev_entries.range(self.curr_pos + 1..).next(),
        }
        .expect("no entries in the given direction");
        self.curr_pos = pos;
        entry
    }

    /// Items yield from most recent to oldest
    #[inline]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&usize, &String)> {
        self.prev_entries.iter().rev()
    }

    /// Returns the most recent entry
    #[inline]
    pub(crate) fn last(&self) -> Option<&str> {
        self.prev_entries.values().next_back().map(String::as_str)
    }

    /// Returns the entry at a given position  
    #[inline]
    pub(crate) fn get(&self, position: &usize) -> Option<&String> {
        self.prev_entries.get(position)
    }
}

#[inline]
fn hash_str(str: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    str.hash(&mut hasher);
    hasher.finish()
}

impl<Ctx, W: Write> LineReader<Ctx, W> {
    /// Pushes onto history and resets the internal history index to the top
    pub fn add_to_history(&mut self, mut add: &str) {
        add = add.trim();

        if self.history.last().is_some_and(|entry| entry == add) {
            self.reset_history_idx();
            return;
        }

        let new_last_p = self.history.top;
        self.history.top += 1;

        self.history
            .value_order_map
            .entry(hash_str(add))
            .and_modify(|prev_p| {
                let old = self
                    .history
                    .prev_entries
                    .remove(prev_p)
                    .expect("value must have been inserted on previous function call");
                self.history.prev_entries.insert(new_last_p, old);
                *prev_p = new_last_p;
            })
            .or_insert_with(|| {
                self.history
                    .prev_entries
                    .insert(new_last_p, add.to_string());
                new_last_p
            });

        self.reset_history_idx();
    }

    /// Changes the current line to the previous history entry if available
    pub fn history_back(&mut self) -> io::Result<()> {
        if self.history.prev_entries.is_empty()
            || self.history.curr_pos == *self.history.prev_entries.keys().next().unwrap()
        {
            self.set_uneventful();
            return Ok(());
        }
        if self.history.curr_pos == self.history.top {
            self.history.temp_top = std::mem::take(&mut self.line.input);
        }
        let entry = self.history.get_unchecked(Direction::Previous).to_string();
        self.change_line(entry)
    }

    /// Changes the current line to the next history entry if available
    pub fn history_forward(&mut self) -> io::Result<()> {
        if self.history.curr_pos == self.history.top {
            self.set_uneventful();
            return Ok(());
        }
        let entry = if self.history.curr_pos
            == *self
                .history
                .prev_entries
                .keys()
                .next_back()
                .expect("missed early return so `history_back` must have been called before")
        {
            self.history.curr_pos = self.history.top;
            std::mem::take(&mut self.history.temp_top)
        } else {
            self.history.get_unchecked(Direction::Next).to_string()
        };
        self.change_line(entry)
    }

    #[inline]
    pub(crate) fn reset_history_idx(&mut self) {
        self.history.curr_pos = self.history.top;
    }
}
