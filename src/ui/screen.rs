use crate::card;
use crate::person::{Brief, People};

pub const GRANDPARENTS: usize = 0;
pub const PARENTS: usize = 1;
pub const CENTER: usize = 2;
pub const CHILDREN: usize = 3;
pub const GRANDCHILDREN: usize = 4;
pub const ROWS: usize = 5;

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
        let (grandparents, grandparent_links) = grandparents(people, &parents);
        let (grandchildren, grandchild_links) = grandchildren(people, &children);
        let links = [
            grandparent_links,
            row.parent_links,
            couple_group(couple, children.len()),
            grandchild_links,
        ];
        Self {
            rows: [
                grandparents,
                briefs(people, &parents),
                row.people,
                briefs(people, &children),
                grandchildren,
            ],
            links,
            details: details(people, center),
            partners,
            partner_index,
            center_pos: row.center_pos,
            partner_pos: row.partner_pos,
        }
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
    let mut ids: Vec<(Option<String>, String)> =
        older.into_iter().map(|(f, id)| (Some(f), id)).collect();
    let center_pos = ids.len();
    ids.push((None, center.to_owned()));
    let mut people_row: Vec<Brief> = Vec::new();
    let partner_pos = partner.as_ref().map(|_| center_pos + 1);
    ids.extend(younger.into_iter().map(|(f, id)| (Some(f), id)));
    for (index, (_, id)) in ids.iter().enumerate() {
        people_row.push(people.brief(id));
        if index == center_pos {
            people_row.extend(partner.clone());
        }
    }
    let position = |i: usize| {
        if partner_pos.is_some() && i > center_pos {
            i + 1
        } else {
            i
        }
    };
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
                .enumerate()
                .filter(|(_, (f, _))| f.as_deref().is_none_or(|f| f == family))
                .map(|(i, _)| position(i))
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

fn grandparents(people: &People, parents: &[String]) -> (Vec<Brief>, Vec<Group>) {
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

fn grandchildren(people: &People, children: &[String]) -> (Vec<Brief>, Vec<Group>) {
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

fn details(people: &People, center: &str) -> Vec<String> {
    let Some(record) = people.record(center) else {
        return Vec::new();
    };
    let card = card::build(people, record);
    let mut lines: Vec<String> = card
        .events
        .iter()
        .map(|e| {
            let text: Vec<&str> = [&e.value, &e.date, &e.place]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect();
            format!("{:<11}{}", e.label, text.join(" · "))
        })
        .collect();
    lines.extend(
        card.notes
            .iter()
            .filter_map(|n| n.lines().next())
            .map(|n| format!("{:<11}{n}", "Note")),
    );
    lines.extend(
        card.contacts
            .iter()
            .map(|c| format!("{:<11}{c}", "Contact")),
    );
    lines.extend(card.added_by.map(|by| format!("{:<11}{by}", "Added by")));
    lines
}
