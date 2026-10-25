//! The lists AutoFill continues: day and month names, and any list a person adds.

/// Lists of text that fill continues in order, such as `Mon, Tue, Wed`. Excel starts with four: the short
/// and the long day names and month names. The caller keeps the lists, for instance in the settings.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomLists {
    lists: Vec<Vec<String>>,
}

/// What a list holds, which decides whether the day, weekday and month fills continue it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Days,
    Months,
    Other,
}

const DAYS_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS_SHORT: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

impl Default for CustomLists {
    fn default() -> Self {
        Self::excel_defaults()
    }
}

impl CustomLists {
    /// Excel's four built in lists.
    pub fn excel_defaults() -> Self {
        let owned = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>();
        Self {
            lists: vec![
                owned(&DAYS_SHORT),
                owned(&crate::dates::DAYS),
                owned(&MONTHS_SHORT),
                owned(&crate::dates::MONTHS),
            ],
        }
    }

    /// No lists at all.
    pub fn none() -> Self {
        Self { lists: Vec::new() }
    }

    /// Every list, the oldest first.
    pub fn lists(&self) -> &[Vec<String>] {
        &self.lists
    }

    /// Add a list. It needs two entries or more, and no entry may be empty.
    pub fn add(&mut self, items: Vec<String>) -> Result<(), String> {
        if items.len() < 2 || items.iter().any(|i| i.trim().is_empty()) {
            return Err("A custom list needs at least two entries, none of them empty.".to_owned());
        }
        if self.lists.contains(&items) {
            return Err("That list is already there.".to_owned());
        }
        self.lists.push(items);
        Ok(())
    }

    /// Take a list away by its place among the lists.
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        if index >= self.lists.len() {
            return Err(format!("there is no list {index}"));
        }
        self.lists.remove(index);
        Ok(())
    }

    /// The lists that hold a text in any case, the newest list first.
    pub fn containing(&self, text: &str) -> Vec<usize> {
        let text = text.trim();
        (0..self.lists.len())
            .rev()
            .filter(|i| self.lists[*i].iter().any(|item| item.eq_ignore_ascii_case(text)))
            .collect()
    }

    /// The list that holds every one of some texts, newest list first, and where each is in it. A text
    /// matches an entry in any case.
    pub fn locate(&self, texts: &[String]) -> Option<(usize, Vec<usize>)> {
        'lists: for (index, list) in self.lists.iter().enumerate().rev() {
            let mut at = Vec::with_capacity(texts.len());
            for t in texts {
                match list.iter().position(|item| item.eq_ignore_ascii_case(t.trim())) {
                    Some(p) => at.push(p),
                    None => continue 'lists,
                }
            }
            return Some((index, at));
        }
        None
    }

    /// Whether a list is the day names or the month names.
    pub fn kind_of(&self, index: usize) -> ListKind {
        let list = &self.lists[index];
        let matches = |names: &[&str]| {
            list.len() == names.len()
                && list
                    .iter()
                    .zip(names)
                    .all(|(a, b)| a.to_ascii_lowercase().starts_with(&b.to_ascii_lowercase()))
        };
        if matches(&DAYS_SHORT) {
            ListKind::Days
        } else if matches(&MONTHS_SHORT) {
            ListKind::Months
        } else {
            ListKind::Other
        }
    }
}

/// How a name is written: as the seed was, in capitals, or in lower case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    AsListed,
    Upper,
    Lower,
}

impl Case {
    /// The case a seed text is written in.
    pub fn of(seed: &str) -> Case {
        let letters: Vec<char> = seed.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
            Case::Upper
        } else if !letters.is_empty() && letters.iter().all(|c| c.is_lowercase()) {
            Case::Lower
        } else {
            Case::AsListed
        }
    }

    /// An entry of a list written in this case.
    pub fn apply(self, item: &str) -> String {
        match self {
            Case::AsListed => item.to_owned(),
            Case::Upper => item.to_uppercase(),
            Case::Lower => item.to_lowercase(),
        }
    }
}
