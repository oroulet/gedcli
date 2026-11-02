use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Padding, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Mode};
use super::family;
use super::forms::{ADD_FIELDS, AddForm, EditForm, SearchForm, ShowForm};
use super::screen::Screen;
use crate::edit::FIELDS;
use crate::person::Brief;

const MIN_TEXT: u16 = 8;
const PADDING: u16 = 1;
const KEYS: &str = " ? help  hjkl move  ⏎ centre  s show  b back  n partner  / find  e edit  c child  p parent  q quit ";

pub fn draw(frame: &mut Frame, app: &App, screen: &Screen) {
    let area = frame.area();
    let [body, status] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    family::draw_family(frame, body, app, screen);
    draw_status(frame, status, app);
    draw_overlay(frame, app);
}

pub(super) fn visible_window(
    boxes: &[Vec<Line>],
    width: u16,
    focus: Option<usize>,
    gap: u16,
) -> (usize, usize) {
    let fits_from = |start: usize| {
        let mut used = 0;
        let mut end = start;
        while end < boxes.len() {
            let next = used + box_width(&boxes[end]) + if end > start { gap } else { 0 };
            if next > width && end > start {
                break;
            }
            used = next;
            end += 1;
        }
        end
    };
    let mut start = 0;
    while focus.is_some_and(|f| f >= fits_from(start)) {
        start += 1;
    }
    (start, fits_from(start))
}

pub(super) fn box_width(lines: &[Line]) -> u16 {
    let text = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    text.max(MIN_TEXT) + 2 + 2 * PADDING
}

pub(super) fn box_height(lines: &[Line]) -> u16 {
    lines.len() as u16 + 2
}

pub(super) fn draw_box(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>, focused: bool) {
    let block = person_block(focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn person_block(focused: bool) -> Block<'static> {
    let block = Block::bordered().padding(Padding::horizontal(PADDING));
    if focused {
        block.border_type(BorderType::Thick).border_style(
            Style::new()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        block.border_type(BorderType::Rounded).border_style(dim())
    }
}

pub(super) fn person_lines(person: &Brief, width: u16) -> Vec<Line<'static>> {
    let style = sex_style(person.sex.as_deref());
    let mut lines: Vec<Line> = name_lines(&person.name, width)
        .into_iter()
        .map(|l| Line::from(Span::styled(l, style)))
        .collect();
    if let Some(life) = &person.life {
        lines.push(Line::from(Span::styled(fit(life, width), dim())));
    }
    if let Some(info) = &person.info {
        lines.push(Line::from(Span::styled(
            fit(info, width),
            dim().add_modifier(Modifier::ITALIC),
        )));
    }
    lines
}

pub(super) fn main_lines(screen: &Screen, width: u16) -> Vec<Line<'static>> {
    let person = screen.center();
    let mut lines = person_lines(person, width);
    for line in lines.iter_mut().take(1) {
        *line = line
            .clone()
            .patch_style(Style::new().add_modifier(Modifier::UNDERLINED));
    }
    if !screen.details.is_empty() {
        lines.push(Line::default());
        lines.extend(screen.details.iter().map(|d| Line::from(fit(d, width))));
    }
    lines
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let text = if app.status.is_empty() {
        KEYS.to_owned()
    } else {
        format!(" {}", app.status)
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::new().add_modifier(Modifier::REVERSED)),
        area,
    );
}

fn draw_overlay(frame: &mut Frame, app: &App) {
    match &app.mode {
        Mode::Browse => {}
        Mode::Help => draw_help(frame),
        Mode::Search(form) => draw_search(frame, form),
        Mode::Show(form) => draw_show(frame, form),
        Mode::Edit(form) => draw_edit(frame, form),
        Mode::Add(form) => draw_add(frame, form),
        Mode::ConfirmQuit(previous) => {
            draw_overlay_mode(frame, previous);
            draw_confirm_quit(frame);
        }
    }
}

fn draw_overlay_mode(frame: &mut Frame, mode: &Mode) {
    match mode {
        Mode::Help => draw_help(frame),
        Mode::Search(form) => draw_search(frame, form),
        Mode::Show(form) => draw_show(frame, form),
        Mode::Edit(form) => draw_edit(frame, form),
        Mode::Add(form) => draw_add(frame, form),
        Mode::Browse | Mode::ConfirmQuit(_) => {}
    }
}

fn draw_confirm_quit(frame: &mut Frame) {
    let inner = popup(frame, 34, 3, " quit ".to_owned());
    let text = Line::from(vec![
        Span::raw(" Quit gedcli? "),
        Span::styled("[Y/n]", Style::new().add_modifier(Modifier::BOLD)),
    ]);
    frame.render_widget(Paragraph::new(text), inner);
}

fn popup(frame: &mut Frame, width: u16, height: u16, title: String) -> Rect {
    let area = frame.area();
    let width = width.min(area.width);
    let height = height.min(area.height);
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}

const HELP: [(&str, &str); 13] = [
    ("←↓↑→ / h j k l", "move between people"),
    ("Enter", "put the highlighted person in the centre"),
    (
        "s / Enter again",
        "show all properties of the highlighted person",
    ),
    ("b / Backspace", "back to the previous person"),
    ("n / Tab", "next partner (changes the children row)"),
    ("/", "find someone by name"),
    ("e", "edit the highlighted person's fields"),
    ("c", "add a child to the highlighted person"),
    ("p", "add a parent to the highlighted person"),
    ("?", "this help"),
    ("q / Ctrl-C", "quit, after a Y/n question (Enter = yes)"),
    ("", ""),
    ("", "every change is saved to the file at once"),
];

fn draw_help(frame: &mut Frame) {
    let inner = popup(
        frame,
        64,
        HELP.len() as u16 + 4,
        " help — any key closes ".to_owned(),
    );
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(
                    format!(" {k:<16}"),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw(*v),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_search(frame: &mut Frame, form: &SearchForm) {
    let inner = popup(
        frame,
        70,
        18,
        " find — type a name, ↑↓ choose, Enter go, Esc cancel ".to_owned(),
    );
    let [input, list] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(format!("> {}▏", form.input.text)), input);
    let items: Vec<ListItem> = form
        .results
        .iter()
        .map(|b| ListItem::new(b.to_string()))
        .collect();
    let mut state =
        ListState::default().with_selected((!form.results.is_empty()).then_some(form.selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(focus_style())
            .highlight_symbol("▶ "),
        list,
        &mut state,
    );
}

fn draw_show(frame: &mut Frame, form: &ShowForm) {
    let title = format!(" {} — j/k scroll, e edit, Esc close ", form.person.name);
    let area = frame.area();
    let width = form
        .lines
        .iter()
        .map(|l| l.width() as u16)
        .max()
        .unwrap_or(0)
        .max(title.width() as u16)
        + 4;
    let height = (form.lines.len() as u16 + 2).min(area.height.saturating_sub(4));
    let inner = popup(
        frame,
        width.min(area.width.saturating_sub(4)),
        height,
        title,
    );
    let lines: Vec<Line> = form
        .lines
        .iter()
        .skip(form.scroll)
        .take(usize::from(inner.height))
        .map(|l| Line::from(fit(l, inner.width)))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_edit(frame: &mut Frame, form: &EditForm) {
    let title = format!(
        " edit {} — ↑↓ field, Enter change, Esc close ",
        form.person.name
    );
    let about_height = form.about.len() as u16;
    let inner = popup(frame, 76, FIELDS.len() as u16 + 7 + about_height, title);
    let [list, about, input] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(about_height + 1),
        Constraint::Length(3),
    ])
    .areas(inner);
    let about_text: Vec<Line> = form
        .about
        .iter()
        .map(|l| Line::from(format!("  {l}")))
        .collect();
    frame.render_widget(Paragraph::new(about_text).style(dim()), about);
    let items: Vec<ListItem> = FIELDS
        .iter()
        .zip(&form.values)
        .map(|(field, value)| {
            ListItem::new(format!("{:<14}{}", field.key(), preview(value.as_deref())))
        })
        .collect();
    let mut state = ListState::default().with_selected(Some(form.selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(focus_style())
            .highlight_symbol("▶ "),
        list,
        &mut state,
    );
    let hint = match &form.input {
        Some(input) => format!(
            "{} = {}▏\nEnter saves · empty removes · Esc cancels",
            form.field().key(),
            input.text
        ),
        None => "dates: 12 JUN 1950, ABT 1900, BET 1900 AND 1910 · sex: M, F, U".to_owned(),
    };
    frame.render_widget(Paragraph::new(hint).style(dim()), input);
}

fn draw_add(frame: &mut Frame, form: &AddForm) {
    let title = format!(
        " add a relative of {} — Tab/↑↓ field, ←→ relation, Enter add, Esc cancel ",
        form.anchor.name
    );
    let inner = popup(frame, 84, ADD_FIELDS.len() as u16 + 5, title);
    let mut lines = Vec::new();
    for (i, label) in ADD_FIELDS.iter().enumerate() {
        let value = if i == 0 {
            format!("◀ {} ▶", relation_word(form))
        } else {
            format!("{}▏", form.inputs[i - 1].text)
        };
        let style = if i == form.selected {
            focus_style()
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::raw(format!(" {label:<12}")),
            Span::styled(value, style),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        " dates are optional: 1950, 12 JUN 1950, ABT 1900 · surname - for none",
        dim(),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn relation_word(form: &AddForm) -> String {
    format!("{:?}", form.relation()).to_lowercase()
}

fn preview(value: Option<&str>) -> String {
    match value {
        None => "—".to_owned(),
        Some(v) => {
            let first = v.lines().next().unwrap_or_default();
            if v.contains('\n') {
                format!("{first} …")
            } else {
                first.to_owned()
            }
        }
    }
}

fn name_lines(name: &str, width: u16) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in name.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.width() + 1 + word.width() <= usize::from(width) => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    if lines.len() > 2 {
        let rest = lines.split_off(1).join(" ");
        lines.push(rest);
    }
    lines.into_iter().map(|l| fit(&l, width)).collect()
}

fn fit(text: &str, width: u16) -> String {
    let width = usize::from(width);
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    for c in text.chars() {
        if out.width() + 2 > width {
            break;
        }
        out.push(c);
    }
    out + "…"
}

fn sex_style(sex: Option<&str>) -> Style {
    let style = Style::new().add_modifier(Modifier::BOLD);
    match sex {
        Some("M") => style.fg(Color::Indexed(39)),
        Some("F") => style.fg(Color::Indexed(213)),
        _ => style.fg(Color::White),
    }
}

pub(super) fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn focus_style() -> Style {
    Style::new().fg(Color::Black).bg(Color::Yellow)
}
