use std::collections::BTreeMap;
use std::fmt::Write as _;

use clap::ValueEnum;
use gedcomkit::dates::GedcomDate;
use gedcomkit::view::IndividualView;
use gedcomkit::{Document, Node};
use serde::Serialize;

use crate::edit;
use crate::person::{Brief, People, brief_of};
use crate::search::normalize;

const SHOWN: usize = 10;
const OLDEST_LIVING: i32 = 110;
const COUNTRY_ALIASES: [(&str, &str); 30] = [
    ("norge", "Norway"),
    ("noreg", "Norway"),
    ("nrwy", "Norway"),
    ("nrwy.", "Norway"),
    ("norwegen", "Norway"),
    ("norvege", "Norway"),
    ("sverige", "Sweden"),
    ("schweden", "Sweden"),
    ("suede", "Sweden"),
    ("danmark", "Denmark"),
    ("danemark", "Denmark"),
    ("danemark.", "Denmark"),
    ("frankrike", "France"),
    ("frankreich", "France"),
    ("tyskland", "Germany"),
    ("deutschland", "Germany"),
    ("allemagne", "Germany"),
    ("sveits", "Switzerland"),
    ("suisse", "Switzerland"),
    ("schweiz", "Switzerland"),
    ("italia", "Italy"),
    ("suomi", "Finland"),
    ("usa", "United States"),
    ("u.s.a.", "United States"),
    ("us", "United States"),
    ("amerika", "United States"),
    ("united states of america", "United States"),
    ("storbritannia", "United Kingdom"),
    ("canada.", "Canada"),
    ("island", "Iceland"),
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    BrokenReference,
    OneSidedLink,
    BadDate,
    Chronology,
    ProbablyDead,
    CountrySpelling,
    PossibleDuplicate,
    MissingBirth,
    NearEmptyFamily,
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Self::BrokenReference => "Pointers to records that do not exist",
            Self::OneSidedLink => "Family links written on one side only",
            Self::BadDate => "Dates that cannot be read",
            Self::Chronology => "Impossible or unlikely dates",
            Self::ProbablyDead => "Born over 110 years ago, no death recorded",
            Self::CountrySpelling => "Country spellings (gedcli check --fix renames them)",
            Self::PossibleDuplicate => "Possible duplicates (same name and birth year)",
            Self::MissingBirth => "No birth year",
            Self::NearEmptyFamily => "Families with fewer than two partners and no children",
        }
    }

    fn slug(self) -> String {
        self.to_possible_value()
            .map(|v| v.get_name().to_owned())
            .unwrap_or_default()
    }
}

#[derive(Serialize)]
pub struct Issue {
    pub kind: Kind,
    pub id: String,
    pub name: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

pub fn run(people: &People) -> Vec<Issue> {
    let mut issues = Vec::new();
    broken_references(people, &mut issues);
    one_sided_links(people, &mut issues);
    for record in people.individuals() {
        bad_dates(record, &mut issues);
        chronology(people, record, &mut issues);
        country_spelling(record, &mut issues);
    }
    duplicates(people, &mut issues);
    missing_birth(people, &mut issues);
    near_empty_families(people, &mut issues);
    issues.sort_by_key(|i| i.kind);
    issues
}

fn issue(kind: Kind, who: &Brief, detail: String, fix: Option<String>) -> Issue {
    Issue {
        kind,
        id: who.id.clone(),
        name: who.name.clone(),
        detail,
        fix,
    }
}

fn fix_cmd(id: &str, field: &str) -> Option<String> {
    Some(format!("gedcli edit {id} --set {field}=…"))
}

fn broken_references(people: &People, issues: &mut Vec<Issue>) {
    for pointer in people.doc.unresolved_pointers() {
        issues.push(Issue {
            kind: Kind::BrokenReference,
            id: pointer.record.clone(),
            name: people.brief(&pointer.record).name,
            detail: format!("{} {} points to nothing", pointer.tag, pointer.pointer),
            fix: None,
        });
    }
}

fn one_sided_links(people: &People, issues: &mut Vec<Issue>) {
    let doc = people.doc;
    for person in people.individuals() {
        let who = brief_of(person);
        for (tag, back) in [("FAMC", &["CHIL"][..]), ("FAMS", &["HUSB", "WIFE"][..])] {
            for family in person.all(tag).filter_map(Node::pointer) {
                let Some(record) = doc.record(family) else {
                    continue;
                };
                if !points_to(record, back, &who.id) {
                    let detail = format!(
                        "{tag} {family}, but the family has no {} back",
                        back.join("/")
                    );
                    issues.push(issue(Kind::OneSidedLink, &who, detail, None));
                }
            }
        }
    }
    for family in doc.records_of("FAM") {
        let fam = family.xref().unwrap_or_default();
        for (tag, back) in [("CHIL", "FAMC"), ("HUSB", "FAMS"), ("WIFE", "FAMS")] {
            for xref in family.all(tag).filter_map(Node::pointer) {
                let Some(person) = doc.record(xref) else {
                    continue;
                };
                if !points_to(person, &[back], fam) {
                    let detail =
                        format!("family {fam} lists them as {tag}, but they have no {back} back");
                    issues.push(issue(Kind::OneSidedLink, &people.brief(xref), detail, None));
                }
            }
        }
    }
}

fn points_to(record: &Node, tags: &[&str], xref: &str) -> bool {
    tags.iter()
        .flat_map(|tag| record.all(tag))
        .any(|n| n.pointer().is_some_and(|p| p.eq_ignore_ascii_case(xref)))
}

fn bad_dates(record: &Node, issues: &mut Vec<Issue>) {
    let who = brief_of(record);
    for event in &record.children {
        if matches!(event.tag(), "CHAN" | "CREA") {
            continue;
        }
        for date in event.all("DATE") {
            let value = date.logical_value();
            if !GedcomDate::parse(&value).is_parsed() {
                let field = field_for(event.tag(), "date");
                let fix = field.and_then(|f| fix_cmd(&who.id, &f));
                issues.push(issue(
                    Kind::BadDate,
                    &who,
                    format!("{} DATE {value:?}", event.tag()),
                    fix,
                ));
            }
        }
    }
}

fn field_for(tag: &str, part: &str) -> Option<String> {
    let event = match tag {
        "BIRT" => "birth",
        "DEAT" => "death",
        "BURI" => "burial",
        _ => return None,
    };
    Some(format!("{event}.{part}"))
}

struct Years {
    birth: Option<i32>,
    death: Option<i32>,
    dead: bool,
}

fn years(record: &Node) -> Years {
    let view = IndividualView::from_node(record);
    let first = |tags: &[&str]| {
        view.events
            .iter()
            .filter(|e| tags.contains(&e.tag.as_str()))
            .find_map(|e| e.year)
    };
    Years {
        birth: first(&["BIRT", "CHR", "BAPM"]),
        death: first(&["DEAT", "BURI"]),
        dead: view
            .events
            .iter()
            .any(|e| matches!(e.tag.as_str(), "DEAT" | "BURI")),
    }
}

fn chronology(people: &People, record: &Node, issues: &mut Vec<Issue>) {
    let who = brief_of(record);
    let own = years(record);
    if let (Some(birth), Some(death)) = (own.birth, own.death)
        && death < birth
    {
        issues.push(issue(
            Kind::Chronology,
            &who,
            format!("died {death} before birth {birth}"),
            fix_cmd(&who.id, "death.date"),
        ));
    }
    if let Some(birth) = own.birth
        && !own.dead
        && edit::current_year() - birth > OLDEST_LIVING
    {
        let detail = format!("born {birth}, no death or burial recorded");
        issues.push(issue(
            Kind::ProbablyDead,
            &who,
            detail,
            fix_cmd(&who.id, "death.date"),
        ));
    }
    let Some(birth) = own.birth else { return };
    for parent in people.parents(&who.id) {
        let Some(parent_record) = people.record(&parent) else {
            continue;
        };
        let theirs = years(parent_record);
        let name = people.brief(&parent).name;
        if let Some(parent_birth) = theirs.birth {
            let age = birth - parent_birth;
            if !(12..=80).contains(&age) {
                issues.push(issue(
                    Kind::Chronology,
                    &who,
                    format!("born {birth} when parent {name} was {age}"),
                    None,
                ));
            }
        }
        let grace = if people.sex(&parent).as_deref() == Some("M") {
            1
        } else {
            0
        };
        if let Some(parent_death) = theirs.death
            && birth > parent_death + grace
        {
            issues.push(issue(
                Kind::Chronology,
                &who,
                format!("born {birth} after parent {name} died {parent_death}"),
                None,
            ));
        }
    }
}

pub fn canonical_country(value: &str) -> Option<&'static str> {
    let key = normalize(value.trim());
    COUNTRY_ALIASES
        .iter()
        .find(|(alias, _)| *alias == key)
        .map(|(_, canonical)| *canonical)
}

fn country_nodes(record: &Node) -> impl Iterator<Item = &Node> {
    record
        .children
        .iter()
        .flat_map(|event| event.all("ADDR"))
        .flat_map(|addr| addr.all("CTRY"))
}

fn country_spelling(record: &Node, issues: &mut Vec<Issue>) {
    let who = brief_of(record);
    for node in country_nodes(record) {
        let value = node.logical_value();
        if let Some(canonical) = canonical_country(&value) {
            issues.push(issue(
                Kind::CountrySpelling,
                &who,
                format!("{value:?} → {canonical}"),
                None,
            ));
        }
    }
}

pub fn fix_countries(doc: &mut Document) -> Vec<String> {
    let mut changed = Vec::new();
    for record in doc.records.iter_mut().filter(|r| r.tag() == "INDI") {
        let mut touched = false;
        for event in &mut record.children {
            for addr in event.children.iter_mut().filter(|n| n.tag() == "ADDR") {
                for country in addr.children.iter_mut().filter(|n| n.tag() == "CTRY") {
                    if let Some(canonical) = canonical_country(&country.logical_value()) {
                        country.set_logical_value(canonical);
                        touched = true;
                    }
                }
            }
        }
        if touched {
            edit::touch(record);
            changed.push(record.xref().unwrap_or_default().to_owned());
        }
    }
    changed
}

fn duplicates(people: &People, issues: &mut Vec<Issue>) {
    let mut groups: BTreeMap<(String, i32), Vec<Brief>> = BTreeMap::new();
    for record in people.individuals() {
        if let Some(birth) = years(record).birth {
            let who = brief_of(record);
            groups
                .entry((normalize(&who.name), birth))
                .or_default()
                .push(who);
        }
    }
    for group in groups.into_values().filter(|g| g.len() > 1) {
        let ids: Vec<&str> = group.iter().map(|b| b.id.as_str()).collect();
        for who in &group {
            let others: Vec<&str> = ids.iter().copied().filter(|id| *id != who.id).collect();
            issues.push(issue(
                Kind::PossibleDuplicate,
                who,
                format!("same as {}?", others.join(", ")),
                None,
            ));
        }
    }
}

fn missing_birth(people: &People, issues: &mut Vec<Issue>) {
    for record in people.individuals() {
        if years(record).birth.is_none() {
            let who = brief_of(record);
            issues.push(issue(
                Kind::MissingBirth,
                &who,
                "no birth, christening or baptism year".to_owned(),
                fix_cmd(&who.id, "birth.date"),
            ));
        }
    }
}

fn near_empty_families(people: &People, issues: &mut Vec<Issue>) {
    let mut families: Vec<(&String, _)> = people.graph.families().iter().collect();
    families.sort_by_key(|(key, _)| key.as_str());
    for (key, members) in families {
        if members.children.is_empty() && members.partners.len() < 2 {
            let partner = members.partners.first().map(|p| people.brief(p));
            let name = partner
                .as_ref()
                .map_or_else(|| "(nobody)".to_owned(), |p| p.name.clone());
            issues.push(Issue {
                kind: Kind::NearEmptyFamily,
                id: key.clone(),
                name,
                detail: format!("{} partner(s), no children", members.partners.len()),
                fix: None,
            });
        }
    }
}

pub fn render(issues: &[Issue], all: bool) -> String {
    if issues.is_empty() {
        return "No problems found.\n".to_owned();
    }
    let mut grouped: BTreeMap<Kind, Vec<&Issue>> = BTreeMap::new();
    for i in issues {
        grouped.entry(i.kind).or_default().push(i);
    }
    let mut out = String::new();
    for (kind, items) in &grouped {
        let _ = writeln!(out, "{} ({}) [{}]", kind.title(), items.len(), kind.slug());
        let shown = if all { items.len() } else { SHOWN };
        for i in items.iter().take(shown) {
            let _ = writeln!(out, "  {} {}: {}", i.name, i.id, i.detail);
            if let Some(fix) = &i.fix {
                let _ = writeln!(out, "      fix: {fix}");
            }
        }
        if items.len() > shown {
            let _ = writeln!(
                out,
                "  … {} more (--all, or --only {})",
                items.len() - shown,
                kind.slug()
            );
        }
        out.push('\n');
    }
    out
}

pub fn list(issues: &[Issue]) -> String {
    let mut out = String::from("# kind\tid\tname\tdetail\tfix\n");
    for i in issues {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}",
            i.kind.slug(),
            i.id,
            i.name,
            i.detail,
            i.fix.as_deref().unwrap_or("")
        );
    }
    out
}
