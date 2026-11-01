use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use gedcomkit::convert::{Conversion, to_version_5_5_1, to_version_7};
use gedcomkit::dates::GedcomDate;
use gedcomkit::{Document, Node};

use crate::store::{Store, write_atomic};

const RFN_AUTHORITIES: [(&str, &str); 1] = [("geni", "https://www.geni.com/")];

pub fn migrate(store: &mut Store, dry_run: bool) -> Result<()> {
    if store.is_v7() {
        println!(
            "{} is already GEDCOM {}",
            store.path.display(),
            store.version()
        );
        return Ok(());
    }
    let mut conversion = to_version_7(&store.doc);
    print!("{}", report(&conversion));
    print!("{}", tidy_v7(&mut conversion.document));
    if dry_run {
        println!("dry run: nothing written");
        return Ok(());
    }
    store.doc = conversion.document;
    store.save()?;
    println!("wrote {}", store.path.display());
    Ok(())
}

pub fn export(store: &Store, version: &str, output: &Path) -> Result<()> {
    let mut conversion = match version {
        "7" | "7.0" => to_version_7(&store.doc),
        "5.5.1" => to_version_5_5_1(&store.doc),
        other => bail!("unsupported version {other:?}; use 7.0 or 5.5.1"),
    };
    eprint!("{}", report(&conversion));
    if conversion.document.is_version_7_or_later() {
        eprint!("{}", tidy_v7(&mut conversion.document));
    }
    write_atomic(output, &store.encode(&conversion.document))?;
    eprintln!("wrote {}", output.display());
    Ok(())
}

fn tidy_v7(doc: &mut Document) -> String {
    let phrases: usize = doc.records.iter_mut().map(drop_redundant_phrases).sum();
    let rfns: usize = doc.records.iter_mut().map(rfn_to_exid).sum();
    format!(
        "gedcli: {phrases} date PHRASE(s) dropped (same date, only zero-padded); {rfns} RFN → EXID\n"
    )
}

fn drop_redundant_phrases(node: &mut Node) -> usize {
    let mut dropped = 0;
    if node.tag() == "DATE" {
        let date = node.value().unwrap_or_default().to_owned();
        let before = node.children.len();
        node.children.retain(|child| {
            !(child.tag() == "PHRASE"
                && GedcomDate::parse(&child.logical_value())
                    .to_gedcom_7()
                    .as_deref()
                    == Some(&date))
        });
        dropped += before - node.children.len();
    }
    dropped
        + node
            .children
            .iter_mut()
            .map(drop_redundant_phrases)
            .sum::<usize>()
}

fn rfn_to_exid(record: &mut Node) -> usize {
    let mut converted = 0;
    for child in &mut record.children {
        let Some((authority, id)) = child
            .value()
            .filter(|_| child.tag() == "RFN" && child.children.is_empty())
            .and_then(|v| v.split_once(':'))
            .and_then(|(system, id)| authority_uri(system).map(|uri| (uri, id.to_owned())))
        else {
            continue;
        };
        child.set_tag("EXID");
        child.set_value(Some(id));
        child.push(Node::with_value("TYPE", authority));
        converted += 1;
    }
    converted
}

fn authority_uri(system: &str) -> Option<&'static str> {
    RFN_AUTHORITIES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(system))
        .map(|(_, uri)| *uri)
}

fn report(conversion: &Conversion) -> String {
    let (converted, dropped, kept) = conversion.counts();
    let mut out = format!(
        "GEDCOM {} → {}: {converted} converted, {dropped} dropped, {kept} kept as written\n",
        conversion.from, conversion.to
    );
    let mut grouped: BTreeMap<(&str, &str), (usize, &str)> = BTreeMap::new();
    for note in &conversion.notes {
        let entry = grouped
            .entry((note.tag.as_str(), note.kind.label()))
            .or_insert((0, note.detail.as_str()));
        entry.0 += 1;
    }
    for ((tag, kind), (count, example)) in grouped {
        out.push_str(&format!("  {count:>4} × {tag:<9} {kind}, e.g. {example}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tidy(text: &str) -> String {
        let mut doc = Document::parse(text).unwrap();
        tidy_v7(&mut doc);
        doc.to_text()
    }

    #[test]
    fn drops_phrase_equal_to_date() {
        assert_eq!(
            tidy("0 @I1@ INDI\n1 BIRT\n2 DATE 1 MAY 2022\n3 PHRASE 01 MAY 2022\n"),
            "0 @I1@ INDI\n1 BIRT\n2 DATE 1 MAY 2022\n"
        );
    }

    #[test]
    fn keeps_meaningful_phrase() {
        let text = "0 @I1@ INDI\n1 BIRT\n2 DATE 1900\n3 PHRASE around new year 1900\n";
        assert_eq!(tidy(text), text);
    }

    #[test]
    fn geni_rfn_becomes_exid() {
        assert_eq!(
            tidy("0 @I1@ INDI\n1 RFN geni:600\n1 RFN other:7\n"),
            "0 @I1@ INDI\n1 EXID 600\n2 TYPE https://www.geni.com/\n1 RFN other:7\n"
        );
    }
}
