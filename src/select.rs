use std::fmt;

use anyhow::{Result, anyhow, bail};
use dialoguer::FuzzySelect;

use crate::person::Brief;

#[derive(Debug)]
pub struct Ambiguous(pub usize);

impl fmt::Display for Ambiguous {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} people match; rerun with --pick N, an xref, or a more precise query",
            self.0
        )
    }
}

impl std::error::Error for Ambiguous {}

pub struct Choice {
    pub pick: Option<usize>,
    pub interactive: bool,
    pub json: bool,
}

pub fn choose(candidates: Vec<Brief>, choice: &Choice) -> Result<Brief> {
    if candidates.is_empty() {
        bail!("nobody matches");
    }
    if let Some(k) = choice.pick {
        let count = candidates.len();
        return candidates
            .into_iter()
            .nth(k.saturating_sub(1))
            .filter(|_| k > 0)
            .ok_or_else(|| anyhow!("--pick {k} is out of range 1..={count}"));
    }
    if candidates.len() == 1 {
        return Ok(candidates.into_iter().next().expect("one candidate"));
    }
    if choice.interactive {
        let labels: Vec<String> = candidates.iter().map(Brief::to_string).collect();
        let index = FuzzySelect::new()
            .with_prompt(format!("{} matches", candidates.len()))
            .items(&labels)
            .default(0)
            .interact()?;
        return Ok(candidates.into_iter().nth(index).expect("selected index"));
    }
    print_candidates(&candidates, choice.json)?;
    Err(Ambiguous(candidates.len()).into())
}

fn print_candidates(candidates: &[Brief], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(candidates)?);
        return Ok(());
    }
    println!("# pick\tname\tlife\tid");
    for (i, c) in candidates.iter().enumerate() {
        println!(
            "{}\t{}\t{}\t{}",
            i + 1,
            c.name,
            c.life.as_deref().unwrap_or(""),
            c.id
        );
    }
    Ok(())
}
