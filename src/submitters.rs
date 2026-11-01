use std::fmt::Write as _;

use gedcomkit::{Document, Node};
use serde::Serialize;

use crate::person::{Brief, People, brief_of};
use crate::search::normalize;

#[derive(Serialize)]
pub struct Tally {
    pub exporter: Option<Contributor>,
    pub contributors: Vec<Contributor>,
    pub unlinked_people: usize,
    pub unlinked_families: usize,
}

#[derive(Serialize, Clone)]
pub struct Contributor {
    pub id: String,
    pub name: String,
    pub people: usize,
    pub families: usize,
}

#[derive(Serialize)]
pub struct Entries {
    pub submitter: Contributor,
    pub linked: Vec<Brief>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unlinked_as_exporter: Vec<Brief>,
}

pub fn exporter(doc: &Document) -> Option<String> {
    doc.header()?.first("SUBM")?.pointer().map(str::to_owned)
}

pub fn submitter_of(record: &Node) -> Option<&str> {
    record.first("SUBM").and_then(Node::pointer)
}

pub fn name_of(doc: &Document, xref: &str) -> String {
    doc.record(xref)
        .and_then(|r| r.value_of("NAME"))
        .unwrap_or_else(|| "(unnamed submitter)".to_owned())
}

pub fn tally(doc: &Document) -> Tally {
    let mut contributors: Vec<Contributor> = doc
        .records_of("SUBM")
        .filter_map(|r| r.xref())
        .map(|xref| Contributor {
            id: xref.to_owned(),
            name: name_of(doc, xref),
            people: linked(doc, "INDI", xref).count(),
            families: linked(doc, "FAM", xref).count(),
        })
        .collect();
    contributors.sort_by(|a, b| b.people.cmp(&a.people).then_with(|| a.name.cmp(&b.name)));
    let exporter = exporter(doc).and_then(|x| contributors.iter().find(|c| c.id == x).cloned());
    Tally {
        exporter,
        contributors,
        unlinked_people: unlinked(doc, "INDI").count(),
        unlinked_families: unlinked(doc, "FAM").count(),
    }
}

pub fn entries(people: &People, query: &str) -> Vec<Entries> {
    let doc = people.doc;
    let tokens: Vec<String> = normalize(query)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let exporter = exporter(doc);
    tally(doc)
        .contributors
        .into_iter()
        .filter(|c| {
            let name = normalize(&c.name);
            tokens.iter().all(|t| name.contains(t.as_str()))
                || c.id.eq_ignore_ascii_case(query.trim())
        })
        .map(|submitter| {
            let is_exporter = exporter.as_deref() == Some(submitter.id.as_str());
            Entries {
                linked: sorted(linked(doc, "INDI", &submitter.id).map(brief_of).collect()),
                unlinked_as_exporter: if is_exporter {
                    sorted(unlinked(doc, "INDI").map(brief_of).collect())
                } else {
                    Vec::new()
                },
                submitter,
            }
        })
        .collect()
}

fn linked<'a>(doc: &'a Document, tag: &'a str, xref: &'a str) -> impl Iterator<Item = &'a Node> {
    doc.records_of(tag)
        .filter(move |r| submitter_of(r).is_some_and(|s| s.eq_ignore_ascii_case(xref)))
}

fn unlinked<'a>(doc: &'a Document, tag: &'a str) -> impl Iterator<Item = &'a Node> {
    doc.records_of(tag).filter(|r| submitter_of(r).is_none())
}

fn sorted(mut people: Vec<Brief>) -> Vec<Brief> {
    people.sort_by_cached_key(|b| normalize(&b.name));
    people
}

pub fn render_tally(t: &Tally) -> String {
    let mut out = String::from("people  submitter\n");
    for c in &t.contributors {
        let mark = if t.exporter.as_ref().is_some_and(|e| e.id == c.id) {
            "  ← exported this file"
        } else {
            ""
        };
        let _ = writeln!(out, "{:>6}  {} {}{mark}", c.people, c.name, c.id);
    }
    let _ = writeln!(out, "{:>6}  (no submitter recorded)", t.unlinked_people);
    if let Some(e) = &t.exporter {
        let _ = writeln!(
            out,
            "\nGeni leaves out SUBM on profiles managed by the exporting account, so the {} \
             people without a submitter are most likely {}'s ({} in total).",
            t.unlinked_people,
            e.name,
            e.people + t.unlinked_people
        );
    }
    out
}

pub fn render_entries(e: &Entries) -> String {
    let total = e.linked.len() + e.unlinked_as_exporter.len();
    let mut out = format!("{} {}: {total} people\n", e.submitter.name, e.submitter.id);
    section(&mut out, "Linked to this submitter", &e.linked);
    section(
        &mut out,
        "No submitter recorded (Geni omits it for the exporter's own profiles)",
        &e.unlinked_as_exporter,
    );
    out
}

fn section(out: &mut String, title: &str, people: &[Brief]) {
    if people.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{title} ({}):", people.len());
    for person in people {
        let _ = writeln!(out, "  {person}");
    }
}

pub fn list_entries(e: &Entries) -> String {
    let mut out = String::from("# link\tname\tlife\tid\n");
    let rows = e
        .linked
        .iter()
        .map(|p| ("subm", p))
        .chain(e.unlinked_as_exporter.iter().map(|p| ("none", p)));
    for (link, p) in rows {
        let _ = writeln!(
            out,
            "{link}\t{}\t{}\t{}",
            p.name,
            p.life.as_deref().unwrap_or(""),
            p.id
        );
    }
    out
}
