use std::fmt::Write as _;

use gedcomkit::dates::{GedcomDate, changed};
use gedcomkit::view::{FamilyView, IndividualView};
use gedcomkit::{Node, tags};
use serde::Serialize;

use crate::person::{Brief, People, brief_of, child_word, parent_word};
use crate::submitters;

const ID_TAGS: [&str; 5] = ["RFN", "EXID", "REFN", "RIN", "_UID"];

#[derive(Serialize)]
pub struct Card {
    #[serde(flatten)]
    pub person: Brief,
    pub sex: Option<String>,
    pub names: Vec<String>,
    pub events: Vec<Event>,
    pub contacts: Vec<String>,
    pub notes: Vec<String>,
    pub media: Vec<String>,
    pub parents: Vec<Relative>,
    pub families: Vec<FamilyCard>,
    pub ids: Vec<String>,
    pub added_by: Option<String>,
    pub changed: Option<String>,
}

#[derive(Serialize)]
pub struct Event {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

#[derive(Serialize)]
pub struct Relative {
    pub relation: String,
    #[serde(flatten)]
    pub person: Brief,
}

#[derive(Serialize)]
pub struct FamilyCard {
    pub id: String,
    pub partner: Option<Brief>,
    pub events: Vec<Event>,
    pub children: Vec<Relative>,
}

pub fn build(people: &People, record: &Node) -> Card {
    let view = IndividualView::from_node(record);
    Card {
        person: brief_of(record),
        sex: view.sex.clone(),
        names: names(&view),
        events: events_of(record),
        contacts: contacts(record),
        notes: notes(people, record),
        media: media(record),
        parents: parents(people, &view.xref),
        families: families(people, &view.xref),
        ids: ids(record),
        added_by: added_by(people, record),
        changed: changed(record).map(|t| t.display()),
    }
}

fn names(view: &IndividualView) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for name in &view.names {
        let text = match &name.kind {
            Some(kind) => format!("{} ({kind})", name.display),
            None => name.display.clone(),
        };
        if !names.contains(&text) {
            names.push(text);
        }
    }
    names
}

pub fn events_of(record: &Node) -> Vec<Event> {
    record
        .children
        .iter()
        .filter(|n| tags::is_event(n.tag()) || tags::is_attribute(n.tag()))
        .map(event)
        .filter(|e| e.date.is_some() || e.place.is_some() || e.value.is_some())
        .collect()
}

fn event_label(node: &Node) -> String {
    let typed = matches!(node.tag(), "EVEN" | "FACT");
    match node
        .value_of("TYPE")
        .filter(|t| typed && !t.trim().is_empty())
    {
        Some(kind) => kind,
        None => tags::display_label(node.tag()).to_owned(),
    }
}

fn event(node: &Node) -> Event {
    let value = node.logical_value();
    Event {
        label: event_label(node),
        date: node
            .value_of("DATE")
            .map(|d| GedcomDate::parse(&d).display()),
        place: place(node),
        value: Some(value).filter(|v| !v.trim().is_empty() && v != "Y"),
        cause: node.value_of("CAUS").filter(|c| !c.trim().is_empty()),
    }
}

fn place(event: &Node) -> Option<String> {
    let mut parts: Vec<String> = event.value_of("PLAC").into_iter().collect();
    if let Some(addr) = event.first("ADDR") {
        let line = addr.logical_value();
        if !line.trim().is_empty() {
            parts.push(line.replace('\n', ", "));
        }
        for tag in ["CITY", "STAE", "CTRY"] {
            parts.extend(addr.value_of(tag));
        }
    }
    let parts: Vec<String> = parts.into_iter().filter(|p| !p.trim().is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

fn contacts(record: &Node) -> Vec<String> {
    let mut found = Vec::new();
    for node in &record.children {
        match node.tag() {
            "EMAIL" | "PHON" | "WWW" => found.push(node.logical_value()),
            "ADDR" => {
                let nested: Vec<String> = ["EMAIL", "PHON", "WWW"]
                    .iter()
                    .flat_map(|tag| node.all(tag).map(Node::logical_value))
                    .collect();
                if nested.is_empty() {
                    found.push(node.logical_value().replace('\n', ", "));
                } else {
                    found.extend(nested);
                }
            }
            _ => {}
        }
    }
    found.dedup();
    found
}

fn notes(people: &People, record: &Node) -> Vec<String> {
    record
        .children
        .iter()
        .filter(|n| matches!(n.tag(), "NOTE" | "SNOTE"))
        .map(|n| match n.pointer().and_then(|p| people.record(p)) {
            Some(shared) => shared.logical_value(),
            None => n.logical_value(),
        })
        .filter(|text| !text.trim().is_empty())
        .collect()
}

fn media(record: &Node) -> Vec<String> {
    record
        .all("OBJE")
        .flat_map(|obje| obje.all("FILE").map(Node::logical_value))
        .collect()
}

fn added_by(people: &People, record: &Node) -> Option<String> {
    let doc = people.doc;
    match submitters::submitter_of(record) {
        Some(xref) => Some(submitters::name_of(doc, xref)),
        None => submitters::exporter(doc).map(|x| {
            format!(
                "{} (no submitter recorded; file exporter)",
                submitters::name_of(doc, &x)
            )
        }),
    }
}

fn ids(record: &Node) -> Vec<String> {
    record
        .children
        .iter()
        .filter(|n| ID_TAGS.contains(&n.tag()))
        .map(|n| match n.value_of("TYPE") {
            Some(kind) => format!("{} ({kind})", n.logical_value()),
            None => n.logical_value(),
        })
        .collect()
}

fn parents(people: &People, xref: &str) -> Vec<Relative> {
    people
        .parents(xref)
        .iter()
        .map(|p| Relative {
            relation: parent_word(people.sex(p).as_deref()).to_owned(),
            person: people.brief(p),
        })
        .collect()
}

fn families(people: &People, xref: &str) -> Vec<FamilyCard> {
    people
        .unions(xref)
        .into_iter()
        .map(|union| FamilyCard {
            events: people
                .record(&union.family)
                .map(|r| family_events(&FamilyView::from_node(r), r))
                .unwrap_or_default(),
            partner: union.partner.as_deref().map(|p| people.brief(p)),
            children: union
                .children
                .iter()
                .map(|c| Relative {
                    relation: child_word(people.sex(c).as_deref()).to_owned(),
                    person: people.brief(c),
                })
                .collect(),
            id: union.family,
        })
        .collect()
}

fn family_events(view: &FamilyView, record: &Node) -> Vec<Event> {
    view.events
        .iter()
        .filter_map(|e| record.at(&e.path))
        .map(event)
        .filter(|e| e.date.is_some() || e.place.is_some() || e.value.is_some())
        .collect()
}

pub fn render(card: &Card) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{}", card.person);
    if let Some(sex) = &card.sex {
        line(&mut out, "Sex", sex);
    }
    if card.names.len() > 1 {
        for name in &card.names {
            line(&mut out, "Name", name);
        }
    }
    for event in &card.events {
        line(&mut out, &event.label, &event_text(event));
    }
    for contact in &card.contacts {
        line(&mut out, "Contact", contact);
    }
    for note in &card.notes {
        line(&mut out, "Note", &note.replace('\n', "\n              "));
    }
    for file in &card.media {
        line(&mut out, "Media", file);
    }
    for parent in &card.parents {
        line(
            &mut out,
            &capitalize(&parent.relation),
            &parent.person.to_string(),
        );
    }
    for family in &card.families {
        render_family(&mut out, family);
    }
    for id in &card.ids {
        line(&mut out, "Id", id);
    }
    if let Some(added_by) = &card.added_by {
        line(&mut out, "Added by", added_by);
    }
    if let Some(changed) = &card.changed {
        line(&mut out, "Changed", changed);
    }
    out
}

fn render_family(out: &mut String, family: &FamilyCard) {
    let partner = family
        .partner
        .as_ref()
        .map_or_else(|| "unknown partner".to_owned(), Brief::to_string);
    line(out, "Partner", &partner);
    for event in &family.events {
        line(out, &format!("  {}", event.label), &event_text(event));
    }
    for child in &family.children {
        line(
            out,
            &format!("  {}", capitalize(&child.relation)),
            &child.person.to_string(),
        );
    }
}

fn event_text(event: &Event) -> String {
    [&event.value, &event.date, &event.place, &event.cause]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ")
}

fn line(out: &mut String, label: &str, value: &str) {
    let _ = writeln!(out, "  {label:<11} {value}");
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
