use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;

use serde::Serialize;

use crate::person::{Brief, People, child_word, parent_word};

#[derive(Serialize)]
pub struct Kinship {
    pub from: Brief,
    pub to: Brief,
    pub relation: String,
    pub common_ancestors: Vec<Brief>,
    pub path: Vec<Step>,
}

#[derive(Serialize, Clone)]
pub struct Step {
    pub relation: String,
    #[serde(flatten)]
    pub person: Brief,
}

struct Blood {
    up: u32,
    down: u32,
    ancestors: Vec<String>,
}

pub fn relate(people: &People, a: &str, b: &str) -> Kinship {
    let (relation, common, path) = describe(people, a, b);
    Kinship {
        from: people.brief(a),
        to: people.brief(b),
        relation,
        common_ancestors: common.iter().map(|x| people.brief(x)).collect(),
        path: path
            .into_iter()
            .map(|(relation, x)| Step {
                relation,
                person: people.brief(&x),
            })
            .collect(),
    }
}

type Path = Vec<(String, String)>;
type Described = (String, Vec<String>, Path);

fn describe(people: &People, a: &str, b: &str) -> Described {
    if a.eq_ignore_ascii_case(b) {
        return (
            "same person".to_owned(),
            Vec::new(),
            vec![(String::new(), a.to_owned())],
        );
    }
    if let Some(blood) = blood(people, a, b) {
        let path = blood_path(people, a, b, &blood);
        let common = if blood.up > 0 && blood.down > 0 {
            blood.ancestors.clone()
        } else {
            Vec::new()
        };
        return (blood_name(people, a, b, &blood), common, path);
    }
    if is_partner(people, a, b) {
        let path = vec![
            (String::new(), a.to_owned()),
            (partner_word(people, b).to_owned(), b.to_owned()),
        ];
        return (partner_word(people, b).to_owned(), Vec::new(), path);
    }
    if let Some(found) = through_partner(people, a, b) {
        return found;
    }
    match shortest_path(people, a, b) {
        Some(path) => (possessive(&path), Vec::new(), path),
        None => ("not connected".to_owned(), Vec::new(), Vec::new()),
    }
}

fn blood(people: &People, a: &str, b: &str) -> Option<Blood> {
    let from_a = people.graph.ancestors(a);
    let from_b = people.graph.ancestors(b);
    let mut shared: Vec<(u32, u32, String)> = from_a
        .iter()
        .filter_map(|(key, &up)| from_b.get(key).map(|&down| (up, down, key.clone())))
        .collect();
    let best = shared.iter().map(|(up, down, _)| (up + down, *up)).min()?;
    shared.retain(|(up, down, _)| (up + down, *up) == best);
    shared.sort_by(|x, y| x.2.cmp(&y.2));
    let (up, down) = (shared[0].0, shared[0].1);
    Some(Blood {
        up,
        down,
        ancestors: shared.into_iter().map(|(_, _, key)| key).collect(),
    })
}

fn blood_name(people: &People, a: &str, b: &str, blood: &Blood) -> String {
    let sex = people.sex(b);
    let sex = sex.as_deref();
    let half = blood.up == 1
        && blood.down == 1
        && blood.ancestors.len() == 1
        && people.parents(a).len() == 2
        && people.parents(b).len() == 2;
    let name = kinship_name(blood.up, blood.down, sex);
    if half { format!("half-{name}") } else { name }
}

pub fn kinship_name(up: u32, down: u32, sex: Option<&str>) -> String {
    let word = |male: &str, female: &str, neutral: &str| -> String {
        match sex {
            Some("M") => male,
            Some("F") => female,
            _ => neutral,
        }
        .to_owned()
    };
    let either = |prefix: &str, male: &str, female: &str| -> String {
        match sex {
            Some("M") => format!("{prefix}{male}"),
            Some("F") => format!("{prefix}{female}"),
            _ => format!("{prefix}{male} or {prefix}{female}"),
        }
    };
    match (up, down) {
        (0, 0) => "same person".to_owned(),
        (0, d) => format!("{}{}", grand(d), word("son", "daughter", "child")),
        (u, 0) => format!("{}{}", grand(u), word("father", "mother", "parent")),
        (1, 1) => word("brother", "sister", "sibling"),
        (1, d) => either(&grand(d - 1), "nephew", "niece"),
        (u, 1) => either(&greats(u - 2), "uncle", "aunt"),
        (u, d) => cousin(u.min(d) - 1, u.abs_diff(d)),
    }
}

fn grand(generations: u32) -> String {
    match generations {
        0 | 1 => String::new(),
        n => format!("{}grand", greats(n - 2)),
    }
}

fn greats(n: u32) -> String {
    match n {
        0 => String::new(),
        1 => "great-".to_owned(),
        n => format!("{n}× great-"),
    }
}

fn cousin(degree: u32, removed: u32) -> String {
    let removed = match removed {
        0 => String::new(),
        1 => " once removed".to_owned(),
        2 => " twice removed".to_owned(),
        n => format!(" {n} times removed"),
    };
    format!("{} cousin{removed}", ordinal(degree))
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    match n {
        1 => "first".to_owned(),
        2 => "second".to_owned(),
        3 => "third".to_owned(),
        n => format!("{n}{suffix}"),
    }
}

fn blood_path(people: &People, a: &str, b: &str, blood: &Blood) -> Path {
    let ancestor = &blood.ancestors[0];
    let up = chain_to_ancestor(people, a, ancestor);
    let mut down = chain_to_ancestor(people, b, ancestor);
    down.reverse();
    let mut path: Path = vec![(String::new(), a.to_owned())];
    for x in up.iter().skip(1) {
        path.push((parent_word(people.sex(x).as_deref()).to_owned(), x.clone()));
    }
    for x in down.iter().skip(1) {
        path.push((child_word(people.sex(x).as_deref()).to_owned(), x.clone()));
    }
    path
}

fn chain_to_ancestor(people: &People, from: &str, ancestor: &str) -> Vec<String> {
    let mut previous: HashMap<String, String> = HashMap::new();
    let mut queue = VecDeque::from([from.to_owned()]);
    let mut seen = HashSet::from([from.to_ascii_uppercase()]);
    while let Some(current) = queue.pop_front() {
        if current.eq_ignore_ascii_case(ancestor) {
            let mut chain = vec![current.clone()];
            let mut at = current;
            while let Some(prev) = previous.get(&at) {
                chain.push(prev.clone());
                at = prev.clone();
            }
            chain.reverse();
            return chain;
        }
        for parent in people.parents(&current) {
            if seen.insert(parent.to_ascii_uppercase()) {
                previous.insert(parent.clone(), current.clone());
                queue.push_back(parent);
            }
        }
    }
    vec![from.to_owned()]
}

fn is_partner(people: &People, a: &str, b: &str) -> bool {
    people
        .graph
        .partners_of(a)
        .iter()
        .any(|p| p.eq_ignore_ascii_case(b))
}

fn partner_word(people: &People, xref: &str) -> &'static str {
    match people.sex(xref).as_deref() {
        Some("M") => "husband",
        Some("F") => "wife",
        _ => "partner",
    }
}

fn through_partner(people: &People, a: &str, b: &str) -> Option<Described> {
    let mut found: Vec<(u32, Described)> = Vec::new();
    for partner in sorted(people.graph.partners_of(a)) {
        if let Some(blood) = blood(people, &partner, b) {
            let described = partners_relative(people, a, &partner, b, &blood);
            found.push((blood.up + blood.down, described));
        }
    }
    for partner in sorted(people.graph.partners_of(b)) {
        if let Some(blood) = blood(people, a, &partner) {
            let described = relatives_partner(people, a, &partner, b, &blood);
            found.push((blood.up + blood.down, described));
        }
    }
    found
        .into_iter()
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, described)| described)
}

fn partners_relative(people: &People, a: &str, partner: &str, b: &str, blood: &Blood) -> Described {
    let name = in_law_of_partner(
        &blood_name(people, partner, b, blood),
        partner_word(people, partner),
    );
    let mut path = vec![(String::new(), a.to_owned())];
    path.extend(
        blood_path(people, partner, b, blood)
            .into_iter()
            .enumerate()
            .map(|(i, (r, x))| {
                if i == 0 {
                    (partner_word(people, partner).to_owned(), x)
                } else {
                    (r, x)
                }
            }),
    );
    (name, Vec::new(), path)
}

fn relatives_partner(people: &People, a: &str, partner: &str, b: &str, blood: &Blood) -> Described {
    let name = partner_of_relative(
        &blood_name(people, a, partner, blood),
        partner_word(people, b),
    );
    let mut path = blood_path(people, a, partner, blood);
    path.push((partner_word(people, b).to_owned(), b.to_owned()));
    (name, Vec::new(), path)
}

fn sorted(xrefs: &[String]) -> Vec<String> {
    let mut xrefs = xrefs.to_vec();
    xrefs.sort_by_key(|x| x.to_ascii_uppercase());
    xrefs
}

fn in_law_of_partner(relation: &str, partner: &str) -> String {
    match relation.trim_start_matches("half-") {
        "father" | "mother" | "brother" | "sister" => format!("{relation}-in-law"),
        "son" | "daughter" | "child" => format!("step{relation}"),
        _ => format!("{partner}'s {relation}"),
    }
}

fn partner_of_relative(relation: &str, partner: &str) -> String {
    match (relation.trim_start_matches("half-"), partner) {
        ("son" | "daughter" | "child", "husband") => "son-in-law".to_owned(),
        ("son" | "daughter" | "child", "wife") => "daughter-in-law".to_owned(),
        ("brother" | "sister" | "sibling", "husband") => "brother-in-law".to_owned(),
        ("brother" | "sister" | "sibling", "wife") => "sister-in-law".to_owned(),
        ("father" | "mother" | "parent", "husband") => "stepfather".to_owned(),
        ("father" | "mother" | "parent", "wife") => "stepmother".to_owned(),
        (_, partner) => format!("{relation}'s {partner}"),
    }
}

fn shortest_path(people: &People, a: &str, b: &str) -> Option<Path> {
    let mut previous: HashMap<String, (String, String)> = HashMap::new();
    let mut queue = VecDeque::from([a.to_owned()]);
    let mut seen = HashSet::from([a.to_ascii_uppercase()]);
    while let Some(current) = queue.pop_front() {
        if current.eq_ignore_ascii_case(b) {
            let mut path = vec![];
            let mut at = current;
            while let Some((relation, prev)) = previous.get(&at) {
                path.push((relation.clone(), at.clone()));
                at = prev.clone();
            }
            path.push((String::new(), at));
            path.reverse();
            return Some(path);
        }
        for (relation, next) in neighbours(people, &current) {
            if seen.insert(next.to_ascii_uppercase()) {
                previous.insert(next.clone(), (relation, current.clone()));
                queue.push_back(next);
            }
        }
    }
    None
}

fn neighbours(people: &People, xref: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in people.parents(xref) {
        out.push((parent_word(people.sex(&p).as_deref()).to_owned(), p));
    }
    for c in sorted(people.graph.children_of(xref)) {
        out.push((child_word(people.sex(&c).as_deref()).to_owned(), c));
    }
    for p in sorted(people.graph.partners_of(xref)) {
        out.push((partner_word(people, &p).to_owned(), p));
    }
    out
}

fn possessive(path: &Path) -> String {
    path.iter()
        .skip(1)
        .map(|(r, _)| r.as_str())
        .collect::<Vec<_>>()
        .join("'s ")
}

pub fn render(k: &Kinship) -> String {
    let mut out = match k.relation.as_str() {
        "same person" => format!("{} is the same person.\n", k.to.name),
        "not connected" => format!("{} and {} are not connected.\n", k.from.name, k.to.name),
        relation => format!("{} is {}'s {relation}.\n", k.to.name, k.from.name),
    };
    if !k.common_ancestors.is_empty() {
        let names: Vec<String> = k.common_ancestors.iter().map(ToString::to_string).collect();
        let _ = writeln!(out, "  common ancestors  {}", names.join(", "));
    }
    let steps: Vec<String> = k
        .path
        .iter()
        .map(|s| {
            if s.relation.is_empty() {
                s.person.name.clone()
            } else {
                format!("{} {}", s.relation, s.person.name)
            }
        })
        .collect();
    if steps.len() > 1 {
        let _ = writeln!(out, "  path              {}", steps.join(" → "));
    }
    out
}

pub fn list(k: &Kinship) -> String {
    let mut out = format!(
        "relation\t{}\n# step\trelation\tname\tlife\tid\n",
        k.relation
    );
    for (i, s) in k.path.iter().enumerate() {
        let life = s.person.life.as_deref().unwrap_or("");
        let _ = writeln!(
            out,
            "{i}\t{}\t{}\t{life}\t{}",
            s.relation, s.person.name, s.person.id
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::kinship_name;

    #[test]
    fn names() {
        assert_eq!(kinship_name(0, 3, Some("F")), "great-granddaughter");
        assert_eq!(kinship_name(4, 0, Some("M")), "2× great-grandfather");
        assert_eq!(kinship_name(1, 2, Some("M")), "nephew");
        assert_eq!(kinship_name(1, 3, Some("F")), "grandniece");
        assert_eq!(kinship_name(3, 1, Some("F")), "great-aunt");
        assert_eq!(kinship_name(2, 2, None), "first cousin");
        assert_eq!(kinship_name(3, 2, None), "first cousin once removed");
        assert_eq!(kinship_name(4, 4, None), "third cousin");
        assert_eq!(kinship_name(1, 1, None), "sibling");
        assert_eq!(kinship_name(0, 2, None), "grandchild");
    }

    #[test]
    fn neutral_names_repeat_the_prefix() {
        assert_eq!(kinship_name(1, 2, None), "nephew or niece");
        assert_eq!(
            kinship_name(1, 4, None),
            "great-grandnephew or great-grandniece"
        );
        assert_eq!(kinship_name(3, 1, None), "great-uncle or great-aunt");
    }

    #[test]
    fn distant_cousins_have_proper_ordinals() {
        assert_eq!(kinship_name(22, 22, None), "21st cousin");
        assert_eq!(kinship_name(13, 12, None), "11th cousin once removed");
    }
}
