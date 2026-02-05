use crate::line::Repl;

use std::{
    collections::{hash_map::DefaultHasher, BTreeMap, HashMap},
    hash::{Hash, Hasher},
    io::{self, Write},
};

#[non_exhaustive]
#[derive(Debug)]
pub enum TagError {
    EmptyHistory,
}

#[derive(Debug)]
pub struct Entry {
    value: String,
    tag: Option<u32>,
}

impl Entry {
    fn untagged(value: String) -> Self {
        Self { value, tag: None }
    }
    fn cloned_value(&self) -> String {
        self.value.clone()
    }

    pub fn value(&self) -> &str {
        &self.value
    }
    pub fn tag(&self) -> Option<u32> {
        self.tag
    }
}

impl std::fmt::Display for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[derive(Default)]
pub(super) struct History {
    prev_entries: BTreeMap<usize, Entry>,
    value_order_map: HashMap<u64, usize>,
    top: usize,
    temp_top: String,
    curr_pos: usize,
}

impl History {
    #[inline]
    fn pairs<'a>((ord, entry): (&usize, &'a Entry)) -> (usize, &'a str) {
        (*ord, entry.value())
    }

    /// Tries to get the next position and entry in the history after `curr_pos`
    #[inline]
    fn next(&self) -> Option<(usize, &str)> {
        self.prev_entries
            .range(self.curr_pos + 1..)
            .next()
            .map(Self::pairs)
    }

    /// Tries to get the next_back position and entry in the history before `curr_pos`
    #[inline]
    fn next_back(&self) -> Option<(usize, &str)> {
        self.prev_entries
            .range(..self.curr_pos)
            .next_back()
            .map(Self::pairs)
    }

    /// Items yield from most recent to oldest
    #[inline]
    pub(super) fn iter(&self) -> impl Iterator<Item = (usize, &str)> {
        self.prev_entries.iter().map(Self::pairs).rev()
    }

    /// Returns the most recent entry
    #[inline]
    pub(super) fn last_entry(&self) -> Option<&str> {
        self.prev_entries.values().next_back().map(Entry::value)
    }

    /// Returns the position of the first entry
    #[inline]
    fn first_position(&self) -> Option<usize> {
        self.prev_entries.keys().next().copied()
    }

    /// Returns the position of the most recent entry
    #[inline]
    fn last_position(&self) -> Option<usize> {
        self.prev_entries.keys().next_back().copied()
    }

    /// Returns the entry at a given position  
    #[inline]
    pub(super) fn get(&self, position: &usize) -> Option<&str> {
        self.prev_entries.get(position).map(Entry::value)
    }

    #[inline]
    pub(super) fn reset_idx(&mut self) {
        self.curr_pos = self.top;
    }

    fn push(&mut self, mut add: &str) {
        add = add.trim();

        if self.last_entry().is_some_and(|entry| entry == add) {
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
                self.prev_entries
                    .insert(new_last_p, Entry::untagged(add.to_string()));
                new_last_p
            });

        self.reset_idx();
    }

    fn get_skip_ct(max: Option<usize>, len: usize) -> usize {
        len.saturating_sub(max.unwrap_or(len))
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

impl<Ctx, W: Write> Repl<Ctx, W> {
    /// Pushes onto history and resets the internal history index to the top
    #[inline]
    pub fn add_to_history(&mut self, add: &str) {
        self.history.push(add);
    }

    /// Iterates over entries in the history from most recent to oldest. If you want an owned copy and to maintain
    /// the correct order of the history stack see: [`Self::export_history`].
    #[inline]
    pub fn history_entries(&self) -> impl Iterator<Item = &Entry> {
        self.history.prev_entries.values().rev()
    }

    /// Iterates over string values in the history from most recent to oldest. If you want an owned copy and to
    /// maintain the correct order of the history stack see: [`Self::export_history`].
    #[inline]
    pub fn history_values(&self) -> impl Iterator<Item = &str> {
        self.history.prev_entries.values().rev().map(Entry::value)
    }

    /// Changes the current line to the previous history entry if available
    pub fn history_back(&mut self) -> io::Result<()> {
        if self.history.prev_entries.is_empty()
            || self.history.curr_pos == self.history.first_position().expect("history is not empty")
        {
            self.set_uneventful();
            return Ok(());
        }

        let (pos, entry) = self
            .history
            .next_back()
            .map(|(pos, entry)| (pos, entry.to_string()))
            .expect("missed early return so `curr_pos` must be greater than the `first_position`");

        let prev = self.change_line(entry)?;

        if self.history.curr_pos == self.history.top {
            self.history.temp_top = prev
        }

        self.history.curr_pos = pos;
        Ok(())
    }

    /// Changes the current line to the next history entry if available
    pub fn history_forward(&mut self) -> io::Result<()> {
        if self.history.curr_pos == self.history.top {
            self.set_uneventful();
            return Ok(());
        }

        let (pos, entry) = if self.history.curr_pos
            == self
                .history
                .last_position()
                .expect("missed early return so `history_back` must have been called before")
        {
            (self.history.top, std::mem::take(&mut self.history.temp_top))
        } else {
            self.history
                .next()
                .map(|(pos, entry)| (pos, entry.to_string()))
                .expect("`curr_pos` is neither top nor `last_position`, so there must be at least one more entry")
        };

        self.change_line(entry)?;
        self.history.curr_pos = pos;
        Ok(())
    }

    /// Returns history exported via clone as a new `Vec` where the most recent commands are on the top of the stack.
    pub fn export_history(&self, max: Option<usize>) -> Vec<String> {
        let skip = History::get_skip_ct(max, self.history.prev_entries.len());

        self.history
            .prev_entries
            .values()
            .skip(skip)
            .map(Entry::cloned_value)
            .collect()
    }

    /// Returns history exported via clone as a new `Vec` where the most recent commands are on the top of the stack
    /// and only contain entries for which the closure returns `true`.
    pub fn export_filtered_history(
        &self,
        f: impl Fn(Option<u32>) -> bool,
        max: Option<usize>,
    ) -> Vec<String> {
        let filtered = self
            .history
            .prev_entries
            .values()
            .filter(|entry| f(entry.tag))
            .map(Entry::value)
            .collect::<Vec<_>>();

        let skip = History::get_skip_ct(max, filtered.len());

        filtered
            .into_iter()
            .skip(skip)
            .map(str::to_string)
            .collect()
    }

    /// Provides mutable access to the tag on the last entry added to history. Can return an error if history
    /// is empty. Note: Each `String` stored in the history can only appear once, thus if multiple tags are needed
    /// bitflags must be used.
    pub fn tag_last_history(&mut self, set: impl FnOnce(&mut Option<u32>)) -> Result<(), TagError> {
        let Some(mut last) = self.history.prev_entries.last_entry() else {
            return Err(TagError::EmptyHistory);
        };

        let last = last.get_mut();
        set(&mut last.tag);

        Ok(())
    }
}
