use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use gedcomkit::Node;
use gedcomkit::names::PersonalName;
use gedcomkit::view::IndividualView;
use serde::Serialize;

use crate::person::{Brief, People};
use crate::search::normalize;
use crate::store::Store;
use crate::submitters::{self, Tally};

const TOP: usize = 10;

#[derive(Serialize)]
pub struct Summary {
    file: FileInfo,
    records: BTreeMap<String, usize>,
    people: PeopleInfo,
    submitters: Tally,
    connections: Connections,
    quality: Quality,
}

#[derive(Serialize)]
struct FileInfo {
    path: String,
    size_bytes: u64,
    version: String,
    encoding: String,
    line_endings: String,
    producer: Option<String>,
    exported: Option<String>,
    exported_by: Option<String>,
}

#[derive(Serialize)]
struct PeopleInfo {
    total: usize,
    by_sex: BTreeMap<String, usize>,
    with_death_or_burial: usize,
    earliest_birth: Option<i32>,
    latest_birth: Option<i32>,
    surnames: Vec<(String, usize)>,
    countries: Vec<(String, usize)>,
}

#[derive(Serialize)]
struct Connections {
    families: usize,
    groups: Vec<usize>,
    isolated: usize,
    top_ancestors: usize,
    deepest_line: u32,
    deepest_line_from: Option<Brief>,
    most_descendants: Option<(Brief, usize)>,
    most_ancestors: Option<(Brief, usize)>,
    largest_family: Option<(Vec<Brief>, usize)>,
    average_children: f64,
}

#[derive(Serialize)]
struct Quality {
    unresolved_pointers: usize,
    without_name: usize,
    without_birth_date: usize,
    childless_families_without_partners: usize,
}

pub fn build(store: &Store) -> Summary {
    let people = People::new(&store.doc);
    let ids: Vec<String> = people
        .individuals()
        .filter_map(|r| r.xref().map(str::to_owned))
        .collect();
    let views: Vec<IndividualView> = people
        .individuals()
        .map(IndividualView::from_node)
        .collect();
    Summary {
        file: file_info(store),
        records: record_counts(store),
        people: people_info(&people, &views),
        submitters: submitters::tally(&store.doc),
        connections: connections(&people, &ids),
        quality: quality(store, &people, &views),
    }
}

fn file_info(store: &Store) -> FileInfo {
    let head = store.doc.header();
    FileInfo {
        path: store.path.display().to_string(),
        size_bytes: store.size,
        version: store.version(),
        encoding: store.encoding.clone(),
        line_endings: store.line_endings().to_owned(),
        producer: head.and_then(|h| h.value_of("SOUR")),
        exported: head.and_then(|h| h.value_of("DATE")),
        exported_by: submitters::exporter(&store.doc).map(|x| submitters::name_of(&store.doc, &x)),
    }
}

fn record_counts(store: &Store) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for record in &store.doc.records {
        if !matches!(record.tag(), "HEAD" | "TRLR") {
            *counts.entry(record.tag().to_owned()).or_default() += 1;
        }
    }
    counts
}

fn people_info(people: &People, views: &[IndividualView]) -> PeopleInfo {
    let mut by_sex = BTreeMap::new();
    for view in views {
        let sex = view.sex.clone().unwrap_or_else(|| "Unknown".to_owned());
        *by_sex.entry(sex).or_default() += 1;
    }
    let births: Vec<i32> = views.iter().filter_map(birth_year).collect();
    PeopleInfo {
        total: views.len(),
        by_sex,
        with_death_or_burial: views
            .iter()
            .filter(|v| {
                v.events
                    .iter()
                    .any(|e| matches!(e.tag.as_str(), "DEAT" | "BURI"))
            })
            .count(),
        earliest_birth: births.iter().min().copied(),
        latest_birth: births.iter().max().copied(),
        surnames: top(people.individuals().filter_map(surname)),
        countries: top(people.individuals().flat_map(countries)),
    }
}

fn birth_year(view: &IndividualView) -> Option<i32> {
    view.events
        .iter()
        .filter(|e| matches!(e.tag.as_str(), "BIRT" | "CHR" | "BAPM"))
        .find_map(|e| e.year)
}

fn surname(record: &Node) -> Option<String> {
    record
        .first("NAME")
        .and_then(|n| PersonalName::from_node(n).surname)
        .filter(|s| !s.trim().is_empty())
}

fn countries(record: &Node) -> Vec<String> {
    let mut found: Vec<String> = record
        .children
        .iter()
        .filter_map(|event| {
            let country = event.first("ADDR").and_then(|a| a.value_of("CTRY"));
            let from_place = || {
                event
                    .value_of("PLAC")
                    .and_then(|p| p.rsplit_once(',').map(|(_, last)| last.trim().to_owned()))
            };
            country.or_else(from_place).filter(|c| !c.is_empty())
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

fn top(items: impl Iterator<Item = String>) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, (String, usize)> = HashMap::new();
    for item in items {
        counts.entry(normalize(&item)).or_insert((item, 0)).1 += 1;
    }
    let mut sorted: Vec<(String, usize)> = counts.into_values().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted.truncate(TOP);
    sorted
}

fn connections(people: &People, ids: &[String]) -> Connections {
    let groups = groups(people, ids);
    let isolated = groups.iter().filter(|&&size| size == 1).count();
    let depths = line_depths(people, ids);
    let deepest = depths
        .iter()
        .max_by_key(|(id, d)| (**d, std::cmp::Reverse(*id)));
    let families = people.graph.families();
    let largest = families
        .iter()
        .max_by_key(|(key, m)| (m.children.len(), std::cmp::Reverse(*key)))
        .map(|(_, m)| {
            (
                m.partners.iter().map(|p| people.brief(p)).collect(),
                m.children.len(),
            )
        });
    let children: usize = families.values().map(|m| m.children.len()).sum();
    Connections {
        families: families.len(),
        groups: groups.into_iter().filter(|&size| size > 1).collect(),
        isolated,
        top_ancestors: ids
            .iter()
            .filter(|id| {
                people.graph.parents_of(id).is_empty() && !people.graph.children_of(id).is_empty()
            })
            .count(),
        deepest_line: deepest.map_or(0, |(_, d)| *d),
        deepest_line_from: deepest.map(|(id, _)| people.brief(id)),
        most_descendants: most(people, ids, |id| people.graph.descendants(id).len() - 1),
        most_ancestors: most(people, ids, |id| people.graph.ancestors(id).len() - 1),
        largest_family: largest,
        average_children: if families.is_empty() {
            0.0
        } else {
            round1(children as f64 / families.len() as f64)
        },
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn most(people: &People, ids: &[String], count: impl Fn(&str) -> usize) -> Option<(Brief, usize)> {
    ids.iter()
        .map(|id| (id, count(id)))
        .max_by_key(|(id, n)| (*n, std::cmp::Reverse(*id)))
        .filter(|(_, n)| *n > 0)
        .map(|(id, n)| (people.brief(id), n))
}

fn groups(people: &People, ids: &[String]) -> Vec<usize> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut sizes = Vec::new();
    for id in ids {
        if seen.contains(&id.to_ascii_uppercase()) {
            continue;
        }
        let mut size = 0;
        let mut stack = vec![id.clone()];
        while let Some(current) = stack.pop() {
            if !seen.insert(current.to_ascii_uppercase()) {
                continue;
            }
            size += 1;
            let graph = &people.graph;
            for next in [
                graph.parents_of(&current),
                graph.children_of(&current),
                graph.partners_of(&current),
            ]
            .into_iter()
            .flatten()
            {
                stack.push(next.clone());
            }
        }
        sizes.push(size);
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

/// Generations in the longest parent-to-child line starting at each person.
fn line_depths(people: &People, ids: &[String]) -> HashMap<String, u32> {
    let mut depths = HashMap::new();
    for id in ids {
        depth(people, id, &mut depths, &mut HashSet::new());
    }
    depths
}

fn depth(
    people: &People,
    id: &str,
    memo: &mut HashMap<String, u32>,
    path: &mut HashSet<String>,
) -> u32 {
    if let Some(&d) = memo.get(id) {
        return d;
    }
    if !path.insert(id.to_ascii_uppercase()) {
        return 0;
    }
    let below = people
        .graph
        .children_of(id)
        .to_vec()
        .iter()
        .map(|child| depth(people, child, memo, path))
        .max()
        .unwrap_or(0);
    path.remove(&id.to_ascii_uppercase());
    memo.insert(id.to_owned(), below + 1);
    below + 1
}

fn quality(store: &Store, people: &People, views: &[IndividualView]) -> Quality {
    Quality {
        unresolved_pointers: store.doc.unresolved_pointers().len(),
        without_name: views.iter().filter(|v| v.names.is_empty()).count(),
        without_birth_date: views.iter().filter(|v| birth_year(v).is_none()).count(),
        childless_families_without_partners: people
            .graph
            .families()
            .values()
            .filter(|m| m.children.is_empty() && m.partners.len() < 2)
            .count(),
    }
}

pub fn render(s: &Summary) -> String {
    let mut out = String::new();
    let f = &s.file;
    let _ = writeln!(out, "File          {} ({} KB)", f.path, f.size_bytes / 1024);
    let _ = writeln!(
        out,
        "Format        GEDCOM {}, {}, {} line endings",
        f.version, f.encoding, f.line_endings
    );
    if let Some(producer) = &f.producer {
        let date = f
            .exported
            .as_deref()
            .map(|d| format!(", exported {d}"))
            .unwrap_or_default();
        let by = f
            .exported_by
            .as_deref()
            .map(|n| format!(" by {n}"))
            .unwrap_or_default();
        let _ = writeln!(out, "Producer      {producer}{date}{by}");
    }
    let records: Vec<String> = s
        .records
        .iter()
        .map(|(tag, n)| format!("{n} {tag}"))
        .collect();
    let _ = writeln!(out, "Records       {}", records.join(", "));

    let p = &s.people;
    let _ = writeln!(out, "\nPeople        {}", p.total);
    let sexes: Vec<String> = p
        .by_sex
        .iter()
        .map(|(sex, n)| format!("{n} {}", sex.to_lowercase()))
        .collect();
    let _ = writeln!(out, "  by sex      {}", sexes.join(", "));
    let _ = writeln!(
        out,
        "  deceased    {} with a death or burial recorded",
        p.with_death_or_burial
    );
    if let (Some(first), Some(last)) = (p.earliest_birth, p.latest_birth) {
        let _ = writeln!(out, "  born        {first} – {last}");
    }
    let _ = writeln!(out, "  surnames    {}", counted(&p.surnames));
    let _ = writeln!(out, "  countries   {}", counted(&p.countries));

    let t = &s.submitters;
    let top: Vec<String> = t
        .contributors
        .iter()
        .take(5)
        .map(|c| format!("{} ({})", c.name, c.people))
        .collect();
    let _ = writeln!(
        out,
        "\nSubmitters    {} accounts; top: {}",
        t.contributors.len(),
        top.join(", ")
    );
    let _ = writeln!(
        out,
        "  unlinked    {} people with no submitter (the exporter's own profiles); see `gedcli submitters`",
        t.unlinked_people
    );

    let c = &s.connections;
    let _ = writeln!(out, "\nConnections");
    let _ = writeln!(
        out,
        "  families    {} (avg {} children)",
        c.families, c.average_children
    );
    let groups: Vec<String> = c.groups.iter().map(ToString::to_string).collect();
    let _ = writeln!(
        out,
        "  groups      {} connected group(s) of {} people; {} isolated",
        c.groups.len(),
        groups.join(" + "),
        c.isolated
    );
    let _ = writeln!(
        out,
        "  top of tree {} ancestors with no parents recorded",
        c.top_ancestors
    );
    if let Some(from) = &c.deepest_line_from {
        let _ = writeln!(
            out,
            "  deepest     {} generations, from {from}",
            c.deepest_line
        );
    }
    if let Some((who, n)) = &c.most_descendants {
        let _ = writeln!(out, "  most desc.  {n} descendants: {who}");
    }
    if let Some((who, n)) = &c.most_ancestors {
        let _ = writeln!(out, "  most anc.   {n} known ancestors: {who}");
    }
    if let Some((parents, n)) = &c.largest_family {
        let names: Vec<String> = parents.iter().map(|p| p.name.clone()).collect();
        let _ = writeln!(out, "  largest     {n} children of {}", names.join(" & "));
    }

    let q = &s.quality;
    let _ = writeln!(out, "\nQuality");
    let _ = writeln!(out, "  broken references   {}", q.unresolved_pointers);
    let _ = writeln!(out, "  without a name      {}", q.without_name);
    let _ = writeln!(out, "  without birth year  {}", q.without_birth_date);
    let _ = writeln!(
        out,
        "  families with <2 partners and no children {}",
        q.childless_families_without_partners
    );
    out
}

fn counted(items: &[(String, usize)]) -> String {
    items
        .iter()
        .map(|(name, n)| format!("{name} ({n})"))
        .collect::<Vec<_>>()
        .join(", ")
}
