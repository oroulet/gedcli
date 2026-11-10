use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use ratatui_image::Image;
use ratatui_image::protocol::Protocol;

use super::app::App;
use super::screen::{
    CENTER, CHILDREN, GRANDCHILDREN, GRANDPARENTS, GREAT_GRANDCHILDREN, GREAT_GRANDPARENTS, Group,
    PARENTS, ROWS, Screen,
};
use super::view::{box_height, box_width, dim, draw_box, main_lines, person_lines, visible_window};

const MARRIAGE: &str = " ⚭ ";
const MAX_TEXT: u16 = 22;
const MAX_MAIN_TEXT: u16 = 60;
const SIBLING_GAP: u16 = 3;
const ARROW_ROOM: u16 = 3;
const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

struct Item {
    index: usize,
    rect: Rect,
    lines: Vec<Line<'static>>,
}

struct RowPlan {
    row: usize,
    y: u16,
    height: u16,
    items: Vec<Item>,
    hidden_before: usize,
    hidden_after: usize,
}

struct Band {
    y: u16,
    lanes: Vec<(Vec<usize>, Vec<usize>, u16)>,
    upper: usize,
    lower: usize,
}

pub struct Plan {
    rows: Vec<RowPlan>,
    bands: Vec<Band>,
    pub height: u16,
}

/// Parents, person and children always; then grandparents, grandchildren and the
/// great-grand rows, each only when it has people, fits, and its neighbour is shown.
pub fn visible_rows(area: Rect, app: &App, screen: &Screen) -> [bool; ROWS] {
    let mut visible = [false; ROWS];
    for row in [PARENTS, CENTER, CHILDREN] {
        visible[row] = true;
    }
    let extras = [
        (GRANDPARENTS, PARENTS),
        (GRANDCHILDREN, CHILDREN),
        (GREAT_GRANDPARENTS, GRANDPARENTS),
        (GREAT_GRANDCHILDREN, GRANDCHILDREN),
    ];
    for (row, neighbour) in extras {
        let mut trial = visible;
        trial[row] = visible[neighbour] && !screen.rows[row].is_empty();
        if trial[row] && plan(area, app, screen, trial).height <= area.height {
            visible = trial;
        }
    }
    visible
}

pub fn plan(area: Rect, app: &App, screen: &Screen, visible: [bool; ROWS]) -> Plan {
    let mut rows: Vec<RowPlan> = (0..ROWS)
        .filter(|&row| visible[row])
        .map(|row| row_plan(area, app, screen, row))
        .collect();
    let mut bands: Vec<Band> = rows
        .windows(2)
        .enumerate()
        .map(|(i, pair)| band(&screen.links[pair[0].row], &pair[0], &pair[1], i))
        .collect();
    let band_rows = |b: &Band| b.lanes.iter().map(|l| l.2 + 1).max().unwrap_or(1);
    let height =
        rows.iter().map(|r| r.height).sum::<u16>() + bands.iter().map(band_rows).sum::<u16>();
    let mut y = area.y + area.height.saturating_sub(height) / 2;
    for (i, row) in rows.iter_mut().enumerate() {
        shift(row, y);
        y += row.height;
        if let Some(band) = bands.get_mut(i) {
            band.y = y;
            y += band_rows(band);
        }
    }
    Plan {
        rows,
        bands,
        height,
    }
}

fn shift(row: &mut RowPlan, y: u16) {
    row.y = y;
    for item in &mut row.items {
        item.rect.y = y;
    }
}

fn row_plan(area: Rect, app: &App, screen: &Screen, row: usize) -> RowPlan {
    let people = &screen.rows[row];
    let boxes: Vec<Vec<Line<'static>>> = if row == CENTER {
        center_boxes(area, screen)
    } else {
        people
            .iter()
            .enumerate()
            .map(|(i, p)| with_photo_room(person_lines(p, MAX_TEXT), screen.photo_size(row, i)))
            .collect()
    };
    let focus = (app.focus.0 == row).then(|| app.focus_index(screen));
    let widest_gap = if row == CENTER { SIBLING_GAP } else { 1 };
    let (start, end) = match visible_window(&boxes, area.width, focus, widest_gap) {
        (0, end) if end == boxes.len() => (0, end),
        _ => visible_window(
            &boxes,
            area.width.saturating_sub(2 * ARROW_ROOM),
            focus,
            widest_gap,
        ),
    };
    let gaps: Vec<u16> = (start..end)
        .map(|i| gap_before(row, i, start, screen))
        .collect();
    let widths: Vec<u16> = boxes[start..end].iter().map(|b| box_width(b)).collect();
    let total = widths.iter().sum::<u16>() + gaps.iter().sum::<u16>();
    let mut x = area.x + area.width.saturating_sub(total) / 2;
    let items: Vec<Item> = boxes
        .into_iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .zip(widths.into_iter().zip(gaps))
        .map(|((index, lines), (width, gap))| {
            x += gap;
            let rect = Rect {
                x,
                y: 0,
                width: width.min(area.right().saturating_sub(x)),
                height: box_height(&lines),
            };
            x += width;
            Item { index, rect, lines }
        })
        .collect();
    let height = items.iter().map(|i| i.rect.height).max().unwrap_or(1);
    RowPlan {
        row,
        y: 0,
        height,
        items,
        hidden_before: start,
        hidden_after: people.len() - end,
    }
}

/// Leaves an empty column on the left of the centre person's text where the portrait is drawn.
/// Leaves an empty area on the left of a card's text where its photo is drawn.
fn with_photo_room(mut lines: Vec<Line<'static>>, size: Option<(u16, u16)>) -> Vec<Line<'static>> {
    let Some((width, height)) = size else {
        return lines;
    };
    while lines.len() < usize::from(height) {
        lines.push(Line::default());
    }
    let room = " ".repeat(usize::from(width) + 1);
    for line in &mut lines {
        line.spans.insert(0, Span::raw(room.clone()));
    }
    lines
}

fn gap_before(row: usize, index: usize, start: usize, screen: &Screen) -> u16 {
    if index == start {
        return 0;
    }
    if row != CENTER {
        return 1;
    }
    let (first, last) = couple_span(screen);
    if index == last && first != last {
        MARRIAGE.width() as u16
    } else if index == first || index == last + 1 {
        SIBLING_GAP
    } else {
        1
    }
}

/// The positions of the couple (or the lone centre person) in the centre row.
fn couple_span(screen: &Screen) -> (usize, usize) {
    let partner = screen.partner_pos.unwrap_or(screen.center_pos);
    (
        screen.center_pos.min(partner),
        screen.center_pos.max(partner),
    )
}

fn center_boxes(area: Rect, screen: &Screen) -> Vec<Vec<Line<'static>>> {
    let partner_space = screen
        .partner_pos
        .and_then(|i| screen.rows[CENTER].get(i))
        .map_or(0, |p| {
            box_width(&person_lines(p, MAX_TEXT)) + MARRIAGE.width() as u16
        });
    let main_width = area
        .width
        .saturating_sub(partner_space + 4)
        .min(MAX_MAIN_TEXT);
    screen.rows[CENTER]
        .iter()
        .enumerate()
        .map(|(i, person)| {
            if i == screen.center_pos {
                with_photo_room(main_lines(screen, main_width), screen.photo_size(CENTER, i))
            } else {
                with_photo_room(person_lines(person, MAX_TEXT), screen.photo_size(CENTER, i))
            }
        })
        .collect()
}

fn band(groups: &[Group], upper: &RowPlan, lower: &RowPlan, index: usize) -> Band {
    let shown = |row: &RowPlan, wanted: &[usize]| -> Vec<usize> {
        row.items
            .iter()
            .enumerate()
            .filter(|(_, it)| wanted.contains(&it.index))
            .map(|(i, _)| i)
            .collect()
    };
    let mut lanes: Vec<(Vec<usize>, Vec<usize>, u16)> = Vec::new();
    let mut ends: Vec<u16> = Vec::new();
    for group in groups {
        let (ups, downs) = (shown(upper, &group.upper), shown(lower, &group.lower));
        if ups.is_empty() || downs.is_empty() {
            continue;
        }
        let xs: Vec<u16> = ups
            .iter()
            .map(|&i| center(&upper.items[i].rect))
            .chain(downs.iter().map(|&i| center(&lower.items[i].rect)))
            .collect();
        let (from, to) = (*xs.iter().min().expect("xs"), *xs.iter().max().expect("xs"));
        let lane = ends
            .iter()
            .position(|&end| end + 1 < from)
            .unwrap_or(ends.len());
        if lane == ends.len() {
            ends.push(to);
        } else {
            ends[lane] = to;
        }
        lanes.push((ups, downs, lane as u16));
    }
    Band {
        y: 0,
        lanes,
        upper: index,
        lower: index + 1,
    }
}

fn center(rect: &Rect) -> u16 {
    rect.x + rect.width / 2
}

pub fn draw_family(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    screen: &Screen,
    images: &HashMap<String, &Protocol>,
) {
    let plan = plan(area, app, screen, app.visible);
    for row in &plan.rows {
        draw_row(frame, area, app, screen, row);
        draw_photos(frame, row, screen, images);
    }
    let mut lines = Connectors::default();
    for band in &plan.bands {
        lines.band(band, &plan.rows[band.upper], &plan.rows[band.lower]);
    }
    lines.render(frame);
}

fn draw_photos(
    frame: &mut Frame,
    row: &RowPlan,
    screen: &Screen,
    images: &HashMap<String, &Protocol>,
) {
    for item in &row.items {
        let Some(person) = screen.at(row.row, item.index) else {
            continue;
        };
        let (Some(image), Some((width, height))) = (
            images.get(&person.id.to_ascii_uppercase()),
            screen.photo_size(row.row, item.index),
        ) else {
            continue;
        };
        let area = Rect {
            x: item.rect.x + 2,
            y: item.rect.y + 1,
            width: width.min(item.rect.width.saturating_sub(4)),
            height: height.min(item.rect.height.saturating_sub(2)),
        };
        frame.render_widget(Image::new(image), area);
    }
}

fn draw_row(frame: &mut Frame, area: Rect, app: &App, screen: &Screen, row: &RowPlan) {
    if row.items.is_empty() {
        let text = format!("{}: none recorded", label(row.row, screen));
        let rect = Rect {
            x: area.x,
            y: row.y,
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Span::styled(text, dim())).centered(), rect);
        return;
    }
    for item in &row.items {
        draw_box(
            frame,
            item.rect,
            item.lines.clone(),
            app.is_focused(row.row, item.index, screen),
        );
    }
    let (first, last) = couple_span(screen);
    if row.row == CENTER
        && first != last
        && let Some(right) = row.items.iter().find(|i| i.index == last)
    {
        let mark = Rect {
            x: right.rect.x - MARRIAGE.width() as u16,
            y: row.y + 1,
            width: MARRIAGE.width() as u16,
            height: 1,
        };
        frame.render_widget(Paragraph::new(MARRIAGE).style(dim()), mark);
    }
    draw_label(frame, area, screen, row);
}

fn draw_label(frame: &mut Frame, area: Rect, screen: &Screen, row: &RowPlan) {
    let first = row.items[0].rect.x;
    let last = row.items.last().map_or(first, |i| i.rect.right());
    let text = label(row.row, screen);
    let room = first.saturating_sub(area.x + 1);
    if !text.is_empty() && (text.width() as u16) < room {
        put(frame, area.x + 1, row.y, room, &text);
    }
    let middle = row.y + row.height / 2;
    if row.hidden_before > 0 {
        let more = format!("◀{}", row.hidden_before);
        put(frame, area.x, middle, first.saturating_sub(area.x), &more);
    }
    if row.hidden_after > 0 {
        let more = format!("{}▶", row.hidden_after);
        let x = area.right().saturating_sub(more.width() as u16).max(last);
        put(frame, x, middle, area.right().saturating_sub(x), &more);
    }
}

fn put(frame: &mut Frame, x: u16, y: u16, width: u16, text: &str) {
    if width > 0 {
        let rect = Rect {
            x,
            y,
            width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Span::styled(text.to_owned(), dim())), rect);
    }
}

fn label(row: usize, screen: &Screen) -> String {
    match row {
        GREAT_GRANDPARENTS => "great-grandparents".to_owned(),
        GRANDPARENTS => "grandparents".to_owned(),
        PARENTS => "parents".to_owned(),
        CENTER if screen.partners.len() > 1 => format!(
            "partner {}/{} · n",
            screen.partner_index + 1,
            screen.partners.len()
        ),
        CHILDREN => match screen.partner() {
            Some(p) => format!(
                "children with {}",
                p.person
                    .as_ref()
                    .map_or("unknown partner", |b| b.name.as_str())
            ),
            None => "children".to_owned(),
        },
        GRANDCHILDREN => "grandchildren".to_owned(),
        GREAT_GRANDCHILDREN => "great-grandchildren".to_owned(),
        _ => String::new(),
    }
}

#[derive(Default)]
struct Connectors {
    /// Line directions per cell, kept per family group so crossings can be told from junctions.
    cells: HashMap<(u16, u16), HashMap<usize, u8>>,
    joints: Vec<(u16, u16, bool)>,
    group: usize,
}

impl Connectors {
    fn band(&mut self, band: &Band, upper: &RowPlan, lower: &RowPlan) {
        for (ups, downs, lane) in &band.lanes {
            self.group += 1;
            let y = band.y + lane;
            let mut xs = Vec::new();
            for &i in ups {
                let rect = upper.items[i].rect;
                let x = center(&rect);
                self.joints.push((x, rect.bottom() - 1, true));
                self.mark(x, rect.bottom(), UP);
                self.vertical(x, rect.bottom(), y);
                xs.push(x);
            }
            let bottom = lower.y - 1;
            for &i in downs {
                let x = center(&lower.items[i].rect);
                self.joints.push((x, lower.y, false));
                self.vertical(x, y, bottom);
                self.mark(x, bottom, DOWN);
                xs.push(x);
            }
            self.horizontal(
                y,
                *xs.iter().min().expect("xs"),
                *xs.iter().max().expect("xs"),
            );
        }
    }

    fn mark(&mut self, x: u16, y: u16, flag: u8) {
        *self
            .cells
            .entry((x, y))
            .or_default()
            .entry(self.group)
            .or_default() |= flag;
    }

    fn vertical(&mut self, x: u16, from: u16, to: u16) {
        for y in from..to {
            self.mark(x, y, DOWN);
            self.mark(x, y + 1, UP);
        }
    }

    fn horizontal(&mut self, y: u16, from: u16, to: u16) {
        for x in from..to {
            self.mark(x, y, RIGHT);
            self.mark(x + 1, y, LEFT);
        }
    }

    fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let buffer = frame.buffer_mut();
        for (&(x, y), groups) in &self.cells {
            if x < area.right() && y < area.bottom() {
                buffer[(x, y)]
                    .set_symbol(cell_glyph(groups))
                    .set_style(dim());
            }
        }
        for &(x, y, below) in &self.joints {
            if x < area.right() && y < area.bottom() {
                let cell = &mut buffer[(x, y)];
                let thick = cell.symbol() == "━";
                let joint = match (below, thick) {
                    (true, false) => "┬",
                    (true, true) => "┳",
                    (false, false) => "┴",
                    (false, true) => "┻",
                };
                cell.set_symbol(joint);
            }
        }
    }
}

/// One family's lines merge into junctions; a vertical line of one family crossing
/// another family's horizontal line is drawn as a bridge (│), not a junction.
fn cell_glyph(groups: &HashMap<usize, u8>) -> &'static str {
    let vertical = UP | DOWN;
    let crossing = groups.len() > 1
        && groups.values().any(|&f| f & vertical == f)
        && groups.values().any(|&f| f & vertical == 0);
    if crossing {
        return "│";
    }
    glyph(groups.values().fold(0, |all, &f| all | f))
}

fn glyph(flags: u8) -> &'static str {
    let has = |f: u8| flags & f != 0;
    match (has(UP), has(DOWN), has(LEFT), has(RIGHT)) {
        (true, true, true, true) => "┼",
        (true, true, true, false) => "┤",
        (true, true, false, true) => "├",
        (false, true, true, true) => "┬",
        (true, false, true, true) => "┴",
        (false, true, false, true) => "┌",
        (false, true, true, false) => "┐",
        (true, false, false, true) => "└",
        (true, false, true, false) => "┘",
        (_, _, false, false) => "│",
        _ => "─",
    }
}

/// Each drawn connection as (upper row, lower row, upper indices, lower indices).
#[cfg(test)]
pub fn drawn_links(
    area: Rect,
    app: &App,
    screen: &Screen,
) -> Vec<(usize, usize, Vec<usize>, Vec<usize>)> {
    let plan = plan(area, app, screen, app.visible);
    plan.bands
        .iter()
        .flat_map(|band| {
            let (upper, lower) = (&plan.rows[band.upper], &plan.rows[band.lower]);
            band.lanes.iter().map(move |(ups, downs, _)| {
                let index = |row: &RowPlan, items: &[usize]| {
                    items.iter().map(|&i| row.items[i].index).collect()
                };
                (upper.row, lower.row, index(upper, ups), index(lower, downs))
            })
        })
        .collect()
}

#[cfg(test)]
mod glyph_tests {
    use super::*;

    #[test]
    fn photo_room_is_left_of_the_text_and_tall_enough() {
        let lines = vec![Line::from("Kari Lund"), Line::from("b. 1950")];
        let roomy = with_photo_room(lines.clone(), Some((6, 3)));
        assert_eq!(roomy.len(), 3);
        assert_eq!(roomy[0].to_string(), "       Kari Lund");
        assert_eq!(with_photo_room(lines, None).len(), 2);
    }

    #[test]
    fn crossing_of_two_families_is_a_bridge_but_one_family_joins() {
        let crossing = HashMap::from([(1, UP | DOWN), (2, LEFT | RIGHT)]);
        assert_eq!(cell_glyph(&crossing), "│");
        let junction = HashMap::from([(1, UP | DOWN | LEFT | RIGHT)]);
        assert_eq!(cell_glyph(&junction), "┼");
        let tee = HashMap::from([(1, LEFT | RIGHT), (2, DOWN | LEFT)]);
        assert_eq!(cell_glyph(&tee), "┬");
    }
}
