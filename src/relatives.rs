use std::collections::HashSet;

use serde::Serialize;

use crate::person::{Brief, People, child_word, parent_word};

#[derive(Serialize)]
pub struct Rel {
    #[serde(flatten)]
    pub person: Brief,
    pub relation: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub repeat: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub families: Vec<Family>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<Rel>,
}

#[derive(Serialize)]
pub struct Family {
    pub partner: Option<Brief>,
    pub children: Vec<Rel>,
}

impl Rel {
    fn new(person: Brief, relation: &str) -> Self {
        Self {
            person,
            relation: relation.to_owned(),
            repeat: false,
            families: Vec::new(),
            parents: Vec::new(),
        }
    }
}

pub fn descendants(people: &People, root: &str, generations: u32) -> Rel {
    descend(people, root, "self", generations, &mut HashSet::new())
}

pub fn ancestors(people: &People, root: &str, generations: u32) -> Rel {
    ascend(people, root, "self", generations, &mut HashSet::new())
}

fn descend(
    people: &People,
    xref: &str,
    relation: &str,
    left: u32,
    seen: &mut HashSet<String>,
) -> Rel {
    let mut rel = Rel::new(people.brief(xref), relation);
    if !seen.insert(xref.to_ascii_uppercase()) {
        rel.repeat = true;
        return rel;
    }
    if left == 0 {
        return rel;
    }
    for union in people.unions(xref) {
        let children = union
            .children
            .iter()
            .map(|c| {
                descend(
                    people,
                    c,
                    child_word(people.sex(c).as_deref()),
                    left - 1,
                    seen,
                )
            })
            .collect();
        rel.families.push(Family {
            partner: union.partner.as_deref().map(|p| people.brief(p)),
            children,
        });
    }
    rel
}

fn ascend(
    people: &People,
    xref: &str,
    relation: &str,
    left: u32,
    seen: &mut HashSet<String>,
) -> Rel {
    let mut rel = Rel::new(people.brief(xref), relation);
    if !seen.insert(xref.to_ascii_uppercase()) {
        rel.repeat = true;
        return rel;
    }
    if left == 0 {
        return rel;
    }
    rel.parents = people
        .parents(xref)
        .iter()
        .map(|p| {
            ascend(
                people,
                p,
                parent_word(people.sex(p).as_deref()),
                left - 1,
                seen,
            )
        })
        .collect();
    rel
}
