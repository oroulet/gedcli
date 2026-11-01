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
        let mut center_row = vec![people.brief(center)];
        center_row.extend(partners.get(partner_index).and_then(|p| p.person.clone()));
        let (grandparents, grandparent_links) = grandparents(people, &parents);
        let (grandchildren, grandchild_links) = grandchildren(people, &children);
        let links = [
            grandparent_links,
            one_group(parents.len(), 1),
            one_group(center_row.len(), children.len()),
            grandchild_links,
        ];
        Self {
            rows: [
                grandparents,
                briefs(people, &parents),
                center_row,
                briefs(people, &children),
                grandchildren,
            ],
            links,
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

fn one_group(upper: usize, lower: usize) -> Vec<Group> {
    if upper == 0 || lower == 0 {
        return Vec::new();
    }
    vec![Group {
        upper: (0..upper).collect(),
        lower: (0..lower).collect(),
    }]
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
    lines
}
