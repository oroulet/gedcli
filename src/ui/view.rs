use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Mode};
use super::forms::{ADD_FIELDS, AddForm, EditForm, SearchForm};
use super::screen::{CENTER, CHILDREN, GRANDCHILDREN, GRANDPARENTS, PARENTS, ROWS, Screen};
use crate::edit::FIELDS;
use crate::person::Brief;

const BOX_WIDTH: u16 = 26;
const BOX_HEIGHT: u16 = 6;
const ROW_HEIGHT: u16 = BOX_HEIGHT + 2;
const TALL: u16 = 42;
const KEYS: &str = " ? help  ←↓↑→/hjkl move  ⏎ centre  b back  n partner  / find  e edit  c child  p parent  q quit ";

pub fn visible_rows(height: u16) -> [bool; ROWS] {
    let tall = height >= TALL;
    [tall, true, true, true, tall]
}

pub fn draw(frame: &mut Frame, app: &App, screen: &Screen) {
    let area = frame.area();
    let [body, status] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let constraints = row_constraints(app.visible);
    let areas = Layout::vertical(constraints).split(body);
    let mut next = 0;
    for row in 0..ROWS {
        if !app.visible[row] {
            continue;
        }
        let area = areas[next];
        next += 1;
        if row == CENTER {
            draw_center(frame, area, app, screen);
        } else {
            draw_row(frame, area, app, screen, row);
        }
    }
    draw_status(frame, status, app);
    draw_overlay(frame, app);
}

fn row_constraints(visible: [bool; ROWS]) -> Vec<Constraint> {
    (0..ROWS)
        .filter(|&row| visible[row])
        .map(|row| {
            if row == CENTER {
                Constraint::Min(BOX_HEIGHT + 2)
            } else {
                Constraint::Length(ROW_HEIGHT)
            }
        })
        .collect()
}

fn row_title(row: usize, screen: &Screen) -> String {
    match row {
        GRANDPARENTS => " grandparents ".to_owned(),
        PARENTS => " parents ".to_owned(),
        CHILDREN => match screen.partner() {
            Some(p) => format!(
                " children with {} ",
                p.person
                    .as_ref()
                    .map_or("unknown partner", |b| b.name.as_str())
            ),
            None => " children ".to_owned(),
        },
        GRANDCHILDREN => " grandchildren ".to_owned(),
        _ => String::new(),
    }
}

fn draw_row(frame: &mut Frame, area: Rect, app: &App, screen: &Screen, row: usize) {
    let people = &screen.rows[row];
    let fits = usize::from((area.width.saturating_sub(2) / BOX_WIDTH).max(1));
    let focus = (app.focus.0 == row).then_some(app.focus.1);
    let start = window_start(people.len(), fits, focus);
    let shown = &people[start..people.len().min(start + fits)];
    let mut title = row_title(row, screen);
    if start > 0 || start + fits < people.len() {
        title.push_str(&format!(
            "{}–{} of {} ",
            start + 1,
            start + shown.len(),
            people.len()
        ));
    }
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .title(title)
        .border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if people.is_empty() {
        frame.render_widget(Paragraph::new(Span::styled(" none recorded", dim())), inner);
        return;
    }
    let total = BOX_WIDTH * shown.len() as u16;
    let mut x = inner.x + inner.width.saturating_sub(total) / 2;
    for (i, person) in shown.iter().enumerate() {
        let rect = Rect {
            x,
            y: inner.y,
            width: BOX_WIDTH.min(inner.right().saturating_sub(x)),
            height: BOX_HEIGHT.min(inner.height),
        };
        draw_person(frame, rect, person, focus == Some(start + i));
        x += BOX_WIDTH;
    }
}

fn window_start(len: usize, fits: usize, focus: Option<usize>) -> usize {
    match focus {
        Some(f) if f >= fits => (f + 1 - fits).min(len.saturating_sub(fits)),
        _ => 0,
    }
}

fn draw_person(frame: &mut Frame, area: Rect, person: &Brief, focused: bool) {
    let block = person_block(focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(person_lines(person, inner.width)), inner);
}

fn person_block(focused: bool) -> Block<'static> {
    if focused {
        Block::bordered()
            .border_type(BorderType::Thick)
            .border_style(Style::new().fg(Color::Yellow))
    } else {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(dim())
    }
}

fn person_lines(person: &Brief, width: u16) -> Vec<Line<'static>> {
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

fn draw_center(frame: &mut Frame, area: Rect, app: &App, screen: &Screen) {
    let title = match screen.partners.len() {
        0 | 1 => " person ".to_owned(),
        n => format!(
            " person · partner {}/{n} (n for next) ",
            screen.partner_index + 1
        ),
    };
    let block = Block::bordered().title(title).border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let partner_width = if screen.rows[CENTER].len() > 1 {
        BOX_WIDTH
    } else {
        0
    };
    let [main, gap, partner] = Layout::horizontal([
        Constraint::Min(20),
        Constraint::Length(if partner_width > 0 { 3 } else { 0 }),
        Constraint::Length(partner_width),
    ])
    .areas(inner);
    draw_main_person(frame, main, app, screen);
    if let Some(person) = screen.rows[CENTER].get(1) {
        frame.render_widget(Paragraph::new("\n\n ⚭").style(dim()), gap);
        let rect = Rect {
            height: BOX_HEIGHT.min(partner.height),
            ..partner
        };
        draw_person(frame, rect, person, app.focus == (CENTER, 1));
    }
}

fn draw_main_person(frame: &mut Frame, area: Rect, app: &App, screen: &Screen) {
    let person = screen.center();
    let block = person_block(app.focus == (CENTER, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = person_lines(person, inner.width);
    lines[0] = Line::from(vec![
        Span::styled(
            person.name.clone(),
            sex_style(person.sex.as_deref()).add_modifier(Modifier::UNDERLINED),
        ),
        Span::styled(format!("  {}", person.id), dim()),
    ]);
    lines.push(Line::default());
    lines.extend(screen.details.iter().map(|d| Line::from(d.clone())));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
        Mode::Edit(form) => draw_edit(frame, form),
        Mode::Add(form) => draw_add(frame, form),
    }
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

const HELP: [(&str, &str); 12] = [
    ("←↓↑→ / h j k l", "move between people"),
    ("Enter", "put the highlighted person in the centre"),
    ("b / Backspace", "back to the previous person"),
    ("n / Tab", "next partner (changes the children row)"),
    ("/", "find someone by name"),
    ("e", "edit the highlighted person's fields"),
    ("c", "add a child to the highlighted person"),
    ("p", "add a parent to the highlighted person"),
    ("?", "this help"),
    ("q / Esc", "quit"),
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

fn draw_edit(frame: &mut Frame, form: &EditForm) {
    let title = format!(
        " edit {} — ↑↓ field, Enter change, Esc close ",
        form.person.name
    );
    let inner = popup(frame, 76, FIELDS.len() as u16 + 6, title);
    let [list, input] = Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(inner);
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

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn focus_style() -> Style {
    Style::new().fg(Color::Black).bg(Color::Yellow)
}
