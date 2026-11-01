use std::fmt::Write as _;

use clap::ValueEnum;
use termtree::Tree;

use crate::person::Brief;
use crate::relatives::Rel;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Tree,
    Chart,
    List,
    Json,
}

impl Format {
    pub fn is_visual(self) -> bool {
        matches!(self, Self::Tree | Self::Chart)
    }
}

pub fn tree(rel: &Rel, ids: bool) -> String {
    to_tree(rel, true, ids).to_string()
}

fn to_tree(rel: &Rel, root: bool, ids: bool) -> Tree<String> {
    let mut node = Tree::new(label(rel, root, ids));
    for parent in &rel.parents {
        node.push(to_tree(parent, false, ids));
    }
    for family in &rel.families {
        let children = family.children.iter().map(|c| to_tree(c, false, ids));
        match &family.partner {
            None if rel.families.len() == 1 => {
                node.leaves.extend(children);
            }
            partner => {
                let name = partner
                    .as_ref()
                    .map_or_else(|| "unknown partner".to_owned(), |p| styled(p, ids));
                node.push(Tree::new(format!("⚭ {name}")).with_leaves(children));
            }
        }
    }
    node
}

fn label(rel: &Rel, root: bool, ids: bool) -> String {
    let relation = if !root && is_parent(&rel.relation) {
        format!("{}: ", rel.relation)
    } else {
        String::new()
    };
    let (mark, suffix) = if rel.repeat {
        ("↺ ", " (see above)")
    } else {
        ("", "")
    };
    format!("{mark}{relation}{}{suffix}", styled(&rel.person, ids))
}

fn sex_style(sex: Option<&str>) -> console::Style {
    let style = console::Style::new().bold();
    match sex {
        Some("M") => style.color256(39),
        Some("F") => style.color256(213),
        _ => style.white(),
    }
}

fn styled(person: &Brief, ids: bool) -> String {
    let life = person
        .life
        .as_ref()
        .map(|l| format!(" ({l})"))
        .unwrap_or_default();
    let id = if ids {
        format!(" {}", person.id)
    } else {
        String::new()
    };
    format!(
        "{}{}{}",
        sex_style(person.sex.as_deref()).apply_to(&person.name),
        console::style(life).dim(),
        console::style(id).dim()
    )
}

fn is_parent(relation: &str) -> bool {
    matches!(relation, "father" | "mother" | "parent")
}

pub fn list(rel: &Rel) -> String {
    let mut out = String::from("# gen\tpath\tname\tlife\tid\n");
    list_into(&mut out, rel, 0, "");
    out
}

fn list_into(out: &mut String, rel: &Rel, generation: u32, parent_path: &str) {
    let path = join(parent_path, &rel.relation);
    let shown = if rel.repeat {
        format!("{path} (repeat)")
    } else {
        path.clone()
    };
    row(out, generation, &shown, &rel.person);
    for parent in &rel.parents {
        list_into(out, parent, generation + 1, &path);
    }
    for family in &rel.families {
        if let Some(partner) = &family.partner {
            row(out, generation, &join(&path, "partner"), partner);
        }
        for child in &family.children {
            list_into(out, child, generation + 1, &path);
        }
    }
}

fn join(parent: &str, relation: &str) -> String {
    match parent {
        "" => relation.to_owned(),
        "self" => relation.to_owned(),
        _ => format!("{parent}>{relation}"),
    }
}

fn row(out: &mut String, generation: u32, path: &str, person: &Brief) {
    let life = person.life.as_deref().unwrap_or("");
    let _ = writeln!(
        out,
        "{generation}\t{path}\t{}\t{life}\t{}",
        person.name, person.id
    );
}
