use crate::line::{completion::Direction, LineReader};

use std::{
    collections::{BTreeMap, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
    io::{self, Write},
};

#[derive(Default)]
pub(super) struct History {
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
    pub(super) fn iter(&self) -> impl Iterator<Item = (&usize, &String)> {
        self.prev_entries.iter().rev()
    }

    /// Returns the most recent entry
    #[inline]
    pub(super) fn last(&self) -> Option<&str> {
        self.prev_entries.values().next_back().map(String::as_str)
    }

    /// Returns the entry at a given position  
    #[inline]
    pub(super) fn get(&self, position: &usize) -> Option<&String> {
        self.prev_entries.get(position)
    }

    #[inline]
    pub(super) fn reset_idx(&mut self) {
        self.curr_pos = self.top;
    }

    pub(super) fn push(&mut self, mut add: &str) {
        add = add.trim();

        if self.last().is_some_and(|entry| entry == add) {
            self.reset_idx();
            return;
        }

        let new_last_p = self.top;
        self.top += 1;

        self.value_order_map
            .entry(hash_str(add))
            .and_modify(|prev_p| {
                let old = self
                    .prev_entries
                    .remove(prev_p)
                    .expect("value must have been inserted on previous function call");
                self.prev_entries.insert(new_last_p, old);
                *prev_p = new_last_p;
            })
            .or_insert_with(|| {
                self.prev_entries.insert(new_last_p, add.to_string());
                new_last_p
            });

        self.reset_idx();
    }
}

impl<A: AsRef<str>> FromIterator<A> for History {
    fn from_iter<T: IntoIterator<Item = A>>(iter: T) -> Self {
        let mut history = History::default();

        for entry in iter {
            history.push(entry.as_ref());
        }

        history
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
    #[inline]
    pub fn add_to_history(&mut self, add: &str) {
        self.history.push(add);
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

    /// Returns history exported via clone as a new `Vec` where the most recient commands are on the top of the stack.
    pub fn export_history(&self, max: Option<usize>) -> Vec<String> {
        let skip = self.history.prev_entries.len()
            - max
                .filter(|&m| {
                    debug_assert!(m != 0, "use `Vec::new`");
                    m <= self.history.prev_entries.len()
                })
                .unwrap_or(self.history.prev_entries.len());

        self.history
            .prev_entries
            .values()
            .skip(skip)
            .cloned()
            .collect::<Vec<_>>()
    }
}
