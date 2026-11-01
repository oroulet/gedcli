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
    }
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
