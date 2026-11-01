use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use serde::Serialize;

use crate::person::{Brief, People};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ancestors,
    Descendants,
}

#[derive(Serialize)]
pub struct Counts {
    pub person: Brief,
    pub direction: &'static str,
    pub generations: Vec<Generation>,
    pub total_distinct: usize,
    pub in_several_generations: usize,
}

#[derive(Serialize)]
pub struct Generation {
    pub generation: u32,
    pub name: String,
    pub found: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub possible: Option<u64>,
}

pub fn count(people: &People, root: &str, generations: u32, direction: Direction) -> Counts {
    let mut frontier: Vec<String> = vec![root.to_owned()];
    let mut appearances: HashMap<String, usize> = HashMap::new();
    let mut rows = Vec::new();
    for generation in 1..=generations.min(longest_possible_line(people)) {
        frontier = next_generation(people, &frontier, direction);
        if frontier.is_empty() {
            break;
        }
        for xref in &frontier {
            *appearances.entry(xref.to_ascii_uppercase()).or_default() += 1;
        }
        rows.push(Generation {
            generation,
            name: generation_name(generation, direction),
            found: frontier.len(),
            possible: (direction == Direction::Ancestors).then(|| 2u64.saturating_pow(generation)),
        });
    }
    Counts {
        person: people.brief(root),
        direction: match direction {
            Direction::Ancestors => "ancestors",
            Direction::Descendants => "descendants",
        },
        generations: rows,
        total_distinct: appearances.len(),
        in_several_generations: appearances.values().filter(|&&n| n > 1).count(),
    }
}

fn longest_possible_line(people: &People) -> u32 {
    u32::try_from(people.individuals().count()).unwrap_or(u32::MAX)
}

fn next_generation(people: &People, frontier: &[String], direction: Direction) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut next = Vec::new();
    for xref in frontier {
        let relatives = match direction {
            Direction::Ancestors => people.parents(xref),
            Direction::Descendants => people.graph.children_of(xref).to_vec(),
        };
        for relative in relatives {
            if seen.insert(relative.to_ascii_uppercase()) {
                next.push(relative);
            }
        }
    }
    next
}

fn generation_name(generation: u32, direction: Direction) -> String {
    let (one, base) = match direction {
        Direction::Ancestors => ("parents", "grandparents"),
        Direction::Descendants => ("children", "grandchildren"),
    };
    match generation {
        1 => one.to_owned(),
        2 => base.to_owned(),
        3 => format!("great-{base}"),
        n => format!("{}× great-{base}", n - 2),
    }
}

pub fn render(counts: &Counts) -> String {
    let mut out = format!("{} of {}\n", capitalized(counts.direction), counts.person);
    let ancestors = counts.direction == "ancestors";
    if ancestors {
        out.push_str("  gen  generation                    found  possible  complete\n");
    } else {
        out.push_str("  gen  generation                    found\n");
    }
    for g in &counts.generations {
        let _ = write!(out, "  {:>3}  {:<28}{:>6}", g.generation, g.name, g.found);
        if let Some(possible) = g.possible {
            let percent = g.found as f64 * 100.0 / possible as f64;
            let _ = write!(out, "  {possible:>8}  {percent:>7.0}%");
        }
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "  total {} distinct {}",
        counts.total_distinct, counts.direction
    );
    if counts.in_several_generations > 0 {
        let _ = writeln!(
            out,
            "  {} of them appear in more than one generation (cousin marriages)",
            counts.in_several_generations
        );
    }
    out
}

pub fn list(counts: &Counts) -> String {
    let mut out = String::from("# gen\tname\tfound\tpossible\n");
    for g in &counts.generations {
        let possible = g.possible.map(|p| p.to_string()).unwrap_or_default();
        let _ = writeln!(out, "{}\t{}\t{}\t{possible}", g.generation, g.name, g.found);
    }
    let _ = writeln!(
        out,
        "total\t{}\t{}\t",
        counts.direction, counts.total_distinct
    );
    out
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use gedcomkit::Document;

    use super::*;

    fn doc(families: &str) -> Document {
        let mut text = String::from("0 HEAD\n");
        for xref in ["R", "P", "Q", "X"] {
            text.push_str(&format!("0 @{xref}@ INDI\n"));
        }
        Document::parse(&format!("{text}{families}0 TRLR\n")).unwrap()
    }

    #[test]
    fn a_person_in_three_generations_is_counted_once() {
        let doc = doc("0 @F1@ FAM\n1 HUSB @X@\n1 WIFE @P@\n1 CHIL @R@\n\
             0 @F2@ FAM\n1 HUSB @X@\n1 WIFE @Q@\n1 CHIL @P@\n\
             0 @F3@ FAM\n1 HUSB @X@\n1 CHIL @Q@\n");
        let counts = count(&People::new(&doc), "@R@", 10, Direction::Ancestors);
        assert_eq!(counts.total_distinct, 3);
        assert_eq!(counts.in_several_generations, 1);
    }

    #[test]
    fn a_cyclic_pedigree_stops() {
        let doc = doc("0 @F1@ FAM\n1 HUSB @P@\n1 CHIL @R@\n0 @F2@ FAM\n1 HUSB @R@\n1 CHIL @P@\n");
        let counts = count(&People::new(&doc), "@R@", u32::MAX, Direction::Ancestors);
        assert_eq!(counts.generations.len(), 4);
    }
}
