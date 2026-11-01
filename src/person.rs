use std::fmt;

use gedcomkit::family::FamilyGraph;
use gedcomkit::view::IndividualView;
use gedcomkit::{Document, Node, RecordIndex};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Brief {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub life: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
}

impl fmt::Display for Brief {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(life) = &self.life {
            write!(f, " ({life})")?;
        }
        write!(f, " {}", self.id)
    }
}

pub struct Union {
    pub family: String,
    pub partner: Option<String>,
    pub children: Vec<String>,
}

pub struct People<'a> {
    pub doc: &'a Document,
    index: RecordIndex<'a>,
    pub graph: FamilyGraph,
}

impl<'a> People<'a> {
    pub fn new(doc: &'a Document) -> Self {
        Self {
            doc,
            index: doc.record_index(),
            graph: FamilyGraph::new(doc),
        }
    }

    pub fn record(&self, xref: &str) -> Option<&'a Node> {
        self.index.get(xref)
    }

    pub fn individuals(&self) -> impl Iterator<Item = &'a Node> {
        self.doc.records_of("INDI")
    }

    pub fn is_individual(&self, xref: &str) -> bool {
        self.record(xref).is_some_and(|r| r.tag() == "INDI")
    }

    pub fn brief(&self, xref: &str) -> Brief {
        match self.record(xref) {
            Some(record) => brief_of(record),
            None => Brief {
                id: xref.to_owned(),
                name: "(missing record)".to_owned(),
                life: None,
                sex: None,
                info: None,
            },
        }
    }

    pub fn sex(&self, xref: &str) -> Option<String> {
        self.record(xref)?.value_of("SEX")
    }

    pub fn birth_key(&self, xref: &str) -> i64 {
        self.record(xref)
            .map(IndividualView::from_node)
            .and_then(|view| {
                view.events
                    .iter()
                    .find(|e| {
                        matches!(e.tag.as_str(), "BIRT" | "CHR" | "BAPM") && e.sort_key.is_some()
                    })
                    .and_then(|e| e.sort_key)
            })
            .unwrap_or(i64::MAX)
    }

    pub fn parents(&self, xref: &str) -> Vec<String> {
        let mut parents = self.graph.parents_of(xref).to_vec();
        parents.sort_by_cached_key(|p| (sex_rank(self.sex(p).as_deref()), p.to_ascii_uppercase()));
        parents
    }

    pub fn unions(&self, xref: &str) -> Vec<Union> {
        self.families_of(xref)
            .into_iter()
            .filter_map(|family| {
                let members = self.graph.family(&family)?;
                let partner = members
                    .partners
                    .iter()
                    .find(|p| !p.eq_ignore_ascii_case(xref))
                    .cloned();
                let mut children = members.children.clone();
                children.sort_by_key(|c| self.birth_key(c));
                Some(Union {
                    family,
                    partner,
                    children,
                })
            })
            .collect()
    }

    fn families_of(&self, xref: &str) -> Vec<String> {
        let mut families: Vec<String> = self
            .record(xref)
            .map(|r| {
                r.all("FAMS")
                    .filter_map(|n| n.pointer().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let mut extra: Vec<String> = self
            .graph
            .families()
            .iter()
            .filter(|(_, m)| m.partners.iter().any(|p| p.eq_ignore_ascii_case(xref)))
            .map(|(key, _)| key.clone())
            .filter(|key| !families.iter().any(|f| f.eq_ignore_ascii_case(key)))
            .collect();
        extra.sort();
        families.extend(extra);
        families
    }
}

pub fn brief_of(record: &Node) -> Brief {
    let view = IndividualView::from_node(record);
    let name = view.display_name();
    Brief {
        id: view.xref.clone(),
        name: if name.trim().is_empty() {
            "Unnamed".to_owned()
        } else {
            name
        },
        life: view.life_years(),
        sex: record.value_of("SEX").filter(|s| !s.trim().is_empty()),
        info: occupation(record).or_else(|| short_note(record)),
    }
}

const SHORT_NOTE: usize = 50;
const GENI_NOTE_PREFIX: &str = "{geni:about_me}";

fn occupation(record: &Node) -> Option<String> {
    record
        .all("OCCU")
        .map(|n| n.logical_value().trim().to_owned())
        .find(|v| !v.is_empty() && !v.eq_ignore_ascii_case("unknown"))
}

fn short_note(record: &Node) -> Option<String> {
    let note = record
        .all("NOTE")
        .find(|n| n.pointer().is_none())?
        .logical_value();
    let text = note.trim().trim_start_matches(GENI_NOTE_PREFIX).trim();
    let is_reference = text.contains("http") || text.starts_with(['*', '[', '\'']);
    let single_line = !text.contains('\n');
    let short = text.chars().count() <= SHORT_NOTE;
    (!text.is_empty() && !is_reference && single_line && short).then(|| text.to_owned())
}

pub fn child_word(sex: Option<&str>) -> &'static str {
    match sex {
        Some("M") => "son",
        Some("F") => "daughter",
        _ => "child",
    }
}

pub fn parent_word(sex: Option<&str>) -> &'static str {
    match sex {
        Some("M") => "father",
        Some("F") => "mother",
        _ => "parent",
    }
}

fn sex_rank(sex: Option<&str>) -> u8 {
    match sex {
        Some("M") => 0,
        Some("F") => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gedcomkit::Document;

    fn info(lines: &str) -> Option<String> {
        let doc = Document::parse(&format!("0 HEAD\n0 @I1@ INDI\n{lines}0 TRLR\n")).unwrap();
        brief_of(&doc.records[1]).info
    }

    #[test]
    fn occupation_wins_and_unknown_is_skipped() {
        assert_eq!(
            info("1 OCCU unknown\n1 OCCU nurse\n1 NOTE short\n").as_deref(),
            Some("nurse")
        );
    }

    #[test]
    fn short_note_without_geni_prefix() {
        assert_eq!(
            info("1 NOTE {geni:about_me} photo taken 1965\n").as_deref(),
            Some("photo taken 1965")
        );
    }

    #[test]
    fn references_and_long_notes_are_skipped() {
        assert_eq!(
            info("1 NOTE {geni:about_me} * Reference: [https://example.org]\n"),
            None
        );
        assert_eq!(info("1 NOTE see http://example.org\n"), None);
        assert_eq!(info(&format!("1 NOTE {}\n", "x".repeat(51))), None);
        assert_eq!(info("1 NOTE first line\n2 CONT second line\n"), None);
    }
}
