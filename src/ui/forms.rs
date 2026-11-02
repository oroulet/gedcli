use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::add::Relation;
use crate::edit::{self, FIELDS, Field};
use crate::person::{Brief, People};
use crate::search;

const SEARCH_RESULTS: usize = 12;

#[derive(Default, Clone)]
pub struct Input {
    pub text: String,
}

impl Input {
    pub fn with(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.clear()
            }
            KeyCode::Char(c) => self.text.push(c),
            KeyCode::Backspace => {
                self.text.pop();
            }
            _ => return false,
        }
        true
    }
}

pub struct SearchForm {
    pub input: Input,
    pub results: Vec<Brief>,
    pub selected: usize,
}

impl SearchForm {
    pub fn new() -> Self {
        Self {
            input: Input::default(),
            results: Vec::new(),
            selected: 0,
        }
    }

    pub fn refresh(&mut self, people: &People) {
        self.results = if self.input.text.trim().is_empty() {
            Vec::new()
        } else {
            search::find(people, &self.input.text)
                .into_iter()
                .take(SEARCH_RESULTS)
                .collect()
        };
        self.selected = 0;
    }
}

pub struct ShowForm {
    pub person: Brief,
    pub lines: Vec<String>,
    pub scroll: usize,
}

impl ShowForm {
    pub fn new(people: &People, person: Brief) -> Self {
        let lines = people
            .record(&person.id)
            .map(|record| crate::card::render(&crate::card::build(people, record)))
            .unwrap_or_default()
            .lines()
            .skip(1)
            .map(str::to_owned)
            .collect();
        Self {
            person,
            lines,
            scroll: 0,
        }
    }

    pub fn scroll_by(&mut self, step: isize) {
        let last = self.lines.len().saturating_sub(1);
        self.scroll = self.scroll.saturating_add_signed(step).min(last);
    }
}

pub struct EditForm {
    pub person: Brief,
    pub values: Vec<Option<String>>,
    pub selected: usize,
    pub input: Option<Input>,
}

impl EditForm {
    pub fn new(people: &People, person: Brief) -> Self {
        let record = people.record(&person.id);
        let values = FIELDS
            .iter()
            .map(|&f| record.and_then(|r| edit::get(r, f)))
            .collect();
        Self {
            person,
            values,
            selected: 0,
            input: None,
        }
    }

    pub fn field(&self) -> Field {
        FIELDS[self.selected]
    }
}

pub const ADD_FIELDS: [&str; 5] = ["relation", "first name", "surname", "birth", "death"];

pub struct AddForm {
    pub anchor: Brief,
    pub family: Option<String>,
    pub choices: Vec<Relation>,
    pub choice: usize,
    pub inputs: [Input; 4],
    pub selected: usize,
}

impl AddForm {
    pub fn new(anchor: Brief, family: Option<String>, choices: Vec<Relation>) -> Self {
        let surname = surname_guess(&anchor.name, &choices);
        Self {
            anchor,
            family,
            choices,
            choice: 0,
            inputs: [
                Input::default(),
                Input::with(&surname),
                Input::default(),
                Input::default(),
            ],
            selected: 1,
        }
    }

    pub fn relation(&self) -> Relation {
        self.choices[self.choice]
    }
}

fn surname_guess(anchor: &str, choices: &[Relation]) -> String {
    let child = choices
        .first()
        .is_some_and(|r| matches!(r, Relation::Son | Relation::Daughter | Relation::Child));
    if child {
        anchor
            .split_whitespace()
            .last()
            .unwrap_or_default()
            .to_owned()
    } else {
        String::new()
    }
}
