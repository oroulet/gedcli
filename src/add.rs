use anyhow::{Result, bail};
use clap::ValueEnum;
use gedcomkit::{Document, Node};

use crate::edit;
use crate::person::{Brief, People};
use crate::search::normalize;

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Relation {
    Son,
    Daughter,
    Child,
    Father,
    Mother,
    Parent,
    Husband,
    Wife,
    Partner,
    Brother,
    Sister,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Child,
    Parent,
    Partner,
    Sibling,
}

impl Relation {
    fn kind(self) -> Kind {
        match self {
            Self::Son | Self::Daughter | Self::Child => Kind::Child,
            Self::Father | Self::Mother | Self::Parent => Kind::Parent,
            Self::Husband | Self::Wife | Self::Partner => Kind::Partner,
            Self::Brother | Self::Sister => Kind::Sibling,
        }
    }

    fn sex(self) -> Option<&'static str> {
        match self {
            Self::Son | Self::Father | Self::Husband | Self::Brother => Some("M"),
            Self::Daughter | Self::Mother | Self::Wife | Self::Sister => Some("F"),
            Self::Child | Self::Parent | Self::Partner => None,
        }
    }

    fn word(self) -> String {
        self.to_possible_value()
            .map_or_else(|| "relative".to_owned(), |v| v.get_name().to_owned())
    }
}

pub struct NewPerson {
    pub given: String,
    pub surname: Option<String>,
    pub birth: Option<String>,
    pub death: Option<String>,
}

pub struct Plan {
    relation: Relation,
    anchor: String,
    anchor_sex: Option<String>,
    family: Option<String>,
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn new_person(
    given: &str,
    surname: &str,
    birth: Option<&str>,
    death: Option<&str>,
    v7: bool,
) -> Result<NewPerson> {
    let clean = |text: &str| -> Result<Option<String>> {
        let text = text.trim();
        if text.contains('/') {
            bail!("a name part cannot contain '/'");
        }
        Ok(Some(text.to_owned()).filter(|t| !t.is_empty() && t != "-"))
    };
    let date = |value: Option<&str>| -> Result<Option<String>> {
        value
            .map(str::trim)
            .filter(|v| !v.is_empty() && *v != "-")
            .map(|v| edit::normalize_date(v, v7))
            .transpose()
    };
    let Some(given) = clean(given)? else {
        bail!("a first name is required")
    };
    Ok(NewPerson {
        given,
        surname: clean(surname)?,
        birth: date(birth)?,
        death: date(death)?,
    })
}

/// `families` are the anchor's families as a partner: (family, other partner).
pub fn plan(
    people: &People,
    anchor: &Brief,
    relation: Relation,
    person: &NewPerson,
    family: Option<String>,
) -> Result<Plan> {
    let mut plan = Plan {
        relation,
        anchor: anchor.id.clone(),
        anchor_sex: people.sex(&anchor.id),
        family,
        lines: Vec::new(),
        warnings: Vec::new(),
    };
    let name = display(person);
    let birth_family = people
        .record(&anchor.id)
        .and_then(|r| r.first("FAMC"))
        .and_then(Node::pointer)
        .map(str::to_owned);
    match relation.kind() {
        Kind::Child => {
            plan.lines.push(match &plan.family {
                Some(f) => format!(
                    "add {name} as {} of {} in family {f}",
                    relation.word(),
                    anchor.name
                ),
                None => format!(
                    "add {name} as {} of {} in a new family",
                    relation.word(),
                    anchor.name
                ),
            });
            same_name(
                people,
                people.graph.children_of(&anchor.id),
                person,
                &mut plan.warnings,
            );
        }
        Kind::Parent => {
            let parents = people.parents(&anchor.id);
            let slot_taken = parents
                .iter()
                .find(|p| relation.sex().is_some() && people.sex(p).as_deref() == relation.sex());
            if let Some(existing) = slot_taken {
                bail!(
                    "{} already has a {}: {}",
                    anchor.name,
                    relation.word(),
                    people.brief(existing)
                );
            }
            if parents.len() >= 2 {
                bail!("{} already has two parents", anchor.name);
            }
            plan.family = birth_family;
            plan.lines.push(format!(
                "add {name} as {} of {}",
                relation.word(),
                anchor.name
            ));
        }
        Kind::Partner => {
            plan.family = None;
            plan.lines.push(format!(
                "add {name} as {} of {} in a new family",
                relation.word(),
                anchor.name
            ));
            same_name(
                people,
                people.graph.partners_of(&anchor.id),
                person,
                &mut plan.warnings,
            );
        }
        Kind::Sibling => {
            plan.family = birth_family;
            let siblings: Vec<String> = plan
                .family
                .as_deref()
                .and_then(|f| people.graph.family(f))
                .map(|m| m.children.clone())
                .unwrap_or_default();
            same_name(people, &siblings, person, &mut plan.warnings);
            plan.lines.push(format!(
                "add {name} as {} of {}",
                relation.word(),
                anchor.name
            ));
        }
    }
    for (label, value) in [("born", &person.birth), ("died", &person.death)] {
        if let Some(value) = value {
            plan.lines.push(format!("  {label} {value}"));
        }
    }
    Ok(plan)
}

fn same_name(
    people: &People,
    relatives: &[String],
    person: &NewPerson,
    warnings: &mut Vec<String>,
) {
    let wanted = normalize(&person.given);
    for relative in relatives {
        let brief = people.brief(relative);
        if normalize(&brief.name).split_whitespace().next() == wanted.split_whitespace().next() {
            warnings.push(format!("already there? {brief}"));
        }
    }
}

fn display(person: &NewPerson) -> String {
    match &person.surname {
        Some(surname) => format!("{} {surname}", person.given),
        None => person.given.clone(),
    }
}

pub fn apply(doc: &mut Document, plan: &Plan, person: &NewPerson) -> Result<String> {
    let xref = doc.next_xref("IG");
    insert_record(
        doc,
        "INDI",
        person_record(&xref, plan.relation.sex(), person),
    );
    let anchor = plan.anchor.clone();
    match plan.relation.kind() {
        Kind::Child => {
            let family = match &plan.family {
                Some(f) => f.clone(),
                None => new_family(doc, &[(&anchor, plan.anchor_sex.as_deref())], &[]),
            };
            join_as_child(doc, &family, &xref);
        }
        Kind::Parent => {
            let family = match &plan.family {
                Some(f) => f.clone(),
                None => {
                    let f = new_family(doc, &[], &[]);
                    join_as_child(doc, &f, &anchor);
                    f
                }
            };
            join_as_partner(doc, &family, &xref, plan.relation.sex());
        }
        Kind::Partner => {
            new_family(
                doc,
                &[
                    (&anchor, plan.anchor_sex.as_deref()),
                    (&xref, plan.relation.sex()),
                ],
                &[],
            );
        }
        Kind::Sibling => {
            let family = match &plan.family {
                Some(f) => f.clone(),
                None => {
                    let f = new_family(doc, &[], &[]);
                    join_as_child(doc, &f, &anchor);
                    f
                }
            };
            join_as_child(doc, &family, &xref);
        }
    }
    if let Some(record) = doc.record_mut(&anchor) {
        edit::touch(record);
    }
    Ok(xref)
}

fn person_record(xref: &str, sex: Option<&str>, person: &NewPerson) -> Node {
    let payload = match &person.surname {
        Some(surname) => format!("{} /{surname}/", person.given),
        None => person.given.clone(),
    };
    let mut name = Node::with_value("NAME", payload).child(Node::with_value("GIVN", &person.given));
    if let Some(surname) = &person.surname {
        name = name.child(Node::with_value("SURN", surname));
    }
    let mut record = Node::record(xref, "INDI").child(name);
    if let Some(sex) = sex {
        record = record.child(Node::with_value("SEX", sex));
    }
    for (tag, date) in [("BIRT", &person.birth), ("DEAT", &person.death)] {
        if let Some(date) = date {
            record = record.child(Node::new(tag).child(Node::with_value("DATE", date)));
        }
    }
    edit::touch(&mut record);
    record
}

pub(crate) fn new_family(
    doc: &mut Document,
    partners: &[(&str, Option<&str>)],
    children: &[&str],
) -> String {
    let xref = doc.next_xref("FG");
    let mut family = Node::record(&xref, "FAM");
    edit::touch(&mut family);
    insert_record(doc, "FAM", family);
    for (partner, sex) in partners {
        join_as_partner(doc, &xref, partner, *sex);
    }
    for child in children {
        join_as_child(doc, &xref, child);
    }
    xref
}

fn join_as_child(doc: &mut Document, family: &str, child: &str) {
    if let Some(record) = doc.record_mut(family) {
        let at = after_last(record, &["HUSB", "WIFE", "CHIL"]).unwrap_or(0);
        record.children.insert(at, Node::with_value("CHIL", child));
    }
    if let Some(record) = doc.record_mut(child) {
        insert_pointer(record, "FAMC", family);
    }
}

fn join_as_partner(doc: &mut Document, family: &str, partner: &str, sex: Option<&str>) {
    if let Some(record) = doc.record_mut(family) {
        let has = |tag: &str| record.first(tag).is_some();
        let tag = match sex {
            Some("F") if !has("WIFE") => "WIFE",
            Some("M") if !has("HUSB") => "HUSB",
            _ if !has("HUSB") => "HUSB",
            _ => "WIFE",
        };
        let at = if tag == "HUSB" {
            0
        } else {
            after_last(record, &["HUSB"]).unwrap_or(0)
        };
        record.children.insert(at, Node::with_value(tag, partner));
    }
    if let Some(record) = doc.record_mut(partner) {
        insert_pointer(record, "FAMS", family);
    }
}

fn insert_pointer(record: &mut Node, tag: &str, target: &str) {
    let at = after_last(record, &["FAMC", "FAMS"]).unwrap_or_else(|| edit::before_tail(record));
    record.children.insert(at, Node::with_value(tag, target));
}

fn after_last(record: &Node, tags: &[&str]) -> Option<usize> {
    record
        .children
        .iter()
        .rposition(|n| tags.contains(&n.tag()))
        .map(|i| i + 1)
}

pub(crate) fn insert_record(doc: &mut Document, tag: &str, record: Node) {
    let at = doc
        .records
        .iter()
        .rposition(|r| r.tag() == tag)
        .map(|i| i + 1)
        .or_else(|| doc.records.iter().position(|r| r.tag() == "TRLR"))
        .unwrap_or(doc.records.len());
    doc.records.insert(at, record);
}
