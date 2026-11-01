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

pub struct Screen {
    pub rows: [Vec<Brief>; ROWS],
    pub details: Vec<String>,
    pub partners: Vec<Partner>,
    pub partner_index: usize,
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
        let mut center_row = vec![people.brief(center)];
        center_row.extend(partners.get(partner_index).and_then(|p| p.person.clone()));
        Self {
            rows: [
                grandparents(people, center),
                briefs(people, &people.parents(center)),
                center_row,
                briefs(people, &children),
                grandchildren(people, &children),
            ],
            details: details(people, center),
            partners,
            partner_index,
        }
    }

    pub fn center(&self) -> &Brief {
        &self.rows[CENTER][0]
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

fn grandparents(people: &People, center: &str) -> Vec<Brief> {
    people
        .parents(center)
        .iter()
        .flat_map(|parent| people.parents(parent))
        .map(|id| people.brief(&id))
        .collect()
}

fn grandchildren(people: &People, children: &[String]) -> Vec<Brief> {
    let mut ids: Vec<String> = Vec::new();
    for child in children {
        let mut theirs = people.graph.children_of(child).to_vec();
        theirs.sort_by_key(|c| people.birth_key(c));
        for id in theirs {
            if !ids.iter().any(|known| known.eq_ignore_ascii_case(&id)) {
                ids.push(id);
            }
        }
    }
    briefs(people, &ids)
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
    lines
}
