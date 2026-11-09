use anyhow::{Result, bail};
use clap::ValueEnum;
use gedcomkit::Node;

use crate::edit;

#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
pub enum Kind {
    Occupation,
    Residence,
    Baptism,
    Christening,
    Confirmation,
    Emigration,
    Immigration,
    Census,
    Education,
    Religion,
    Title,
    Event,
}

impl Kind {
    const fn tag(self) -> &'static str {
        match self {
            Self::Occupation => "OCCU",
            Self::Residence => "RESI",
            Self::Baptism => "BAPM",
            Self::Christening => "CHR",
            Self::Confirmation => "CONF",
            Self::Emigration => "EMIG",
            Self::Immigration => "IMMI",
            Self::Census => "CENS",
            Self::Education => "EDUC",
            Self::Religion => "RELI",
            Self::Title => "TITL",
            Self::Event => "EVEN",
        }
    }

    const fn needs_value(self) -> bool {
        matches!(
            self,
            Self::Occupation | Self::Education | Self::Religion | Self::Title
        )
    }
}

pub struct Fact {
    pub kind: Kind,
    pub value: Option<String>,
    pub kind_label: Option<String>,
    pub date: Option<String>,
    pub place: Option<String>,
}

impl Fact {
    pub fn describe(&self) -> String {
        let parts: Vec<&str> = [&self.kind_label, &self.value, &self.date, &self.place]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect();
        format!("{} {}", self.kind.tag(), parts.join(" · "))
    }
}

/// Checks the fact and writes its date in the file's GEDCOM version.
pub fn checked(mut fact: Fact, v7: bool) -> Result<Fact> {
    let clean = |text: Option<String>| text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    fact.value = clean(fact.value);
    fact.kind_label = clean(fact.kind_label);
    fact.place = clean(fact.place);
    fact.date = clean(fact.date)
        .map(|d| edit::normalize_date(&d, v7))
        .transpose()?;
    if fact.kind.needs_value() && fact.value.is_none() {
        bail!("{:?} needs a value", fact.kind);
    }
    if fact.kind == Kind::Event && fact.kind_label.is_none() {
        bail!("an event needs --type, e.g. --type \"Service militaire\"");
    }
    if fact.value.is_none() && fact.date.is_none() && fact.place.is_none() {
        bail!("give a value, --date or --place");
    }
    Ok(fact)
}

/// Adds the fact as a new structure; returns false when an identical one is already recorded.
pub fn add(record: &mut Node, fact: &Fact) -> bool {
    let node = to_node(fact);
    if record.children.iter().any(|existing| same(existing, &node)) {
        return false;
    }
    let at = record
        .children
        .iter()
        .position(|n| matches!(n.tag(), "DEAT" | "BURI"))
        .unwrap_or_else(|| edit::before_tail(record));
    record.children.insert(at, node);
    true
}

fn to_node(fact: &Fact) -> Node {
    let mut node = match &fact.value {
        Some(value) => Node::with_value(fact.kind.tag(), value),
        None => Node::new(fact.kind.tag()),
    };
    for (tag, value) in [
        ("TYPE", &fact.kind_label),
        ("DATE", &fact.date),
        ("PLAC", &fact.place),
    ] {
        if let Some(value) = value {
            node = node.child(Node::with_value(tag, value));
        }
    }
    node
}

fn same(a: &Node, b: &Node) -> bool {
    let detail = |n: &Node, tag: &str| n.value_of(tag).map(|v| v.to_lowercase());
    a.tag() == b.tag()
        && a.logical_value().to_lowercase() == b.logical_value().to_lowercase()
        && ["TYPE", "DATE", "PLAC"]
            .iter()
            .all(|t| detail(a, t) == detail(b, t))
}
