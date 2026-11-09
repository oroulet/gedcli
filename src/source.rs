use gedcomkit::{Document, Node};

use crate::add;

const SOURCE_PREFIX: &str = "SG";

/// The source record titled `title`, created if the file has none yet.
pub fn ensure(doc: &mut Document, title: &str) -> String {
    let existing = doc
        .records_of("SOUR")
        .find(|r| r.value_of("TITL").as_deref() == Some(title))
        .and_then(|r| r.xref().map(str::to_owned));
    if let Some(xref) = existing {
        return xref;
    }
    let xref = doc.next_xref(SOURCE_PREFIX);
    let record = Node::record(&xref, "SOUR").child(Node::with_value("TITL", title));
    add::insert_record(doc, "SOUR", record);
    xref
}

/// Adds a citation of `source` to a person or family record, once.
pub fn cite(record: &mut Node, source: &str) {
    let cited = record
        .all("SOUR")
        .any(|n| n.pointer().is_some_and(|p| p.eq_ignore_ascii_case(source)));
    if cited {
        return;
    }
    let at = record
        .children
        .iter()
        .position(|n| n.tag() == "CHAN")
        .unwrap_or(record.children.len());
    record.children.insert(at, Node::with_value("SOUR", source));
}
