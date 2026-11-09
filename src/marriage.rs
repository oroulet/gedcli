use anyhow::{Context, Result};
use gedcomkit::{Document, Node};

use crate::person::{Brief, People};
use crate::{add, edit};

pub struct Couple {
    pub first: Brief,
    pub second: Brief,
    pub family: Option<String>,
}

pub struct Change {
    pub family: String,
    pub before: [Option<String>; 2],
    pub after: [Option<String>; 2],
}

/// The family the two people already share as partners, if any.
pub fn couple(people: &People, first: Brief, second: Brief) -> Couple {
    let family = people
        .unions(&first.id)
        .into_iter()
        .find(|u| {
            u.partner
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(&second.id))
        })
        .map(|u| u.family);
    Couple {
        first,
        second,
        family,
    }
}

/// Sets the marriage date and place of the couple's family, creating the family when they have none.
pub fn apply(
    doc: &mut Document,
    couple: &Couple,
    date: Option<&str>,
    place: Option<&str>,
) -> Result<Change> {
    let v7 = doc.is_version_7_or_later();
    let date = date.map(|d| edit::normalize_date(d, v7)).transpose()?;
    let family = match &couple.family {
        Some(f) => f.clone(),
        None => add::new_family(
            doc,
            &[
                (&couple.first.id, couple.first.sex.as_deref()),
                (&couple.second.id, couple.second.sex.as_deref()),
            ],
            &[],
        ),
    };
    let record = doc.record_mut(&family).context("family vanished")?;
    let before = marriage_of(record);
    for (sub, value) in [("DATE", date.as_deref()), ("PLAC", place)] {
        if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
            edit::set_event_detail(record, "MARR", sub, Some(value));
        }
    }
    let after = marriage_of(record);
    if before != after {
        edit::touch(record);
    }
    Ok(Change {
        family,
        before,
        after,
    })
}

fn marriage_of(family: &Node) -> [Option<String>; 2] {
    let event = family.first("MARR");
    ["DATE", "PLAC"].map(|tag| event.and_then(|e| e.value_of(tag)))
}
