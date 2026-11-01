use std::fmt::Write as _;

use clap::ValueEnum;
use termtree::Tree;

use crate::person::Brief;
use crate::relatives::Rel;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Tree,
    List,
    Json,
}

pub fn tree(rel: &Rel) -> String {
    to_tree(rel, true).to_string()
}

fn to_tree(rel: &Rel, root: bool) -> Tree<String> {
    let mut node = Tree::new(label(rel, root));
    for parent in &rel.parents {
        node.push(to_tree(parent, false));
    }
    for family in &rel.families {
        let children = family.children.iter().map(|c| to_tree(c, false));
        match &family.partner {
            None if rel.families.len() == 1 => {
                node.leaves.extend(children);
            }
            partner => {
                let name = partner
                    .as_ref()
                    .map_or_else(|| "unknown partner".to_owned(), Brief::to_string);
                node.push(Tree::new(format!("⚭ {name}")).with_leaves(children));
            }
        }
    }
    node
}

fn label(rel: &Rel, root: bool) -> String {
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
    format!("{mark}{relation}{}{suffix}", rel.person)
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
