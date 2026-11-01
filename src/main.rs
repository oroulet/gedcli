mod card;
mod edit;
mod migrate;
mod person;
mod relatives;
mod render;
mod search;
mod select;
mod store;

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use dialoguer::Confirm;
use serde_json::json;

use person::{Brief, People};
use render::Format;
use select::{Ambiguous, Choice};
use store::Store;

const DEFAULT_GENERATIONS: &str = "10";

#[derive(Parser)]
#[command(version, about = "Browse and edit a GEDCOM family tree")]
struct Cli {
    /// GEDCOM file [default: the only *.ged in the current directory]
    #[arg(long, short, global = true, env = "GEDCLI_FILE")]
    file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct Target {
    /// Words of the name (case and accents ignored), or an xref like @I123@
    #[arg(required = true, num_args = 1..)]
    query: Vec<String>,
    /// Take the N-th candidate (1-based) when several people match
    #[arg(long, value_name = "N")]
    pick: Option<usize>,
}

#[derive(Subcommand)]
enum Command {
    /// Show a person, or their descendants/ancestors
    Show {
        #[command(flatten)]
        target: Target,
        /// Show descendants, this many generations deep
        #[arg(long, value_name = "GENERATIONS", num_args = 0..=1, default_missing_value = DEFAULT_GENERATIONS)]
        children: Option<u32>,
        /// Show ancestors, this many generations up
        #[arg(long, value_name = "GENERATIONS", num_args = 0..=1, default_missing_value = DEFAULT_GENERATIONS)]
        parents: Option<u32>,
        /// Output format [default: tree on a terminal, list otherwise]
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// Edit a person's common fields, interactively or with --set
    #[command(after_help = format!("Fields: {}", edit::field_keys()))]
    Edit {
        #[command(flatten)]
        target: Target,
        /// Set a field without prompting, e.g. --set birth.date="12 JUN 1950" (empty value removes)
        #[arg(long = "set", value_name = "FIELD=VALUE")]
        assignments: Vec<String>,
        /// Write without asking for confirmation
        #[arg(long, short)]
        yes: bool,
    },
    /// Convert the file in place to GEDCOM 7.0
    Migrate {
        /// Only print what the conversion would do
        #[arg(long)]
        dry_run: bool,
    },
    /// Write a converted copy of the file
    Export {
        /// Target GEDCOM version: 7.0 or 5.5.1
        #[arg(long = "to", value_name = "VERSION")]
        version: String,
        #[arg(long, short)]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) if err.is::<Ambiguous>() => {
            eprintln!("error: {err}");
            ExitCode::from(2)
        }
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let mut store = Store::open(store::resolve(cli.file)?)?;
    match cli.command {
        Command::Show {
            target,
            children,
            parents,
            format,
        } => show(&store, &target, children, parents, format),
        Command::Edit {
            target,
            assignments,
            yes,
        } => edit(&mut store, &target, &assignments, yes),
        Command::Migrate { dry_run } => migrate::migrate(&mut store, dry_run),
        Command::Export { version, output } => migrate::export(&store, &version, &output),
    }
}

fn on_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn find(people: &People, target: &Target, interactive: bool, json: bool) -> Result<Brief> {
    let choice = Choice {
        pick: target.pick,
        interactive,
        json,
    };
    select::choose(search::find(people, &target.query.join(" ")), &choice)
}

fn show(
    store: &Store,
    target: &Target,
    children: Option<u32>,
    parents: Option<u32>,
    format: Option<Format>,
) -> Result<()> {
    let format = format.unwrap_or(if std::io::stdout().is_terminal() {
        Format::Tree
    } else {
        Format::List
    });
    let people = People::new(&store.doc);
    let interactive = on_terminal() && format == Format::Tree;
    let person = find(&people, target, interactive, format == Format::Json)?;
    if children.is_none() && parents.is_none() {
        return show_card(&people, &person, format);
    }
    let ancestors = parents.map(|n| relatives::ancestors(&people, &person.id, n));
    let descendants = children.map(|n| relatives::descendants(&people, &person.id, n));
    if format == Format::Json {
        let value = json!({ "person": person, "ancestors": ancestors, "descendants": descendants });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    let draw = if format == Format::Tree {
        render::tree
    } else {
        render::list
    };
    let sections: Vec<String> = [ancestors, descendants]
        .iter()
        .flatten()
        .map(draw)
        .collect();
    print!("{}", sections.join("\n"));
    Ok(())
}

fn show_card(people: &People, person: &Brief, format: Format) -> Result<()> {
    let record = people.record(&person.id).context("record vanished")?;
    let card = card::build(people, record);
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&card)?),
        _ => print!("{}", card::render(&card)),
    }
    Ok(())
}

fn edit(store: &mut Store, target: &Target, assignments: &[String], yes: bool) -> Result<()> {
    let interactive = on_terminal();
    let v7 = store.is_v7();
    if !v7 {
        eprintln!(
            "note: file is GEDCOM {}; `gedcli migrate` converts it to 7.0",
            store.version()
        );
    }
    let (person, mut record) = {
        let people = People::new(&store.doc);
        let person = find(&people, target, interactive, false)?;
        let record = people
            .record(&person.id)
            .context("record vanished")?
            .clone();
        (person, record)
    };
    if interactive && assignments.is_empty() {
        print!(
            "{}",
            card::render(&card::build(&People::new(&store.doc), &record))
        );
    }
    let changes = if assignments.is_empty() {
        if !interactive {
            bail!("not on a terminal: use --set field=value");
        }
        edit::interactive(&mut record, v7)?
    } else {
        edit::apply_assignments(&mut record, assignments, v7)?
    };
    if changes.is_empty() {
        println!("no changes");
        return Ok(());
    }
    println!("{person}");
    print!("{}", edit::render_changes(&changes));
    if !yes && !confirm(interactive)? {
        println!("not written");
        return Ok(());
    }
    edit::touch(&mut record);
    *store
        .doc
        .record_mut(&person.id)
        .context("record vanished")? = record;
    store.save()?;
    println!("wrote {}", store.path.display());
    Ok(())
}

fn confirm(interactive: bool) -> Result<bool> {
    if !interactive {
        bail!("not on a terminal: pass --yes to write");
    }
    Ok(Confirm::new()
        .with_prompt("Write changes?")
        .default(true)
        .interact()?)
}
