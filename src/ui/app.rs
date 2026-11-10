use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::forms::{
    ADD_FIELDS, AddForm, DISCARD_ROW, EDIT_ROWS, EditForm, SearchForm, ShowForm, WRITE_ROW,
};
use super::screen::{CENTER, ROWS, Screen};
use crate::add::Relation;
use crate::edit::Field;
use crate::person::{Brief, People};

const RECENT: usize = 12;

pub enum Mode {
    Browse,
    Help,
    Search(SearchForm),
    Show(ShowForm),
    Edit(EditForm),
    Add(AddForm),
    /// Asking "quit?"; keeps the mode to return to on "no".
    ConfirmQuit(Box<Mode>),
}

pub enum Effect {
    None,
    /// Store the edited copy of a person, with the changes made to it.
    Write {
        person: String,
        record: gedcomkit::Node,
        changes: Vec<crate::edit::Change>,
    },
    Add(AddRequest),
}

pub struct AddRequest {
    pub anchor: String,
    pub relation: Relation,
    pub family: Option<String>,
    pub given: String,
    pub surname: String,
    pub birth: String,
    pub death: String,
}

pub struct App {
    pub center: String,
    pub partner_index: usize,
    /// The highlighted row and person; `None` means the centre person.
    pub focus: (usize, Option<String>),
    pub mode: Mode,
    pub status: String,
    pub visible: [bool; ROWS],
    pub quit: bool,
    history: Vec<String>,
    recent: Vec<String>,
    pub show_portraits: bool,
}

impl App {
    pub fn new(center: String) -> Self {
        Self {
            center,
            partner_index: 0,
            focus: (CENTER, None),
            mode: Mode::Browse,
            status: String::new(),
            visible: [true; ROWS],
            quit: false,
            history: Vec::new(),
            recent: Vec::new(),
            show_portraits: false,
        }
    }

    pub fn screen(&self, people: &People) -> Screen {
        Screen::build(people, &self.center, self.partner_index)
    }

    pub fn key(&mut self, key: KeyEvent, people: &People) -> Effect {
        if let Mode::ConfirmQuit(_) = self.mode {
            self.confirm_quit_key(key);
            return Effect::None;
        }
        if is_ctrl_c(key) {
            self.ask_to_quit();
            return Effect::None;
        }
        let screen = self.screen(people);
        match std::mem::replace(&mut self.mode, Mode::Browse) {
            Mode::Browse => self.browse_key(key, &screen, people),
            Mode::Help => Effect::None,
            Mode::Show(form) => self.show_key(key, form, people),
            Mode::Search(form) => self.search_key(key, form, people),
            Mode::Edit(form) => self.edit_key(key, form),
            Mode::Add(form) => self.add_key(key, form),
            confirm @ Mode::ConfirmQuit(_) => {
                self.mode = confirm;
                Effect::None
            }
        }
    }

    fn browse_key(&mut self, key: KeyEvent, screen: &Screen, people: &People) -> Effect {
        match key.code {
            KeyCode::Char('q') => self.ask_to_quit(),
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Char('h') | KeyCode::Left => self.move_sideways(screen, -1),
            KeyCode::Char('l') | KeyCode::Right => self.move_sideways(screen, 1),
            KeyCode::Char('k') | KeyCode::Up => self.move_vertically(screen, -1),
            KeyCode::Char('j') | KeyCode::Down => self.move_vertically(screen, 1),
            KeyCode::Enter if self.focused(screen).id == screen.center().id => {
                self.open_show(screen, people)
            }
            KeyCode::Enter => self.recenter_on_focus(screen),
            KeyCode::Char('s') => self.open_show(screen, people),
            KeyCode::Char('b') | KeyCode::Backspace => self.back(),
            KeyCode::Char('n') | KeyCode::Tab => self.next_partner(screen),
            KeyCode::Char('i') => {
                self.show_portraits = !self.show_portraits;
                self.status = format!(
                    "portraits {}",
                    if self.show_portraits { "on" } else { "off" }
                );
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Search(SearchForm::new(people, &self.center, &self.recent))
            }
            KeyCode::Char('e') => self.open_edit(screen, people),
            KeyCode::Char('c') => self.open_add_child(screen, people),
            KeyCode::Char('p') => self.open_add_parent(screen),
            _ => {}
        }
        Effect::None
    }

    pub fn focused<'a>(&self, screen: &'a Screen) -> &'a Brief {
        screen
            .at(self.focus.0, self.focus_index(screen))
            .unwrap_or_else(|| screen.center())
    }

    /// Where the highlighted person sits in their row.
    pub fn focus_index(&self, screen: &Screen) -> usize {
        let row = &screen.rows[self.focus.0];
        let found = self
            .focus
            .1
            .as_deref()
            .and_then(|id| row.iter().position(|b| b.id.eq_ignore_ascii_case(id)));
        found.unwrap_or(if self.focus.0 == CENTER {
            screen.center_pos
        } else {
            0
        })
    }

    pub fn is_focused(&self, row: usize, index: usize, screen: &Screen) -> bool {
        self.focus.0 == row && self.focus_index(screen) == index
    }

    fn focus_on(&mut self, screen: &Screen, row: usize, index: usize) {
        self.focus = (row, screen.at(row, index).map(|b| b.id.clone()));
    }

    fn move_sideways(&mut self, screen: &Screen, step: isize) {
        let row = self.focus.0;
        let col = self.focus_index(screen).saturating_add_signed(step);
        if col < screen.rows[row].len() {
            self.focus_on(screen, row, col);
        }
    }

    fn move_vertically(&mut self, screen: &Screen, step: isize) {
        let (row, col) = (self.focus.0, self.focus_index(screen));
        let mut target = row;
        loop {
            match target.checked_add_signed(step).filter(|&r| r < ROWS) {
                None => return,
                Some(next) => target = next,
            }
            if self.visible[target] && !screen.rows[target].is_empty() {
                break;
            }
        }
        let from = screen.rows[row].len().max(1);
        let to = screen.rows[target].len();
        let col = if target == CENTER {
            screen.center_pos
        } else {
            (col * to / from).min(to - 1)
        };
        self.focus_on(screen, target, col);
    }

    fn recenter_on_focus(&mut self, screen: &Screen) {
        let person = self.focused(screen).id.clone();
        self.recenter(person);
    }

    pub fn recenter(&mut self, person: String) {
        if person.eq_ignore_ascii_case(&self.center) {
            return;
        }
        let previous = std::mem::replace(&mut self.center, person);
        self.remember_recent(&previous);
        self.history.push(previous);
        self.reset_view();
    }

    fn remember_recent(&mut self, person: &str) {
        self.recent.retain(|id| !id.eq_ignore_ascii_case(person));
        self.recent.insert(0, person.to_owned());
        self.recent.truncate(RECENT);
    }

    fn back(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.center = previous;
            self.reset_view();
        }
    }

    fn reset_view(&mut self) {
        self.partner_index = 0;
        self.focus = (CENTER, None);
    }

    fn next_partner(&mut self, screen: &Screen) {
        if screen.partners.len() > 1 {
            self.partner_index = (screen.partner_index + 1) % screen.partners.len();
            self.focus = (CENTER, None);
            self.status = format!(
                "partner {}/{}",
                self.partner_index + 1,
                screen.partners.len()
            );
        }
    }

    fn ask_to_quit(&mut self) {
        let previous = std::mem::replace(&mut self.mode, Mode::Browse);
        self.mode = Mode::ConfirmQuit(Box::new(previous));
    }

    fn confirm_quit_key(&mut self, key: KeyEvent) {
        let Mode::ConfirmQuit(previous) = std::mem::replace(&mut self.mode, Mode::Browse) else {
            return;
        };
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => self.quit = true,
            _ if is_ctrl_c(key) => self.quit = true,
            KeyCode::Char('n' | 'N') | KeyCode::Esc => self.mode = *previous,
            _ => self.mode = Mode::ConfirmQuit(previous),
        }
    }

    fn open_show(&mut self, screen: &Screen, people: &People) {
        self.mode = Mode::Show(ShowForm::new(people, self.focused(screen).clone()));
    }

    fn show_key(&mut self, key: KeyEvent, mut form: ShowForm, people: &People) -> Effect {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 's') | KeyCode::Enter => return Effect::None,
            KeyCode::Char('e') => {
                self.mode = Mode::Edit(EditForm::new(people, form.person));
                return Effect::None;
            }
            KeyCode::Down | KeyCode::Char('j') => form.scroll_by(1),
            KeyCode::Up | KeyCode::Char('k') => form.scroll_by(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => form.scroll_by(10),
            KeyCode::PageUp => form.scroll_by(-10),
            KeyCode::Home | KeyCode::Char('g') => form.scroll = 0,
            _ => {}
        }
        self.mode = Mode::Show(form);
        Effect::None
    }

    fn open_edit(&mut self, screen: &Screen, people: &People) {
        self.mode = Mode::Edit(EditForm::new(people, self.focused(screen).clone()));
    }

    fn open_add_child(&mut self, screen: &Screen, people: &People) {
        let anchor = self.focused(screen).clone();
        let family = if anchor.id.eq_ignore_ascii_case(&self.center) {
            screen.partner().map(|p| p.family.clone())
        } else {
            let unions = people.unions(&anchor.id);
            if unions.len() > 1 {
                self.status = format!(
                    "{} has several partners: centre them and pick one with n",
                    anchor.name
                );
                return;
            }
            unions.into_iter().next().map(|u| u.family)
        };
        let choices = vec![Relation::Son, Relation::Daughter, Relation::Child];
        self.mode = Mode::Add(AddForm::new(anchor, family, choices));
    }

    fn open_add_parent(&mut self, screen: &Screen) {
        let anchor = self.focused(screen).clone();
        let choices = vec![Relation::Father, Relation::Mother, Relation::Parent];
        self.mode = Mode::Add(AddForm::new(anchor, None, choices));
    }

    fn search_key(&mut self, key: KeyEvent, mut form: SearchForm, people: &People) -> Effect {
        match key.code {
            KeyCode::Esc => return Effect::None,
            KeyCode::Enter => {
                if let Some(person) = form.results.get(form.selected) {
                    self.recenter(person.id.clone());
                }
                return Effect::None;
            }
            KeyCode::Down => {
                form.selected = (form.selected + 1).min(form.results.len().saturating_sub(1))
            }
            KeyCode::Up => form.selected = form.selected.saturating_sub(1),
            _ => {
                if form.input.key(key) {
                    form.refresh(people);
                }
            }
        }
        self.mode = Mode::Search(form);
        Effect::None
    }

    fn edit_key(&mut self, key: KeyEvent, mut form: EditForm) -> Effect {
        if form.input.is_some() {
            self.edit_input_key(key, &mut form);
        } else if form.confirm_discard {
            if matches!(key.code, KeyCode::Char('y' | 'Y')) {
                return Effect::None;
            }
            form.confirm_discard = false;
        } else {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') if form.changes.is_empty() => {
                    return Effect::None;
                }
                KeyCode::Esc | KeyCode::Char('q') => form.confirm_discard = true,
                KeyCode::Char('w') => return write(form),
                KeyCode::Down | KeyCode::Char('j') => {
                    form.selected = (form.selected + 1).min(EDIT_ROWS - 1)
                }
                KeyCode::Up | KeyCode::Char('k') => form.selected = form.selected.saturating_sub(1),
                KeyCode::Enter if form.selected == WRITE_ROW => return write(form),
                KeyCode::Enter if form.selected == DISCARD_ROW => return Effect::None,
                KeyCode::Enter => match open_field(&form) {
                    Ok(input) => form.input = Some(input),
                    Err(message) => form.error = Some(message),
                },
                _ => {}
            }
        }
        self.mode = Mode::Edit(form);
        Effect::None
    }

    fn edit_input_key(&mut self, key: KeyEvent, form: &mut EditForm) {
        let Some(mut input) = form.input.take() else {
            return;
        };
        let Some(field) = form.field() else { return };
        match key.code {
            KeyCode::Esc => form.error = None,
            KeyCode::Enter => match form.apply(field, &input.text) {
                Ok(()) => form.error = None,
                Err(err) => {
                    form.error = Some(format!("{err:#}"));
                    form.input = Some(input);
                }
            },
            _ => {
                input.key(key);
                form.input = Some(input);
            }
        }
    }

    fn add_key(&mut self, key: KeyEvent, mut form: AddForm) -> Effect {
        match key.code {
            KeyCode::Esc => return Effect::None,
            KeyCode::Enter => return add_request(form),
            KeyCode::Down | KeyCode::Tab => form.selected = (form.selected + 1) % ADD_FIELDS.len(),
            KeyCode::Up | KeyCode::BackTab => {
                form.selected = (form.selected + ADD_FIELDS.len() - 1) % ADD_FIELDS.len()
            }
            KeyCode::Left if form.selected == 0 => {
                form.choice = (form.choice + form.choices.len() - 1) % form.choices.len()
            }
            KeyCode::Right | KeyCode::Char(' ') if form.selected == 0 => {
                form.choice = (form.choice + 1) % form.choices.len();
            }
            _ if form.selected > 0 => {
                form.inputs[form.selected - 1].key(key);
            }
            _ => {}
        }
        self.mode = Mode::Add(form);
        Effect::None
    }
}

fn is_ctrl_c(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn write(form: EditForm) -> Effect {
    if form.changes.is_empty() {
        return Effect::None;
    }
    Effect::Write {
        person: form.person.id,
        record: form.record,
        changes: form.changes,
    }
}

fn open_field(form: &EditForm) -> Result<super::forms::Input, String> {
    let current = form.values[form.selected].clone().unwrap_or_default();
    if form.field() == Some(Field::Note) && current.contains('\n') {
        return Err("multi-line note: edit it with `gedcli edit --set note=…`".to_owned());
    }
    Ok(super::forms::Input::with(&current))
}

fn add_request(form: AddForm) -> Effect {
    let relation = form.relation();
    let [given, surname, birth, death] = form.inputs.map(|i| i.text);
    Effect::Add(AddRequest {
        anchor: form.anchor.id,
        relation,
        family: form.family,
        given,
        surname,
        birth,
        death,
    })
}
