use std::collections::HashMap;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::app::App;
use super::screen::{CENTER, CHILDREN, GRANDCHILDREN, GRANDPARENTS, Group, PARENTS, ROWS, Screen};
use super::view::{box_height, box_width, dim, draw_box, main_lines, person_lines, visible_window};

const MARRIAGE: &str = " ⚭ ";
const MAX_TEXT: u16 = 22;
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

pub fn visible_rows(area: Rect, app: &App, screen: &Screen) -> [bool; ROWS] {
    let mut visible = [false, true, true, true, false];
    for row in [GRANDPARENTS, GRANDCHILDREN] {
        let mut trial = visible;
        trial[row] = !screen.rows[row].is_empty();
        if plan(area, app, screen, trial).height <= area.height {
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
        people.iter().map(|p| person_lines(p, MAX_TEXT)).collect()
    };
    let gap = if row == CENTER {
        MARRIAGE.width() as u16
    } else {
        1
    };
    let focus = (app.focus.0 == row).then_some(app.focus.1);
    let (start, end) = if row == CENTER {
        (0, boxes.len())
    } else {
        visible_window(&boxes, area.width, focus)
    };
    let widths: Vec<u16> = boxes[start..end].iter().map(|b| box_width(b)).collect();
    let total = widths.iter().sum::<u16>() + gap * widths.len().saturating_sub(1) as u16;
    let mut x = area.x + area.width.saturating_sub(total) / 2;
    let items: Vec<Item> = boxes
        .into_iter()
        .enumerate()
        .skip(start)
        .take(end - start)
        .zip(widths)
        .map(|((index, lines), width)| {
            let rect = Rect {
                x,
                y: 0,
                width: width.min(area.right().saturating_sub(x)),
                height: box_height(&lines),
            };
            x += width + gap;
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

fn center_boxes(area: Rect, screen: &Screen) -> Vec<Vec<Line<'static>>> {
    let partner = screen.rows[CENTER]
        .get(1)
        .map(|p| person_lines(p, MAX_TEXT));
    let partner_space = partner
        .as_ref()
        .map_or(0, |p| box_width(p) + MARRIAGE.width() as u16);
    let main = main_lines(screen, area.width.saturating_sub(partner_space + 4));
    std::iter::once(main).chain(partner).collect()
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

pub fn draw_family(frame: &mut Frame, area: Rect, app: &App, screen: &Screen) {
    let plan = plan(area, app, screen, app.visible);
    for row in &plan.rows {
        draw_row(frame, area, app, screen, row);
    }
    let mut lines = Connectors::default();
    for band in &plan.bands {
        lines.band(band, &plan.rows[band.upper], &plan.rows[band.lower]);
    }
    lines.render(frame);
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
            app.focus == (row.row, item.index),
        );
    }
    if row.row == CENTER
        && let Some(partner) = row.items.get(1)
    {
        let mark = Rect {
            x: partner.rect.x - MARRIAGE.width() as u16,
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
    let mut text = label(row.row, screen);
    if row.hidden_before > 0 {
        text = format!("◀ {} more · {text}", row.hidden_before);
    }
    let room = first.saturating_sub(area.x + 1);
    if !text.is_empty() && (text.width() as u16) < room {
        let rect = Rect {
            x: area.x + 1,
            y: row.y,
            width: room,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Span::styled(text, dim())), rect);
    }
    if row.hidden_after > 0 {
        let more = format!(" {} more ▶", row.hidden_after);
        let rect = Rect {
            x: last,
            y: row.y,
            width: area.right().saturating_sub(last),
            height: 1,
        };
        frame.render_widget(Paragraph::new(Span::styled(more, dim())), rect);
    }
}

fn label(row: usize, screen: &Screen) -> String {
    match row {
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
        _ => String::new(),
    }
}

#[derive(Default)]
struct Connectors {
    cells: HashMap<(u16, u16), u8>,
    joints: Vec<(u16, u16, bool)>,
}

impl Connectors {
    fn band(&mut self, band: &Band, upper: &RowPlan, lower: &RowPlan) {
        for (ups, downs, lane) in &band.lanes {
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
        *self.cells.entry((x, y)).or_default() |= flag;
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
        for (&(x, y), &flags) in &self.cells {
            if x < area.right() && y < area.bottom() {
                buffer[(x, y)].set_symbol(glyph(flags)).set_style(dim());
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
