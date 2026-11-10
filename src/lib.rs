mod add;
mod card;
mod check;
mod compare;
mod count;
mod edit;
mod fact;
mod html;
mod kinship;
mod marriage;
mod media;
mod migrate;
mod person;
mod relatives;
mod render;
mod search;
mod select;
mod source;
mod store;
mod submitters;
mod summary;
mod ui;

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

#[derive(Subcommand)]
enum MediaCommand {
    /// Download media linked by web address into a folder next to the file and link the local copies
    Fetch {
        /// Only list what would be downloaded
        #[arg(long)]
        dry_run: bool,
        /// Folder for the files, relative to the .ged file
        #[arg(long, default_value = "media")]
        dir: String,
    },
}

#[derive(Args)]
struct SourceArg {
    /// Cite this source (by title) on what is changed; the source record is created if missing
    #[arg(long = "source", value_name = "TITLE")]
    title: Option<String>,
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
        /// Show every match, one after the other, instead of asking
        #[arg(long, conflicts_with = "pick")]
        all: bool,
        /// Show xrefs in tree output (list and json always include them)
        #[arg(long)]
        ids: bool,
        /// Count relatives per generation instead of listing them
        #[arg(long)]
        count: bool,
        /// Output format [default: tree on a terminal, list otherwise]
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// Add a relative, e.g. add "kari lund" daughter Liv Berg 1980
    Add {
        /// The existing person: name words in quotes, or an xref
        anchor: String,
        #[arg(value_enum)]
        relation: add::Relation,
        /// First name(s); quote several
        given: String,
        /// Surname; "-" for none
        surname: String,
        /// Birth date, e.g. 2012, "12 JUN 2012", "about 1900"
        birth: Option<String>,
        /// Death date
        death: Option<String>,
        /// For a child: the other parent, to choose the family when there are several
        #[arg(long = "with", value_name = "PARTNER")]
        with_partner: Option<String>,
        #[command(flatten)]
        source: SourceArg,
        /// Write without asking for confirmation
        #[arg(long, short)]
        yes: bool,
    },
    /// Full-screen browser: a person with partner, parents and children; edit and add as you go
    Ui {
        /// Name words or an xref to start from [default: where you left off, else yourself]
        query: Vec<String>,
    },
    /// Add a dated fact (occupation, residence, baptism, …) without replacing existing ones
    #[command(
        after_help = "Example: gedcli fact \"ola berg\" residence --date 1910 --place \"Grøtavær, Trondenes\""
    )]
    Fact {
        #[command(flatten)]
        target: Target,
        #[arg(long, value_enum)]
        kind: fact::Kind,
        /// The fact's text: the occupation, education, religion or title
        #[arg(long)]
        value: Option<String>,
        /// For --kind event: what kind of event, e.g. "Service militaire"
        #[arg(long = "type", value_name = "TYPE")]
        kind_label: Option<String>,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        place: Option<String>,
        #[command(flatten)]
        source: SourceArg,
        /// Write without asking for confirmation
        #[arg(long, short)]
        yes: bool,
    },
    /// Set the marriage date and place of a couple (creates their family if needed)
    Marriage {
        /// First partner: name words in quotes, or an xref
        first: String,
        /// Second partner: name words in quotes, or an xref
        second: String,
        /// Marriage date, e.g. "17 DEC 1942"
        #[arg(long)]
        date: Option<String>,
        /// Marriage place
        #[arg(long)]
        place: Option<String>,
        #[command(flatten)]
        source: SourceArg,
        /// Write without asking for confirmation
        #[arg(long, short)]
        yes: bool,
    },
    /// Edit a person's common fields, interactively or with --set
    #[command(after_help = format!("Fields: {}", edit::field_keys()))]
    Edit {
        #[command(flatten)]
        target: Target,
        /// Set a field without prompting, e.g. --set birth.date="12 JUN 1950" (empty value removes)
        #[arg(long = "set", value_name = "FIELD=VALUE")]
        assignments: Vec<String>,
        #[command(flatten)]
        source: SourceArg,
        /// Write without asking for confirmation
        #[arg(long, short)]
        yes: bool,
    },
    /// How two people are related, e.g. relation "kari lund" ola
    Relation {
        /// First person: name words in quotes, or an xref
        from: String,
        /// Second person: name words in quotes, or an xref
        to: String,
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// List data problems, with the command that fixes each where there is one
    Check {
        /// Only these kinds (repeatable)
        #[arg(long, value_enum)]
        only: Vec<check::Kind>,
        /// Show every issue instead of 10 per kind
        #[arg(long)]
        all: bool,
        /// Rename country spellings (Norge, Nrwy. …) to one name, after confirmation
        #[arg(long)]
        fix: bool,
        /// Write the fix without asking
        #[arg(long, short)]
        yes: bool,
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// Write a static HTML page: a tree diagram, then every person with all details and notes
    Html {
        #[command(flatten)]
        target: Target,
        /// Ancestor generations [default: 10 when --children is not given]
        #[arg(long, value_name = "GENERATIONS", num_args = 0..=1, default_missing_value = DEFAULT_GENERATIONS)]
        parents: Option<u32>,
        /// Descendant generations
        #[arg(long, value_name = "GENERATIONS", num_args = 0..=1, default_missing_value = DEFAULT_GENERATIONS)]
        children: Option<u32>,
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Compare with another GEDCOM file (e.g. a newer Geni export): new, missing and changed people
    Compare {
        /// The other file
        other: PathBuf,
        #[arg(long, value_enum)]
        format: Option<Format>,
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
        /// Link every person without a submitter to the file's exporter (HEAD.SUBM)
        #[arg(long, conflicts_with_all = ["query", "count"])]
        claim: bool,
        /// Write the claim without asking
        #[arg(long, short, requires = "claim")]
        yes: bool,
        #[arg(long, value_enum)]
        format: Option<Format>,
    },
    /// Photos and documents: fetch web media into a local folder
    Media {
        #[command(subcommand)]
        action: MediaCommand,
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

/// Runs gedcli with these command-line arguments (the first is the program name).
pub fn main_with<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match run(Cli::parse_from(args)) {
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
            all,
            ids,
            format,
        } => show(
            &store,
            &target,
            Relatives {
                children,
                parents,
                count,
                all,
                ids,
            },
            format,
        ),
        Command::Add {
            anchor,
            relation,
            given,
            surname,
            birth,
            death,
            with_partner,
            source,
            yes,
        } => {
            let person = add::new_person(
                &given,
                &surname,
                birth.as_deref(),
                death.as_deref(),
                store.is_v7(),
            )?;
            add_relative(
                &mut store,
                &anchor,
                relation,
                &person,
                with_partner.as_deref(),
                source.title.as_deref(),
                yes,
            )
        }
        Command::Ui { query } => ui::start(&mut store, &query.join(" ")),
        Command::Edit {
            target,
            assignments,
            source,
            yes,
        } => edit(
            &mut store,
            &target,
            &assignments,
            source.title.as_deref(),
            yes,
        ),
        Command::Fact {
            target,
            kind,
            value,
            kind_label,
            date,
            place,
            source,
            yes,
        } => {
            let fact = fact::Fact {
                kind,
                value,
                kind_label,
                date,
                place,
            };
            add_fact(&mut store, &target, fact, source.title.as_deref(), yes)
        }
        Command::Marriage {
            first,
            second,
            date,
            place,
            source,
            yes,
        } => set_marriage(
            &mut store,
            [&first, &second],
            date.as_deref(),
            place.as_deref(),
            source.title.as_deref(),
            yes,
        ),
        Command::Relation { from, to, format } => relation(&store, &from, &to, format),
        Command::Check {
            only,
            all,
            fix,
            yes,
            format,
        } => {
            if fix {
                fix_countries(&mut store, yes)
            } else {
                print_check(&store, &only, all, format)
            }
        }
        Command::Html {
            target,
            parents,
            children,
            output,
        } => html(&store, &target, parents, children, &output),
        Command::Compare { other, format } => print_compare(&store, other, format),
        Command::Summary { format } => print_summary(&store, format),
        Command::Submitters {
            claim: true, yes, ..
        } => claim_unlinked(&mut store, yes),
        Command::Submitters {
            query,
            count,
            format,
            ..
        } => print_submitters(&store, &query, count, format),
        Command::Media {
            action: MediaCommand::Fetch { dry_run, dir },
        } => fetch_media(&mut store, dry_run, &dir),
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

fn claim_unlinked(store: &mut Store, yes: bool) -> Result<()> {
    let mut doc = store.doc.clone();
    let (exporter, claimed) = submitters::claim_unlinked(&mut doc)?;
    if claimed.is_empty() {
        println!("everyone already has a submitter");
        return Ok(());
    }
    let name = submitters::name_of(&doc, &exporter);
    println!(
        "{} people without a submitter will be linked to {name} {exporter}",
        claimed.len()
    );
    if !yes && !confirm(on_terminal())? {
        println!("not written");
        return Ok(());
    }
    store.doc = doc;
    store.save()?;
    println!(
        "claimed {} people, wrote {}",
        claimed.len(),
        store.path.display()
    );
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

fn relation(store: &Store, from: &str, to: &str, format: Option<Format>) -> Result<()> {
    let format = format.unwrap_or(if std::io::stdout().is_terminal() {
        Format::Tree
    } else {
        Format::List
    });
    let people = People::new(&store.doc);
    let interactive = on_terminal() && format.is_visual();
    let pick = |query: &str| {
        let target = Target {
            query: vec![query.to_owned()],
            pick: None,
        };
        find(&people, &target, interactive, format == Format::Json)
            .with_context(|| format!("for {query:?}"))
    };
    let (a, b) = (pick(from)?, pick(to)?);
    let kinship = kinship::relate(&people, &a.id, &b.id);
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&kinship)?),
        Format::List => print!("{}", kinship::list(&kinship)),
        _ => print!("{}", kinship::render(&kinship)),
    }
    Ok(())
}

fn print_check(
    store: &Store,
    only: &[check::Kind],
    all: bool,
    format: Option<Format>,
) -> Result<()> {
    let people = People::new(&store.doc);
    let mut issues = check::run(&people);
    if !only.is_empty() {
        issues.retain(|i| only.contains(&i.kind));
    }
    let format = format.unwrap_or(if std::io::stdout().is_terminal() {
        Format::Tree
    } else {
        Format::List
    });
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&issues)?),
        Format::List => print!("{}", check::list(&issues)),
        _ => print!("{}", check::render(&issues, all)),
    }
    Ok(())
}

fn fix_countries(store: &mut Store, yes: bool) -> Result<()> {
    let mut doc = store.doc.clone();
    let changed = check::fix_countries(&mut doc);
    if changed.is_empty() {
        println!("no country spellings to fix");
        return Ok(());
    }
    let issues: Vec<check::Issue> = check::run(&People::new(&store.doc))
        .into_iter()
        .filter(|i| i.kind == check::Kind::CountrySpelling)
        .collect();
    print!("{}", check::render(&issues, true));
    if !yes && !confirm(on_terminal())? {
        println!("not written");
        return Ok(());
    }
    store.doc = doc;
    store.save()?;
    println!(
        "fixed {} people, wrote {}",
        changed.len(),
        store.path.display()
    );
    Ok(())
}

fn html(
    store: &Store,
    target: &Target,
    parents: Option<u32>,
    children: Option<u32>,
    output: &std::path::Path,
) -> Result<()> {
    let people = People::new(&store.doc);
    let person = find(&people, target, on_terminal(), false)?;
    let parents = parents.or(if children.is_none() { Some(10) } else { None });
    let ancestors = parents.map(|n| relatives::ancestors(&people, &person.id, n));
    let descendants = children.map(|n| relatives::descendants(&people, &person.id, n));
    let page = html::page(&people, &person, ancestors.as_ref(), descendants.as_ref());
    store::write_atomic(output, page.as_bytes())?;
    println!("wrote {}", output.display());
    Ok(())
}

fn fetch_media(store: &mut Store, dry_run: bool, dir: &str) -> Result<()> {
    let remotes = media::remote_media(&store.doc);
    let files: usize = remotes.iter().map(|r| r.files.len()).sum();
    println!("{files} web file(s) in {} media link(s)", remotes.len());
    if dry_run {
        for remote in &remotes {
            for file in &remote.files {
                println!("{}\t{dir}/{}\t{}", remote.owner_name, file.local, file.url);
            }
        }
        return Ok(());
    }
    let folder = media::media_dir(&store.path, dir);
    let outcome = media::fetch(&mut store.doc, &folder, dir, &mut |line| {
        eprintln!("{line}")
    })?;
    if outcome.converted > 0 {
        store.save()?;
    }
    println!(
        "downloaded {}, already there {}, failed {}; {} link(s) now point to {}",
        outcome.downloaded,
        outcome.already_there,
        outcome.failed.len(),
        outcome.converted,
        folder.display()
    );
    for (who, why) in &outcome.failed {
        println!("  failed for {who}: {why}");
    }
    Ok(())
}

fn print_compare(store: &Store, other: PathBuf, format: Option<Format>) -> Result<()> {
    let theirs = Store::open(other)?;
    let comparison = compare::compare(store, &theirs);
    let format = format.unwrap_or(if std::io::stdout().is_terminal() {
        Format::Tree
    } else {
        Format::List
    });
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&comparison)?),
        Format::List => print!("{}", compare::list(&comparison)),
        _ => print!("{}", compare::render(&comparison)),
    }
    Ok(())
}

fn add_relative(
    store: &mut Store,
    anchor: &str,
    relation: add::Relation,
    person: &add::NewPerson,
    with_partner: Option<&str>,
    source: Option<&str>,
    yes: bool,
) -> Result<()> {
    let interactive = on_terminal();
    let plan = {
        let people = People::new(&store.doc);
        let pick = |query: &str| {
            let target = Target {
                query: vec![query.to_owned()],
                pick: None,
            };
            find(&people, &target, interactive, false).with_context(|| format!("for {query:?}"))
        };
        let anchor = pick(anchor)?;
        let family = match with_partner {
            Some(partner) => Some(family_with(&people, &anchor, &pick(partner)?)?),
            None => choose_family(&people, &anchor, interactive)?,
        };
        add::plan(&people, &anchor, relation, person, family)?
    };
    for line in &plan.lines {
        println!("{line}");
    }
    for warning in &plan.warnings {
        println!("warning: {warning}");
    }
    if !yes && !confirm(interactive)? {
        println!("not written");
        return Ok(());
    }
    let xref = add::apply(&mut store.doc, &plan, person)?;
    cite(&mut store.doc, &xref, source)?;
    store.save()?;
    println!("added {xref}, wrote {}", store.path.display());
    Ok(())
}

fn cite(doc: &mut gedcomkit::Document, xref: &str, title: Option<&str>) -> Result<()> {
    if let Some(title) = title {
        let source = source::ensure(doc, title);
        source::cite(doc.record_mut(xref).context("record vanished")?, &source);
    }
    Ok(())
}

fn add_fact(
    store: &mut Store,
    target: &Target,
    fact: fact::Fact,
    source: Option<&str>,
    yes: bool,
) -> Result<()> {
    let interactive = on_terminal();
    let fact = fact::checked(fact, store.is_v7())?;
    let (person, mut record) = {
        let people = People::new(&store.doc);
        let person = find(&people, target, interactive, false)?;
        let record = people
            .record(&person.id)
            .context("record vanished")?
            .clone();
        (person, record)
    };
    if !fact::add(&mut record, &fact) {
        println!("already recorded for {}: {}", person.name, fact.describe());
        return Ok(());
    }
    println!("{person}\n  + {}", fact.describe());
    if !yes && !confirm(interactive)? {
        println!("not written");
        return Ok(());
    }
    if let Some(title) = source {
        source::cite(&mut record, &source::ensure(&mut store.doc, title));
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

fn set_marriage(
    store: &mut Store,
    partners: [&str; 2],
    date: Option<&str>,
    place: Option<&str>,
    source: Option<&str>,
    yes: bool,
) -> Result<()> {
    if date.is_none() && place.is_none() {
        bail!("give --date and/or --place");
    }
    let interactive = on_terminal();
    let couple = {
        let people = People::new(&store.doc);
        let pick = |query: &str| {
            let target = Target {
                query: vec![query.to_owned()],
                pick: None,
            };
            find(&people, &target, interactive, false).with_context(|| format!("for {query:?}"))
        };
        marriage::couple(&people, pick(partners[0])?, pick(partners[1])?)
    };
    let family = couple.family.as_deref().unwrap_or("a new family");
    println!(
        "marriage of {} and {} in {family}",
        couple.first.name, couple.second.name
    );
    if !yes && !confirm(interactive)? {
        println!("not written");
        return Ok(());
    }
    let change = marriage::apply(&mut store.doc, &couple, date, place)?;
    cite(&mut store.doc, &change.family, source)?;
    store.save()?;
    let shown = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".to_owned());
    for (label, (before, after)) in ["date", "place"]
        .iter()
        .zip(change.before.iter().zip(&change.after))
    {
        if before != after {
            println!("  marriage.{label}  {} → {}", shown(before), shown(after));
        }
    }
    println!("wrote {}", store.path.display());
    Ok(())
}

fn family_with(people: &People, anchor: &Brief, partner: &Brief) -> Result<String> {
    people
        .unions(&anchor.id)
        .into_iter()
        .find(|u| {
            u.partner
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(&partner.id))
        })
        .map(|u| u.family)
        .with_context(|| {
            format!(
                "{} and {} have no family together; add the partner first",
                anchor.name, partner.name
            )
        })
}

fn choose_family(people: &People, anchor: &Brief, interactive: bool) -> Result<Option<String>> {
    let unions = people.unions(&anchor.id);
    if unions.len() <= 1 {
        return Ok(unions.into_iter().next().map(|u| u.family));
    }
    let labels: Vec<String> = unions
        .iter()
        .map(|u| {
            let partner = u.partner.as_deref().map_or_else(
                || "unknown partner".to_owned(),
                |p| people.brief(p).to_string(),
            );
            format!("with {partner} ({} children)", u.children.len())
        })
        .collect();
    if !interactive {
        bail!(
            "{} has several families; pass --with one of: {}",
            anchor.name,
            labels.join("; ")
        );
    }
    let index = dialoguer::Select::new()
        .with_prompt("Which family")
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(Some(unions[index].family.clone()))
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
    all: bool,
    ids: bool,
}

enum Shown {
    Text(String),
    Json(serde_json::Value),
}

fn show(store: &Store, target: &Target, wanted: Relatives, format: Option<Format>) -> Result<()> {
    if wanted.count && wanted.children.is_none() && wanted.parents.is_none() {
        bail!("--count needs --parents and/or --children");
    }
    let format = format.unwrap_or(if std::io::stdout().is_terminal() {
        Format::Tree
    } else {
        Format::List
    });
    let people = People::new(&store.doc);
    let persons = if wanted.all {
        let found = search::find(&people, &target.query.join(" "));
        if found.is_empty() {
            bail!("nobody matches");
        }
        found
    } else {
        let interactive = on_terminal() && format.is_visual();
        vec![find(&people, target, interactive, format == Format::Json)?]
    };
    let shown = persons
        .iter()
        .map(|person| show_one(&people, person, &wanted, format))
        .collect::<Result<Vec<_>>>()?;
    print_shown(&persons, shown, wanted.all, format)
}

fn print_shown(persons: &[Brief], shown: Vec<Shown>, all: bool, format: Format) -> Result<()> {
    let mut texts = Vec::new();
    let mut values = Vec::new();
    for (person, item) in persons.iter().zip(shown) {
        match item {
            Shown::Json(value) => values.push(value),
            Shown::Text(text) if all => texts.push(format!("{}\n{text}", heading(person, format))),
            Shown::Text(text) => texts.push(text),
        }
    }
    if format == Format::Json {
        let value = if all {
            serde_json::Value::Array(values)
        } else {
            values.remove(0)
        };
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    let text = texts.join("\n");
    print!("{text}");
    Ok(())
}

fn heading(person: &Brief, format: Format) -> String {
    match format {
        Format::List => format!("# {}\t{}", person.name, person.id),
        _ => format!("══ {person} ══"),
    }
}

fn show_one(people: &People, person: &Brief, wanted: &Relatives, format: Format) -> Result<Shown> {
    if wanted.children.is_none() && wanted.parents.is_none() {
        return show_card(people, person, format);
    }
    if wanted.count {
        return show_counts(people, person, wanted, format);
    }
    let ancestors = wanted
        .parents
        .map(|n| relatives::ancestors(people, &person.id, n));
    let descendants = wanted
        .children
        .map(|n| relatives::descendants(people, &person.id, n));
    let sections: Vec<String> = match format {
        Format::Json => {
            return Ok(Shown::Json(
                json!({ "person": person, "ancestors": ancestors, "descendants": descendants }),
            ));
        }
        Format::Tree => [ancestors, descendants]
            .iter()
            .flatten()
            .map(|rel| render::tree(rel, wanted.ids))
            .collect(),
        Format::List => [ancestors, descendants]
            .iter()
            .flatten()
            .map(render::list)
            .collect(),
    };
    Ok(Shown::Text(sections.join("\n")))
}

fn show_counts(
    people: &People,
    person: &Brief,
    wanted: &Relatives,
    format: Format,
) -> Result<Shown> {
    let tables: Vec<count::Counts> = [
        wanted.parents.map(|n| (n, count::Direction::Ancestors)),
        wanted.children.map(|n| (n, count::Direction::Descendants)),
    ]
    .into_iter()
    .flatten()
    .map(|(n, direction)| count::count(people, &person.id, n, direction))
    .collect();
    Ok(match format {
        Format::Json => Shown::Json(serde_json::to_value(&tables)?),
        Format::List => Shown::Text(tables.iter().map(count::list).collect()),
        _ => Shown::Text(
            tables
                .iter()
                .map(count::render)
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    })
}

fn show_card(people: &People, person: &Brief, format: Format) -> Result<Shown> {
    let record = people.record(&person.id).context("record vanished")?;
    let card = card::build(people, record);
    Ok(match format {
        Format::Json => Shown::Json(serde_json::to_value(&card)?),
        _ => Shown::Text(card::render(&card)),
    })
}

fn edit(
    store: &mut Store,
    target: &Target,
    assignments: &[String],
    source: Option<&str>,
    yes: bool,
) -> Result<()> {
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
    if let Some(title) = source {
        source::cite(&mut record, &source::ensure(&mut store.doc, title));
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
