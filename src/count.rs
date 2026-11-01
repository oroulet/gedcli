use std::collections::HashSet;
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
    let mut everyone: HashSet<String> = HashSet::new();
    let mut appearances = 0;
    let mut rows = Vec::new();
    for generation in 1..=generations {
        frontier = next_generation(people, &frontier, direction);
        if frontier.is_empty() {
            break;
        }
        appearances += frontier.len();
        everyone.extend(frontier.iter().map(|x| x.to_ascii_uppercase()));
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
        total_distinct: everyone.len(),
        in_several_generations: appearances - everyone.len(),
    }
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
