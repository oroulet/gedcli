use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::card::{self, Card};
use crate::person::{Brief, People, brief_of};
use crate::store::Store;

const SHOWN_TEXT: usize = 90;

#[derive(Serialize)]
pub struct Comparison {
    pub ours: String,
    pub theirs: String,
    pub only_theirs: Vec<NewPerson>,
    pub only_ours: Vec<Brief>,
    pub changed: Vec<Changed>,
}

#[derive(Serialize)]
pub struct NewPerson {
    #[serde(flatten)]
    pub person: Brief,
    pub connections: Vec<String>,
}

#[derive(Serialize)]
pub struct Changed {
    #[serde(flatten)]
    pub person: Brief,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

pub fn compare(ours: &Store, theirs: &Store) -> Comparison {
    let a = People::new(&ours.doc);
    let b = People::new(&theirs.doc);
    let ours_ids = ids(&a);
    let theirs_ids = ids(&b);
    let mut changed = Vec::new();
    for id in ours_ids.keys().filter(|id| theirs_ids.contains_key(*id)) {
        let before = facts(&card::build(&a, a.record(id).expect("indexed")));
        let after = facts(&card::build(&b, b.record(id).expect("indexed")));
        let added: Vec<String> = only_in(&after, &before);
        let removed: Vec<String> = only_in(&before, &after);
        if !added.is_empty() || !removed.is_empty() {
            changed.push(Changed {
                person: b.brief(id),
                added,
                removed,
            });
        }
    }
    Comparison {
        ours: format!("{} (GEDCOM {})", ours.path.display(), ours.version()),
        theirs: format!("{} (GEDCOM {})", theirs.path.display(), theirs.version()),
        only_theirs: theirs_ids
            .keys()
            .filter(|id| !ours_ids.contains_key(*id))
            .map(|id| new_person(&b, id))
            .collect(),
        only_ours: ours_ids
            .keys()
            .filter(|id| !theirs_ids.contains_key(*id))
            .map(|id| a.brief(id))
            .collect(),
        changed,
    }
}

fn ids(people: &People) -> std::collections::BTreeMap<String, ()> {
    people
        .individuals()
        .filter_map(|r| r.xref().map(|x| (x.to_ascii_uppercase(), ())))
        .collect()
}

fn only_in(these: &Facts, those: &Facts) -> Vec<String> {
    these
        .iter()
        .filter(|(key, _)| !those.contains_key(*key))
        .map(|(_, shown)| shown.clone())
        .collect()
}

fn new_person(people: &People, id: &str) -> NewPerson {
    let record = people.record(id).expect("indexed");
    let mut connections = Vec::new();
    for parent in people.parents(id) {
        connections.push(format!("child of {}", people.brief(&parent)));
    }
    for partner in people.graph.partners_of(id) {
        connections.push(format!("partner of {}", people.brief(partner)));
    }
    for child in people.graph.children_of(id) {
        connections.push(format!("parent of {}", people.brief(child)));
    }
    NewPerson {
        person: brief_of(record),
        connections,
    }
}

type Facts = BTreeMap<String, String>;

trait Insert {
    fn insert(&mut self, fact: String);
    fn relative(&mut self, relation: &str, person: &Brief);
}

impl Insert for Facts {
    fn insert(&mut self, fact: String) {
        BTreeMap::insert(self, fact.clone(), fact);
    }

    fn relative(&mut self, relation: &str, person: &Brief) {
        BTreeMap::insert(
            self,
            format!("{relation}: {}", person.id),
            format!("{relation}: {person}"),
        );
    }
}

fn facts(card: &Card) -> Facts {
    let mut facts = Facts::new();
    if let Some(sex) = &card.sex {
        Insert::insert(&mut facts, format!("sex: {sex}"));
    }
    for name in &card.names {
        Insert::insert(&mut facts, format!("name: {name}"));
    }
    for e in &card.events {
        let parts: Vec<&str> = [&e.value, &e.date, &e.place]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        Insert::insert(
            &mut facts,
            format!("{}: {}", e.label.to_lowercase(), parts.join(" · ")),
        );
    }
    for c in &card.contacts {
        Insert::insert(&mut facts, format!("contact: {c}"));
    }
    for n in &card.notes {
        Insert::insert(&mut facts, format!("note: {}", n.trim()));
    }
    for m in &card.media {
        Insert::insert(
            &mut facts,
            format!("media: {}", m.split('?').next().unwrap_or(m)),
        );
    }
    for p in &card.parents {
        facts.relative(&p.relation, &p.person);
    }
    for f in &card.families {
        if let Some(partner) = &f.partner {
            facts.relative("partner", partner);
        }
        for e in &f.events {
            let parts: Vec<&str> = [&e.value, &e.date, &e.place]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect();
            Insert::insert(
                &mut facts,
                format!("family {}: {}", e.label.to_lowercase(), parts.join(" · ")),
            );
        }
        for c in &f.children {
            facts.relative(&c.relation, &c.person);
        }
    }
    facts
}

pub fn render(c: &Comparison) -> String {
    let mut out = format!("ours:   {}\ntheirs: {}\n", c.ours, c.theirs);
    let _ = writeln!(out, "\nOnly in theirs: {} people", c.only_theirs.len());
    for n in &c.only_theirs {
        let _ = writeln!(out, "  + {}", n.person);
        for link in &n.connections {
            let _ = writeln!(out, "      {link}");
        }
    }
    let _ = writeln!(out, "\nOnly in ours: {} people", c.only_ours.len());
    for p in &c.only_ours {
        let _ = writeln!(out, "  - {p}");
    }
    let _ = writeln!(out, "\nChanged: {} people", c.changed.len());
    for ch in &c.changed {
        let _ = writeln!(out, "  {}", ch.person);
        for fact in &ch.removed {
            let _ = writeln!(out, "      - {}", short(fact));
        }
        for fact in &ch.added {
            let _ = writeln!(out, "      + {}", short(fact));
        }
    }
    out
}

pub fn list(c: &Comparison) -> String {
    let mut out = String::from("# change\tid\tname\tdetail\n");
    for n in &c.only_theirs {
        let _ = writeln!(
            out,
            "new\t{}\t{}\t{}",
            n.person.id,
            n.person.name,
            n.connections.join("; ")
        );
    }
    for p in &c.only_ours {
        let _ = writeln!(out, "gone\t{}\t{}\t", p.id, p.name);
    }
    for ch in &c.changed {
        for fact in &ch.removed {
            let _ = writeln!(
                out,
                "removed\t{}\t{}\t{}",
                ch.person.id,
                ch.person.name,
                fact.replace('\n', " ")
            );
        }
        for fact in &ch.added {
            let _ = writeln!(
                out,
                "added\t{}\t{}\t{}",
                ch.person.id,
                ch.person.name,
                fact.replace('\n', " ")
            );
        }
    }
    out
}

fn short(fact: &str) -> String {
    let flat = fact.replace('\n', " ");
    if flat.chars().count() <= SHOWN_TEXT {
        return flat;
    }
    let cut: String = flat.chars().take(SHOWN_TEXT).collect();
    format!("{cut}…")
}
