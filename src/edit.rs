use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow, bail};
use dialoguer::{Editor, Input, Select};
use gedcomkit::Node;
use gedcomkit::dates::{DatePoint, GedcomDate, timestamp_node};
use gedcomkit::names::PersonalName;

const TAIL_TAGS: [&str; 13] = [
    "FAMC", "FAMS", "ASSO", "SUBM", "RFN", "EXID", "REFN", "RIN", "_UID", "OBJE", "SOUR", "CHAN",
    "CREA",
];
const EVENT_TAGS_KEPT_WHEN_EMPTY: [&str; 2] = ["DEAT", "BURI"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Given,
    Surname,
    Sex,
    BirthDate,
    BirthPlace,
    DeathDate,
    DeathPlace,
    BurialDate,
    BurialPlace,
    Occupation,
    Note,
}

pub const FIELDS: [Field; 11] = [
    Field::Given,
    Field::Surname,
    Field::Sex,
    Field::BirthDate,
    Field::BirthPlace,
    Field::DeathDate,
    Field::DeathPlace,
    Field::BurialDate,
    Field::BurialPlace,
    Field::Occupation,
    Field::Note,
];

impl Field {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Given => "name.given",
            Self::Surname => "name.surname",
            Self::Sex => "sex",
            Self::BirthDate => "birth.date",
            Self::BirthPlace => "birth.place",
            Self::DeathDate => "death.date",
            Self::DeathPlace => "death.place",
            Self::BurialDate => "burial.date",
            Self::BurialPlace => "burial.place",
            Self::Occupation => "occupation",
            Self::Note => "note",
        }
    }

    fn parse(key: &str) -> Result<Self> {
        FIELDS
            .into_iter()
            .find(|f| f.key() == key)
            .ok_or_else(|| anyhow!("unknown field {key:?}; known: {}", field_keys()))
    }

    const fn event(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::BirthDate => Some(("BIRT", "DATE")),
            Self::BirthPlace => Some(("BIRT", "PLAC")),
            Self::DeathDate => Some(("DEAT", "DATE")),
            Self::DeathPlace => Some(("DEAT", "PLAC")),
            Self::BurialDate => Some(("BURI", "DATE")),
            Self::BurialPlace => Some(("BURI", "PLAC")),
            _ => None,
        }
    }

    const fn is_date(self) -> bool {
        matches!(self, Self::BirthDate | Self::DeathDate | Self::BurialDate)
    }
}

pub fn field_keys() -> String {
    FIELDS.map(Field::key).join(", ")
}

#[derive(Debug)]
pub struct Change {
    pub field: Field,
    pub before: Option<String>,
    pub after: Option<String>,
}

pub fn get(record: &Node, field: Field) -> Option<String> {
    let value = match field {
        Field::Given => record
            .first("NAME")
            .and_then(|n| PersonalName::from_node(n).given),
        Field::Surname => record
            .first("NAME")
            .and_then(|n| PersonalName::from_node(n).surname),
        Field::Sex => record.value_of("SEX"),
        Field::Occupation => record.value_of("OCCU"),
        Field::Note => inline_note(record).map(Node::logical_value),
        _ => {
            let (tag, sub) = field.event()?;
            record.first(tag).and_then(|e| e.value_of(sub))
        }
    };
    value.filter(|v| !v.trim().is_empty())
}

pub fn apply_assignments(
    record: &mut Node,
    assignments: &[String],
    v7: bool,
) -> Result<Vec<Change>> {
    let mut changes = Vec::new();
    for assignment in assignments {
        let (key, raw) = assignment
            .split_once('=')
            .ok_or_else(|| anyhow!("--set expects field=value, got {assignment:?}"))?;
        let change = apply(record, Field::parse(key.trim())?, raw, v7)?;
        merge(&mut changes, change);
    }
    Ok(changes)
}

pub fn interactive(record: &mut Node, v7: bool) -> Result<Vec<Change>> {
    let mut changes = Vec::new();
    loop {
        let mut items: Vec<String> = FIELDS
            .iter()
            .map(|&f| format!("{:<13} {}", f.key(), preview(get(record, f))))
            .collect();
        items.push("✔ done".to_owned());
        items.push("✘ cancel".to_owned());
        let index = Select::new()
            .with_prompt("Field to change")
            .items(&items)
            .default(0)
            .interact()?;
        match FIELDS.get(index) {
            Some(&field) => match prompt_and_apply(record, field, v7) {
                Ok(change) => merge(&mut changes, change),
                Err(err) => eprintln!("✘ {err}"),
            },
            None if index == FIELDS.len() => return Ok(changes),
            None => return Ok(Vec::new()),
        }
    }
}

fn prompt_and_apply(record: &mut Node, field: Field, v7: bool) -> Result<Change> {
    let current = get(record, field).unwrap_or_default();
    let raw = if field == Field::Note {
        Editor::new()
            .edit(&current)?
            .unwrap_or_else(|| current.clone())
    } else {
        Input::<String>::new()
            .with_prompt(format!("{} (empty removes)", field.key()))
            .with_initial_text(current)
            .allow_empty(true)
            .interact_text()?
    };
    apply(record, field, &raw, v7)
}

fn preview(value: Option<String>) -> String {
    match value {
        None => "—".to_owned(),
        Some(v) => {
            let first = v.lines().next().unwrap_or_default();
            let short: String = first.chars().take(50).collect();
            if short.len() < v.len() {
                format!("{short}…")
            } else {
                short
            }
        }
    }
}

fn merge(changes: &mut Vec<Change>, change: Change) {
    match changes.iter_mut().find(|c| c.field == change.field) {
        Some(existing) => existing.after = change.after,
        None => changes.push(change),
    }
    changes.retain(|c| c.before != c.after);
}

pub fn apply(record: &mut Node, field: Field, raw: &str, v7: bool) -> Result<Change> {
    let before = get(record, field);
    let value = normalize(field, raw, v7)?;
    set(record, field, value.as_deref());
    Ok(Change {
        field,
        before,
        after: get(record, field),
    })
}

fn normalize(field: Field, raw: &str, v7: bool) -> Result<Option<String>> {
    let value = if field == Field::Note {
        raw.trim_end()
    } else {
        raw.trim()
    };
    if value.is_empty() {
        return Ok(None);
    }
    if field.is_date() {
        return normalize_date(value, v7).map(Some);
    }
    if field == Field::Sex {
        return normalize_sex(value, v7).map(Some);
    }
    if field != Field::Note && value.contains('/') && matches!(field, Field::Given | Field::Surname)
    {
        bail!("a name part cannot contain '/'");
    }
    Ok(Some(value.to_owned()))
}

pub(crate) fn normalize_date(value: &str, v7: bool) -> Result<String> {
    let date = GedcomDate::parse(value);
    let written = if !date.is_parsed() {
        None
    } else if v7 {
        date.to_gedcom_7()
    } else {
        date.to_gedcom_5_5_1()
    };
    written.ok_or_else(|| {
        anyhow!("cannot read date {value:?}; try 12 JUN 1950, JUN 1950, 1950, ABT 1900, BEF 1900, BET 1900 AND 1910")
    })
}

fn normalize_sex(value: &str, v7: bool) -> Result<String> {
    let upper = value.to_ascii_uppercase();
    let allowed: &[&str] = if v7 {
        &["M", "F", "X", "U"]
    } else {
        &["M", "F", "U"]
    };
    let code = match upper.as_str() {
        "MALE" => "M",
        "FEMALE" => "F",
        other => other,
    };
    if allowed.contains(&code) {
        Ok(code.to_owned())
    } else {
        bail!("sex must be one of {}", allowed.join(", "))
    }
}

fn set(record: &mut Node, field: Field, value: Option<&str>) {
    match field {
        Field::Given | Field::Surname => set_name_part(record, field, value),
        Field::Sex => set_simple(record, "SEX", value),
        Field::Occupation => set_occupation(record, value),
        Field::Note => set_note(record, value),
        _ => {
            let (tag, sub) = field.event().expect("event field");
            set_event_detail(record, tag, sub, value);
        }
    }
}

fn set_name_part(record: &mut Node, field: Field, value: Option<&str>) {
    let old = get(record, field);
    let index = match record.children.iter().position(|n| n.tag() == "NAME") {
        Some(index) => index,
        None => insert_child(record, Node::new("NAME"), 0),
    };
    let name_node = &mut record.children[index];
    let payload = name_node.logical_value();
    let (updated, sub) = match field {
        Field::Given => (with_given(&payload, old.as_deref(), value), "GIVN"),
        _ => (with_surname(&payload, old.as_deref(), value), "SURN"),
    };
    name_node.set_logical_value(&updated);
    if let Some(i) = name_node.children.iter().position(|n| n.tag() == sub) {
        match value {
            Some(v) => name_node.children[i].set_value(Some(v.to_owned())),
            None => {
                name_node.children.remove(i);
            }
        }
    }
}

type Surname<'a> = Option<(&'a str, &'a str)>;

fn split_payload(payload: &str) -> (&str, Surname<'_>) {
    match payload.split_once('/') {
        None => (payload, None),
        Some((given, after)) => match after.split_once('/') {
            Some((surname, rest)) => (given, Some((surname, rest))),
            None => (given, Some((after, ""))),
        },
    }
}

fn join_payload(given: &str, surname: Surname) -> String {
    let Some((surname, rest)) = surname else {
        return given.to_owned();
    };
    let slashed = format!("/{surname}/{rest}");
    if given.is_empty() {
        slashed
    } else {
        format!("{given} {slashed}")
    }
}

fn with_given(payload: &str, old: Option<&str>, new: Option<&str>) -> String {
    let (given, surname) = split_payload(payload);
    join_payload(&replace_words(given, old, new), surname)
}

fn with_surname(payload: &str, old: Option<&str>, new: Option<&str>) -> String {
    let (given, surname) = split_payload(payload);
    let given = given.trim();
    match (surname, new) {
        (None, None) => payload.to_owned(),
        (None, Some(new)) => join_payload(given, Some((new, ""))),
        (Some((part, rest)), _) => {
            let part = replace_words(part, old, new);
            if part.is_empty() {
                format!("{given}{rest}").trim().to_owned()
            } else {
                join_payload(given, Some((&part, rest)))
            }
        }
    }
}

fn replace_words(part: &str, old: Option<&str>, new: Option<&str>) -> String {
    let words: Vec<&str> = part.split_whitespace().collect();
    let old: Vec<&str> = old.unwrap_or_default().split_whitespace().collect();
    let new: Vec<&str> = new.unwrap_or_default().split_whitespace().collect();
    let found = (!old.is_empty())
        .then(|| words.windows(old.len()).position(|w| w == old.as_slice()))
        .flatten();
    let Some(at) = found else {
        return new.join(" ");
    };
    [&words[..at], &new, &words[at + old.len()..]]
        .concat()
        .join(" ")
}

fn set_simple(record: &mut Node, tag: &str, value: Option<&str>) {
    let index = record.children.iter().position(|n| n.tag() == tag);
    match (index, value) {
        (Some(i), Some(v)) => record.children[i].set_logical_value(v),
        (Some(i), None) => {
            record.children.remove(i);
        }
        (None, Some(v)) => {
            let at = after_names(record);
            insert_child(record, Node::with_value(tag, v), at);
        }
        (None, None) => {}
    }
}

fn set_occupation(record: &mut Node, value: Option<&str>) {
    let index = record.children.iter().position(|n| n.tag() == "OCCU");
    match (index, value) {
        (Some(i), Some(v)) => record.children[i].set_logical_value(v),
        (Some(i), None) if record.children[i].children.is_empty() => {
            record.children.remove(i);
        }
        (Some(i), None) => record.children[i].set_logical_value(""),
        (None, Some(v)) => {
            let at = before_tail(record);
            insert_child(record, Node::with_value("OCCU", v), at);
        }
        (None, None) => {}
    }
}

fn inline_note(record: &Node) -> Option<&Node> {
    record.all("NOTE").find(|n| n.pointer().is_none())
}

fn set_note(record: &mut Node, value: Option<&str>) {
    let index = record
        .children
        .iter()
        .position(|n| n.tag() == "NOTE" && n.pointer().is_none());
    match (index, value) {
        (Some(i), Some(v)) => record.children[i].set_logical_value(v),
        (Some(i), None) => {
            record.children.remove(i);
        }
        (None, Some(v)) => {
            let at = before_tail(record);
            insert_child(record, Node::with_value("NOTE", v), at);
        }
        (None, None) => {}
    }
}

fn set_event_detail(record: &mut Node, tag: &str, sub: &str, value: Option<&str>) {
    let index = record.children.iter().position(|n| n.tag() == tag);
    let index = match (index, value) {
        (Some(i), _) => i,
        (None, Some(_)) => {
            let at = before_tail(record);
            insert_child(record, Node::new(tag), at)
        }
        (None, None) => return,
    };
    let event = &mut record.children[index];
    let detail = event.children.iter().position(|n| n.tag() == sub);
    match (detail, value) {
        (Some(d), Some(v)) => event.children[d].set_logical_value(v),
        (Some(d), None) => {
            event.children.remove(d);
        }
        (None, Some(v)) => {
            let at = if sub == "DATE" { 0 } else { date_end(event) };
            event.children.insert(at, Node::with_value(sub, v));
            if event.value() == Some("Y") {
                event.set_value(None);
            }
        }
        (None, None) => {}
    }
    if event.children.is_empty() && event.value().is_none() {
        if EVENT_TAGS_KEPT_WHEN_EMPTY.contains(&tag) {
            event.set_value(Some("Y".to_owned()));
        } else {
            record.children.remove(index);
        }
    }
}

fn date_end(event: &Node) -> usize {
    event
        .children
        .iter()
        .position(|n| n.tag() == "DATE")
        .map_or(0, |i| i + 1)
}

fn after_names(record: &Node) -> usize {
    record
        .children
        .iter()
        .rposition(|n| n.tag() == "NAME")
        .map_or(0, |i| i + 1)
}

pub(crate) fn before_tail(record: &Node) -> usize {
    record
        .children
        .iter()
        .position(|n| TAIL_TAGS.contains(&n.tag()))
        .unwrap_or(record.children.len())
}

fn insert_child(record: &mut Node, child: Node, at: usize) -> usize {
    record.children.insert(at, child);
    at
}

pub fn touch(record: &mut Node) {
    let (date, time) = now_utc();
    let stamp = timestamp_node("CHAN", date, Some(&time));
    match record.children.iter().position(|n| n.tag() == "CHAN") {
        Some(i) => record.children[i] = stamp,
        None => record.children.push(stamp),
    }
}

pub fn current_year() -> i32 {
    now_utc().0.year
}

fn now_utc() -> (DatePoint, String) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or_default();
    let rest = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let point = DatePoint::from_ymd(year, Some(month), Some(day)).expect("valid calendar day");
    let time = format!(
        "{:02}:{:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    );
    (point, time)
}

// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year as i32, month as u8, day as u8)
}

pub fn render_changes(changes: &[Change]) -> String {
    changes
        .iter()
        .map(|c| {
            format!(
                "  {:<13} {} → {}\n",
                c.field.key(),
                c.before.as_deref().unwrap_or("—"),
                c.after.as_deref().unwrap_or("—")
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gedcomkit::Document;

    fn person(text: &str) -> Node {
        let doc = Document::parse(&format!("0 HEAD\n{text}0 TRLR\n")).unwrap();
        doc.records[1].clone()
    }

    fn render(record: &Node) -> String {
        Document {
            records: vec![record.clone()],
        }
        .to_text()
    }

    #[test]
    fn sets_birth_place_after_date() {
        let mut p = person("0 @I1@ INDI\n1 NAME Ann /Lee/\n1 BIRT\n2 DATE 1900\n1 FAMC @F1@\n");
        apply(&mut p, Field::BirthPlace, "Oslo", true).unwrap();
        assert_eq!(
            render(&p),
            "0 @I1@ INDI\n1 NAME Ann /Lee/\n1 BIRT\n2 DATE 1900\n2 PLAC Oslo\n1 FAMC @F1@\n"
        );
    }

    #[test]
    fn creates_death_before_links_and_normalizes_date() {
        let mut p = person("0 @I1@ INDI\n1 NAME Ann /Lee/\n1 FAMC @F1@\n");
        apply(&mut p, Field::DeathDate, "about 5 january 1950", true).unwrap();
        assert_eq!(
            render(&p),
            "0 @I1@ INDI\n1 NAME Ann /Lee/\n1 DEAT\n2 DATE ABT 5 JAN 1950\n1 FAMC @F1@\n"
        );
    }

    #[test]
    fn removing_death_date_keeps_death_fact() {
        let mut p = person("0 @I1@ INDI\n1 DEAT\n2 DATE 1950\n");
        apply(&mut p, Field::DeathDate, "", true).unwrap();
        assert_eq!(render(&p), "0 @I1@ INDI\n1 DEAT Y\n");
    }

    #[test]
    fn removing_birth_date_drops_empty_birth() {
        let mut p = person("0 @I1@ INDI\n1 BIRT\n2 DATE 1950\n");
        apply(&mut p, Field::BirthDate, "", true).unwrap();
        assert_eq!(render(&p), "0 @I1@ INDI\n");
    }

    #[test]
    fn rejects_unreadable_date() {
        let mut p = person("0 @I1@ INDI\n");
        assert!(apply(&mut p, Field::BirthDate, "sometime", true).is_err());
    }

    #[test]
    fn surname_updates_payload_and_surn() {
        let mut p = person("0 @I1@ INDI\n1 NAME Ann /Lee/\n2 GIVN Ann\n2 SURN Lee\n");
        apply(&mut p, Field::Surname, "Berg", true).unwrap();
        assert_eq!(
            render(&p),
            "0 @I1@ INDI\n1 NAME Ann /Berg/\n2 GIVN Ann\n2 SURN Berg\n"
        );
    }

    #[test]
    fn name_edits_keep_title_particle_suffix_and_nickname() {
        let text = "0 @I1@ INDI\n1 NAME Dr. Jan /van Berg/ Jr.\n2 NPFX Dr.\n2 GIVN Jan\n2 SPFX van\n2 SURN Berg\n2 NSFX Jr.\n";
        let mut p = person(text);
        apply(&mut p, Field::Given, "Johan", true).unwrap();
        apply(&mut p, Field::Surname, "Strand", true).unwrap();
        assert!(render(&p).starts_with("0 @I1@ INDI\n1 NAME Dr. Johan /van Strand/ Jr.\n2 NPFX Dr.\n2 GIVN Johan\n2 SPFX van\n2 SURN Strand\n"));
        let mut p = person("0 @I1@ INDI\n1 NAME Kari 'Mia' /Lund/\n2 GIVN Kari\n");
        apply(&mut p, Field::Surname, "Berg", true).unwrap();
        assert!(render(&p).contains("1 NAME Kari 'Mia' /Berg/\n"));
    }

    #[test]
    fn name_parts_can_be_added_and_removed() {
        let mut p = person("0 @I1@ INDI\n1 NAME Ann\n");
        apply(&mut p, Field::Surname, "Lee", true).unwrap();
        assert!(render(&p).contains("1 NAME Ann /Lee/\n"));
        apply(&mut p, Field::Given, "", true).unwrap();
        assert!(render(&p).contains("1 NAME /Lee/\n"));
        apply(&mut p, Field::Given, "Per Ole", true).unwrap();
        apply(&mut p, Field::Surname, "", true).unwrap();
        assert!(render(&p).contains("1 NAME Per Ole\n"));
    }

    #[test]
    fn sex_is_validated() {
        let mut p = person("0 @I1@ INDI\n1 NAME Ann /Lee/\n");
        apply(&mut p, Field::Sex, "female", false).unwrap();
        assert_eq!(render(&p), "0 @I1@ INDI\n1 NAME Ann /Lee/\n1 SEX F\n");
        assert!(apply(&mut p, Field::Sex, "X", false).is_err());
    }

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_758), (2026, 11, 1));
    }
}
