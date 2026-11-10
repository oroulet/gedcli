use crate::card;
use crate::person::{Brief, People};

pub const GREAT_GRANDPARENTS: usize = 0;
pub const GRANDPARENTS: usize = 1;
pub const PARENTS: usize = 2;
pub const CENTER: usize = 3;
pub const CHILDREN: usize = 4;
pub const GRANDCHILDREN: usize = 5;
pub const GREAT_GRANDCHILDREN: usize = 6;
pub const ROWS: usize = 7;

pub struct Partner {
    pub family: String,
    pub person: Option<Brief>,
}

/// People in row `r` (`upper`) and their children in row `r + 1` (`lower`), by index.
pub struct Group {
    pub upper: Vec<usize>,
    pub lower: Vec<usize>,
}

pub struct Screen {
    pub rows: [Vec<Brief>; ROWS],
    pub links: [Vec<Group>; ROWS - 1],
    pub details: Vec<String>,
    pub partners: Vec<Partner>,
    pub partner_index: usize,
    pub center_pos: usize,
    pub partner_pos: Option<usize>,
    /// Upper-cased ids of the people whose photo is shown.
    pub photos: std::collections::HashSet<String>,
}

struct CenterRow {
    people: Vec<Brief>,
    center_pos: usize,
    partner_pos: Option<usize>,
    parent_links: Vec<Group>,
}

impl Screen {
    pub fn build(people: &People, center: &str, partner_index: usize) -> Self {
        let partners: Vec<Partner> = people
            .unions(center)
            .into_iter()
            .map(|u| Partner {
                person: u.partner.as_deref().map(|p| people.brief(p)),
                family: u.family,
            })
            .collect();
        let partner_index = if partners.is_empty() {
            0
        } else {
            partner_index % partners.len()
        };
        let children = current_children(people, center, partner_index);
        let parents = people.parents(center);
        let partner = partners.get(partner_index).and_then(|p| p.person.clone());
        let row = center_row(people, center, partner, &parents);
        let couple: Vec<usize> = std::iter::once(row.center_pos)
            .chain(row.partner_pos)
            .collect();
        let (grandparents, grandparent_links) = parents_row(people, &parents);
        let (great_grandparents, great_grandparent_links) =
            parents_row(people, &ids(&grandparents));
        let (grandchildren, grandchild_links) = children_row(people, &children);
        let (great_grandchildren, great_grandchild_links) =
            children_row(people, &ids(&grandchildren));
        let links = [
            great_grandparent_links,
            grandparent_links,
            row.parent_links,
            couple_group(couple, children.len()),
            grandchild_links,
            great_grandchild_links,
        ];
        Self {
            rows: [
                great_grandparents,
                grandparents,
                briefs(people, &parents),
                row.people,
                briefs(people, &children),
                grandchildren,
                great_grandchildren,
            ],
            links,
            details: details(people, center),
            partners,
            partner_index,
            center_pos: row.center_pos,
            partner_pos: row.partner_pos,
            photos: std::collections::HashSet::new(),
        }
    }

    /// The image area of a person's card, when their photo is shown.
    pub fn photo_size(&self, row: usize, index: usize) -> Option<(u16, u16)> {
        let person = self.at(row, index)?;
        if !self.photos.contains(&person.id.to_ascii_uppercase()) {
            return None;
        }
        Some(if row == CENTER && index == self.center_pos {
            super::portrait::PORTRAIT
        } else {
            super::portrait::THUMB
        })
    }

    pub fn center(&self) -> &Brief {
        &self.rows[CENTER][self.center_pos]
    }

    pub fn partner(&self) -> Option<&Partner> {
        self.partners.get(self.partner_index)
    }

    pub fn at(&self, row: usize, col: usize) -> Option<&Brief> {
        self.rows.get(row)?.get(col)
    }
}

fn briefs(people: &People, ids: &[String]) -> Vec<Brief> {
    ids.iter().map(|id| people.brief(id)).collect()
}

fn current_children(people: &People, center: &str, partner_index: usize) -> Vec<String> {
    people
        .unions(center)
        .into_iter()
        .nth(partner_index)
        .map(|u| u.children)
        .unwrap_or_default()
}

fn couple_group(upper: Vec<usize>, lower: usize) -> Vec<Group> {
    if lower == 0 {
        return Vec::new();
    }
    vec![Group {
        upper,
        lower: (0..lower).collect(),
    }]
}

/// Older siblings, the person, their partner, then younger siblings; linked to their parents by family.
fn center_row(
    people: &People,
    center: &str,
    partner: Option<Brief>,
    parents: &[String],
) -> CenterRow {
    let families = birth_families(people, center);
    let own_birth = people.birth_key(center);
    let mut siblings: Vec<(String, String)> = Vec::new();
    for family in &families {
        for child in people
            .graph
            .family(family)
            .map(|m| m.children.clone())
            .unwrap_or_default()
        {
            let known = siblings
                .iter()
                .any(|(_, id)| id.eq_ignore_ascii_case(&child));
            if !known && !child.eq_ignore_ascii_case(center) {
                siblings.push((family.clone(), child));
            }
        }
    }
    siblings.sort_by_key(|(_, id)| people.birth_key(id));
    let (older, younger): (Vec<_>, Vec<_>) = siblings
        .into_iter()
        .partition(|(_, id)| people.birth_key(id) < own_birth);
    let partner_first = partner.is_some() && wife_and_husband(people, center, partner.as_ref());
    let couple_len = 1 + usize::from(partner.is_some());
    let center_pos = older.len() + usize::from(partner_first);
    let partner_pos = partner
        .as_ref()
        .map(|_| older.len() + usize::from(!partner_first));
    let mut people_row: Vec<Brief> = older.iter().map(|(_, id)| people.brief(id)).collect();
    let mut couple = vec![people.brief(center)];
    couple.extend(partner);
    if partner_first {
        couple.reverse();
    }
    people_row.extend(couple);
    people_row.extend(younger.iter().map(|(_, id)| people.brief(id)));
    let mut ids: Vec<(Option<String>, usize)> = older
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (Some(f.clone()), i))
        .collect();
    ids.push((None, center_pos));
    ids.extend(
        younger
            .iter()
            .enumerate()
            .map(|(j, (f, _))| (Some(f.clone()), older.len() + couple_len + j)),
    );
    let parent_links = families
        .iter()
        .filter_map(|family| {
            let members = people.graph.family(family)?;
            let upper: Vec<usize> = parents
                .iter()
                .enumerate()
                .filter(|(_, p)| members.partners.iter().any(|m| m.eq_ignore_ascii_case(p)))
                .map(|(i, _)| i)
                .collect();
            let lower: Vec<usize> = ids
                .iter()
                .filter(|(f, _)| f.as_deref().is_none_or(|f| f == family))
                .map(|(_, position)| *position)
                .collect();
            (!upper.is_empty()).then_some(Group { upper, lower })
        })
        .collect();
    CenterRow {
        people: people_row,
        center_pos,
        partner_pos,
        parent_links,
    }
}

/// Whether the centre person is a woman whose partner is not: then the husband is drawn on her left.
fn wife_and_husband(people: &People, center: &str, partner: Option<&Brief>) -> bool {
    let partner_sex = partner.and_then(|p| p.sex.as_deref());
    people.sex(center).as_deref() == Some("F") && partner_sex != Some("F")
}

fn birth_families(people: &People, person: &str) -> Vec<String> {
    let mut families: Vec<String> = people
        .record(person)
        .map(|r| {
            r.all("FAMC")
                .filter_map(|n| n.pointer().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut extra: Vec<String> = people
        .graph
        .families()
        .iter()
        .filter(|(_, m)| m.children.iter().any(|c| c.eq_ignore_ascii_case(person)))
        .map(|(key, _)| key.clone())
        .filter(|key| !families.iter().any(|f| f.eq_ignore_ascii_case(key)))
        .collect();
    extra.sort();
    families.extend(extra);
    families
}

fn ids(row: &[Brief]) -> Vec<String> {
    row.iter().map(|b| b.id.clone()).collect()
}

/// The parents of each person in a row, linked to their child.
fn parents_row(people: &People, parents: &[String]) -> (Vec<Brief>, Vec<Group>) {
    let mut row = Vec::new();
    let mut links = Vec::new();
    for (index, parent) in parents.iter().enumerate() {
        let theirs = people.parents(parent);
        let upper = (row.len()..row.len() + theirs.len()).collect();
        row.extend(theirs.iter().map(|id| people.brief(id)));
        if !theirs.is_empty() {
            links.push(Group {
                upper,
                lower: vec![index],
            });
        }
    }
    (row, links)
}

/// The children of each person in a row, linked to their parent.
fn children_row(people: &People, children: &[String]) -> (Vec<Brief>, Vec<Group>) {
    let mut ids: Vec<String> = Vec::new();
    let mut links = Vec::new();
    for (index, child) in children.iter().enumerate() {
        let mut theirs = people.graph.children_of(child).to_vec();
        theirs.sort_by_key(|c| people.birth_key(c));
        let mut lower = Vec::new();
        for id in theirs {
            if !ids.iter().any(|known| known.eq_ignore_ascii_case(&id)) {
                lower.push(ids.len());
                ids.push(id);
            }
        }
        if !lower.is_empty() {
            links.push(Group {
                upper: vec![index],
                lower,
            });
        }
    }
    (briefs(people, &ids), links)
}

const NOTE_LINES: usize = 3;
const GENI_NOTE_PREFIX: &str = "{geni:about_me}";

fn details(people: &People, center: &str) -> Vec<String> {
    let Some(record) = people.record(center) else {
        return Vec::new();
    };
    let card = card::build(people, record);
    let (occupations, events): (Vec<_>, Vec<_>) =
        card.events.iter().partition(|e| e.label == "Occupation");
    let mut lines: Vec<String> = occupations
        .into_iter()
        .chain(events)
        .map(event_line)
        .collect();
    for note in &card.notes {
        lines.extend(note_lines(note));
    }
    lines.extend(
        card.contacts
            .iter()
            .map(|c| format!("{:<11}{c}", "Contact")),
    );
    lines
}

fn event_line(event: &card::Event) -> String {
    let text: Vec<&str> = [&event.value, &event.date, &event.place, &event.cause]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect();
    format!("{:<11}{}", event.label, text.join(" · "))
}

fn note_lines(note: &str) -> Vec<String> {
    let text = note.trim().trim_start_matches(GENI_NOTE_PREFIX).trim();
    let all: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut lines: Vec<String> = all
        .iter()
        .take(NOTE_LINES)
        .enumerate()
        .map(|(i, line)| format!("{:<11}{line}", if i == 0 { "Note" } else { "" }))
        .collect();
    if all.len() > NOTE_LINES {
        lines.push(format!(
            "{:<11}… {} more lines (s shows all)",
            "",
            all.len() - NOTE_LINES
        ));
    }
    lines
}
