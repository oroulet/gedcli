use unicode_width::UnicodeWidthChar;

use crate::relatives::Rel;

const GAP: usize = 2;
const WRAP: usize = 18;

struct Box {
    lines: Vec<String>,
    children: Vec<Box>,
}

struct Placed {
    depth: usize,
    center: usize,
    left: usize,
    lines: Vec<String>,
    child_centers: Vec<usize>,
}

pub fn descendants(rel: &Rel) -> String {
    to_text(draw(&descendant_box(rel)))
}

pub fn ancestors(rel: &Rel) -> String {
    let mut rows = draw(&reverse_labels(ancestor_box(rel)));
    rows.reverse();
    for row in &mut rows {
        for cell in row.iter_mut() {
            if let [c] = cell.chars().collect::<Vec<_>>()[..] {
                *cell = flip(c).to_string();
            }
        }
    }
    to_text(rows)
}

fn reverse_labels(mut node: Box) -> Box {
    node.lines.reverse();
    node.children = node.children.into_iter().map(reverse_labels).collect();
    node
}

fn flip(c: char) -> char {
    match c {
        '┌' => '└',
        '└' => '┌',
        '┐' => '┘',
        '┘' => '┐',
        '┬' => '┴',
        '┴' => '┬',
        other => other,
    }
}

type Grid = Vec<Vec<String>>;

fn to_text(rows: Grid) -> String {
    rows.into_iter()
        .map(|row| row.concat().trim_end().to_owned() + "\n")
        .collect()
}

fn blank(height: usize, width: usize) -> Grid {
    vec![vec![" ".to_owned(); width]; height]
}

fn person_lines(rel: &Rel) -> Vec<String> {
    let mark = if rel.repeat { "↺ " } else { "" };
    let mut lines = wrap(&format!("{mark}{}", rel.person.name));
    lines.extend(rel.person.life.clone());
    lines.extend(rel.person.info.as_deref().map(wrap).unwrap_or_default());
    lines
}

fn descendant_box(rel: &Rel) -> Box {
    let mut lines = person_lines(rel);
    let mut children = Vec::new();
    if let [only] = rel.families.as_slice() {
        if let Some(partner) = &only.partner {
            lines.extend(wrap(&format!("⚭ {}", partner.name)));
        }
        children = only.children.iter().map(descendant_box).collect();
    } else {
        for family in &rel.families {
            let name = family
                .partner
                .as_ref()
                .map_or("unknown", |p| p.name.as_str());
            children.push(Box {
                lines: wrap(&format!("⚭ {name}")),
                children: family.children.iter().map(descendant_box).collect(),
            });
        }
    }
    Box { lines, children }
}

fn ancestor_box(rel: &Rel) -> Box {
    Box {
        lines: person_lines(rel),
        children: rel.parents.iter().map(ancestor_box).collect(),
    }
}

fn wrap(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if width(line) + 1 + width(word) <= WRAP => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

fn width(text: &str) -> usize {
    text.chars().map(columns).sum()
}

fn columns(c: char) -> usize {
    c.width().unwrap_or(1)
}

fn label_width(node: &Box) -> usize {
    node.lines.iter().map(|l| width(l)).max().unwrap_or(1)
}

fn subtree_width(node: &Box) -> usize {
    let kids: usize = node.children.iter().map(subtree_width).sum::<usize>()
        + GAP * node.children.len().saturating_sub(1);
    label_width(node).max(kids)
}

fn place(node: &Box, left: usize, depth: usize, out: &mut Vec<Placed>) -> usize {
    let total = subtree_width(node);
    let kids_width: usize = node.children.iter().map(subtree_width).sum::<usize>()
        + GAP * node.children.len().saturating_sub(1);
    let mut x = left + (total - kids_width.min(total)) / 2;
    let mut child_centers = Vec::new();
    for child in &node.children {
        child_centers.push(place(child, x, depth + 1, out));
        x += subtree_width(child) + GAP;
    }
    let label = label_width(node);
    let wanted = match (child_centers.first(), child_centers.last()) {
        (Some(first), Some(last)) => (first + last) / 2,
        _ => left + total / 2,
    };
    let label_left = wanted
        .saturating_sub(label / 2)
        .clamp(left, left + total - label);
    let center = wanted.clamp(label_left, label_left + label - 1);
    out.push(Placed {
        depth,
        center,
        left: label_left,
        lines: node.lines.clone(),
        child_centers,
    });
    center
}

fn draw(root: &Box) -> Grid {
    let mut placed = Vec::new();
    place(root, 0, 0, &mut placed);
    let total = subtree_width(root);
    let depths = placed.iter().map(|p| p.depth).max().unwrap_or(0) + 1;
    let mut rows = Vec::new();
    for depth in 0..depths {
        let level: Vec<&Placed> = placed.iter().filter(|p| p.depth == depth).collect();
        let height = level.iter().map(|p| p.lines.len()).max().unwrap_or(1);
        let mut labels = blank(height, total);
        let mut links = blank(2, total);
        for p in &level {
            write_label(&mut labels, p);
            if !p.child_centers.is_empty() {
                for row in &mut labels[p.lines.len()..] {
                    row[p.center] = "│".to_owned();
                }
                connect(&mut links, p);
            }
        }
        rows.extend(labels);
        if depth + 1 < depths {
            rows.extend(links);
        }
    }
    rows
}

fn write_label(rows: &mut [Vec<String>], p: &Placed) {
    let label = label_width_of(&p.lines);
    for (row, line) in p.lines.iter().enumerate() {
        let start = p.left + (label - width(line)) / 2;
        write_line(&mut rows[row], start, line);
    }
}

fn write_line(row: &mut [String], start: usize, line: &str) {
    let mut col = start;
    for c in line.chars() {
        match columns(c) {
            0 if col > start => row[col - 1].push(c),
            0 => row[col].push(c),
            w => {
                row[col] = c.to_string();
                for cell in &mut row[col + 1..col + w] {
                    cell.clear();
                }
                col += w;
            }
        }
    }
}

fn label_width_of(lines: &[String]) -> usize {
    lines.iter().map(|l| width(l)).max().unwrap_or(1)
}

fn connect(links: &mut [Vec<String>], p: &Placed) {
    let (Some(&first), Some(&last)) = (p.child_centers.first(), p.child_centers.last()) else {
        return;
    };
    links[0][p.center] = "│".to_owned();
    let lo = first.min(p.center);
    let hi = last.max(p.center);
    for (x, cell) in links[1].iter_mut().enumerate().take(hi + 1).skip(lo) {
        let up = x == p.center;
        let down = p.child_centers.contains(&x);
        *cell = glyph(up, down, x > lo, x < hi).to_string();
    }
}

fn glyph(up: bool, down: bool, left: bool, right: bool) -> char {
    match (up, down, left, right) {
        (true, true, false, false) => '│',
        (false, true, false, true) => '┌',
        (false, true, true, false) => '┐',
        (true, false, false, true) => '└',
        (true, false, true, false) => '┘',
        (false, true, true, true) => '┬',
        (true, false, true, true) => '┴',
        (true, true, false, true) => '├',
        (true, true, true, false) => '┤',
        (true, true, true, true) => '┼',
        _ => '─',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_and_combining_letters_keep_columns_aligned() {
        assert_eq!(width("王小明"), 6);
        assert_eq!(width("A\u{30a}se"), 3);
        let mut row = vec![" ".to_owned(); 8];
        write_line(&mut row, 1, "王A\u{30a}");
        assert_eq!(row.concat(), " 王A\u{30a}    ");
        assert_eq!(width(&row.concat()), 8);
    }

    #[test]
    fn wraps_long_names() {
        assert_eq!(
            wrap("Kari Eugénie Lund-Berg"),
            ["Kari Eugénie", "Lund-Berg"]
        );
    }
}
