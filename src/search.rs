use deunicode::deunicode;
use gedcomkit::Node;

use crate::person::{Brief, People, brief_of};

const NAME_PARTS: [&str; 7] = ["GIVN", "SURN", "SPFX", "NPFX", "NSFX", "NICK", "_MARNM"];

pub fn find(people: &People, query: &str) -> Vec<Brief> {
    if let Some(xref) = as_xref(query).filter(|x| people.is_individual(x)) {
        return vec![people.brief(&xref)];
    }
    let tokens: Vec<String> = normalize(query)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let mut found: Vec<Brief> = people
        .individuals()
        .filter(|record| {
            let hay = haystack(record);
            tokens.iter().all(|token| hay.contains(token.as_str()))
        })
        .map(brief_of)
        .collect();
    found.sort_by_cached_key(|b| (normalize(&b.name), b.life.clone()));
    found
}

pub fn normalize(text: &str) -> String {
    deunicode(text).to_lowercase()
}

fn haystack(record: &Node) -> String {
    let mut parts = Vec::new();
    for name in record.all("NAME") {
        parts.push(name.logical_value().replace('/', " "));
        for tag in NAME_PARTS {
            parts.extend(name.all(tag).map(Node::logical_value));
        }
    }
    normalize(&parts.join(" "))
}

fn as_xref(query: &str) -> Option<String> {
    let query = query.trim();
    if is_wrapped_xref(query) {
        return Some(query.to_owned());
    }
    let bare = query.trim_matches('@');
    if bare.is_empty() || !bare.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let digits = bare.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let prefix = if digits.len() == bare.len() { "I" } else { "" };
    Some(format!("@{prefix}{bare}@"))
}

fn is_wrapped_xref(query: &str) -> bool {
    query.len() > 2
        && query.starts_with('@')
        && query.ends_with('@')
        && !query[1..query.len() - 1].contains(['@', ' '])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xref_forms() {
        assert_eq!(as_xref("@I12@").as_deref(), Some("@I12@"));
        assert_eq!(as_xref("I12").as_deref(), Some("@I12@"));
        assert_eq!(as_xref("12").as_deref(), Some("@I12@"));
        assert_eq!(as_xref("@P_1-a@").as_deref(), Some("@P_1-a@"));
        assert_eq!(as_xref("kari"), None);
        assert_eq!(as_xref("Ola 2"), None);
    }

    #[test]
    fn normalize_strips_diacritics() {
        assert_eq!(normalize("Åse Lund Ørnes"), "ase lund ornes");
    }
}
