mod card;
mod chart;
mod count;
mod edit;
mod migrate;
mod person;
mod relatives;
mod render;
mod search;
mod select;
mod store;
mod submitters;
mod summary;

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
        #[arg(long, group = "relatives", value_name = "GENERATIONS", num_args = 0..=1, default_missing_value = DEFAULT_GENERATIONS)]
        children: Option<u32>,
        /// Show ancestors, this many generations up
        #[arg(long, group = "relatives", value_name = "GENERATIONS", num_args = 0..=1, default_missing_value = DEFAULT_GENERATIONS)]
        parents: Option<u32>,
        /// Count relatives per generation instead of listing them
        #[arg(long, requires = "relatives")]
        count: bool,
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
    /// Overview of the file: format, counts, connections, data quality
    Summary {
        /// Output format: tree or list print text, json prints JSON
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// Who entered the data: people per submitter, or the people one submitter entered
    Submitters {
        /// Submitter name words (accents ignored); omit for the overview
        query: Vec<String>,
        /// Only print the number of people
        #[arg(long)]
        count: bool,
        #[arg(long, value_enum)]
        format: Option<Format>,
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
            count,
            format,
        } => show(
            &store,
            &target,
            Relatives {
                children,
                parents,
                count,
            },
            format,
        ),
        Command::Edit {
            target,
            assignments,
            yes,
        } => edit(&mut store, &target, &assignments, yes),
        Command::Summary { format } => print_summary(&store, format),
        Command::Submitters {
            query,
            count,
            format,
        } => print_submitters(&store, &query, count, format),
        Command::Migrate { dry_run } => migrate::migrate(&mut store, dry_run),
        Command::Export { version, output } => migrate::export(&store, &version, &output),
    }
}

fn print_summary(store: &Store, format: Option<Format>) -> Result<()> {
    let summary = summary::build(store);
    if format == Some(Format::Json) {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        print!("{}", summary::render(&summary));
    }
    Ok(())
}

fn print_submitters(
    store: &Store,
    query: &[String],
    count: bool,
    format: Option<Format>,
) -> Result<()> {
    let json = format == Some(Format::Json);
    if query.is_empty() {
        let tally = submitters::tally(&store.doc);
        if json {
            println!("{}", serde_json::to_string_pretty(&tally)?);
        } else {
            print!("{}", submitters::render_tally(&tally));
        }
        return Ok(());
    }
    let people = People::new(&store.doc);
    let found = submitters::entries(&people, &query.join(" "));
    if found.is_empty() {
        bail!("no submitter matches; run `gedcli submitters` to see them all");
    }
    for entries in &found {
        let total = entries.linked.len() + entries.unlinked_as_exporter.len();
        match (json, count, format) {
            (true, _, _) => println!("{}", serde_json::to_string_pretty(entries)?),
            (false, true, _) => println!(
                "{total}\t{}\t{}",
                entries.submitter.name, entries.submitter.id
            ),
            (false, false, Some(Format::List)) => print!("{}", submitters::list_entries(entries)),
            _ => print!("{}", submitters::render_entries(entries)),
        }
    }
    Ok(())
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

struct Relatives {
    children: Option<u32>,
    parents: Option<u32>,
    count: bool,
}

fn show(store: &Store, target: &Target, wanted: Relatives, format: Option<Format>) -> Result<()> {
    let format = format.unwrap_or(if std::io::stdout().is_terminal() {
        Format::Tree
    } else {
        Format::List
    });
    let people = People::new(&store.doc);
    let interactive = on_terminal() && format.is_visual();
    let person = find(&people, target, interactive, format == Format::Json)?;
    if wanted.children.is_none() && wanted.parents.is_none() {
        return show_card(&people, &person, format);
    }
    if wanted.count {
        return show_counts(&people, &person, &wanted, format);
    }
    let ancestors = wanted
        .parents
        .map(|n| relatives::ancestors(&people, &person.id, n));
    let descendants = wanted
        .children
        .map(|n| relatives::descendants(&people, &person.id, n));
    let sections: Vec<String> = match format {
        Format::Json => {
            let value =
                json!({ "person": person, "ancestors": ancestors, "descendants": descendants });
            vec![format!("{}\n", serde_json::to_string_pretty(&value)?)]
        }
        Format::Chart => {
            let up = ancestors.as_ref().map(chart::ancestors);
            let down = descendants.as_ref().map(chart::descendants);
            [up, down].into_iter().flatten().collect()
        }
        Format::Tree => [ancestors, descendants]
            .iter()
            .flatten()
            .map(render::tree)
            .collect(),
        Format::List => [ancestors, descendants]
            .iter()
            .flatten()
            .map(render::list)
            .collect(),
    };
    let text = sections.join("\n");
    warn_if_too_wide(&text, format);
    print!("{text}");
    Ok(())
}

fn show_counts(people: &People, person: &Brief, wanted: &Relatives, format: Format) -> Result<()> {
    let tables: Vec<count::Counts> = [
        wanted.parents.map(|n| (n, count::Direction::Ancestors)),
        wanted.children.map(|n| (n, count::Direction::Descendants)),
    ]
    .into_iter()
    .flatten()
    .map(|(n, direction)| count::count(people, &person.id, n, direction))
    .collect();
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&tables)?),
        Format::List => print!("{}", tables.iter().map(count::list).collect::<String>()),
        _ => print!(
            "{}",
            tables
                .iter()
                .map(count::render)
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
    Ok(())
}

fn warn_if_too_wide(text: &str, format: Format) {
    if format != Format::Chart {
        return;
    }
    let Some((_, columns)) = console::Term::stdout().size_checked() else {
        return;
    };
    let widest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    if widest > usize::from(columns) {
        eprintln!(
            "note: chart is {widest} columns wide, terminal has {columns}; use fewer generations or pipe to `less -S`"
        );
    }
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
