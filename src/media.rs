use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use gedcomkit::{Document, Node};

use crate::add;
use crate::person::brief_of;

const MEDIA_PREFIX: &str = "OG";
const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(60);

/// A media link written inside a record (the GEDCOM 5.5.1 way) whose files are on the web.
pub struct Remote {
    pub owner: String,
    pub owner_name: String,
    path: Vec<usize>,
    pub files: Vec<RemoteFile>,
}

pub struct RemoteFile {
    pub url: String,
    pub local: String,
}

pub struct Outcome {
    pub downloaded: usize,
    pub already_there: usize,
    pub converted: usize,
    pub failed: Vec<(String, String)>,
}

/// Every embedded media link, at any depth, with at least one http(s) file, and the local name
/// each file will get (one name per distinct address).
pub fn remote_media(doc: &Document) -> Vec<Remote> {
    let mut names = Names::default();
    let mut found = Vec::new();
    for record in doc
        .records
        .iter()
        .filter(|r| !matches!(r.tag(), "HEAD" | "TRLR" | "OBJE"))
    {
        let Some(owner) = record.xref() else { continue };
        let owner_name = if record.tag() == "INDI" {
            brief_of(record).name
        } else {
            owner.to_owned()
        };
        let mut paths = Vec::new();
        embedded_links(record, &mut Vec::new(), &mut paths);
        for path in paths {
            let node = record.at(&path).expect("path from walk");
            let files: Vec<RemoteFile> = node
                .all("FILE")
                .map(Node::logical_value)
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                .map(|url| RemoteFile {
                    local: names.local(owner, &url),
                    url,
                })
                .collect();
            if !files.is_empty() {
                found.push(Remote {
                    owner: owner.to_owned(),
                    owner_name: owner_name.clone(),
                    path,
                    files,
                });
            }
        }
    }
    found
}

fn embedded_links(node: &Node, path: &mut Vec<usize>, found: &mut Vec<Vec<usize>>) {
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if child.tag() == "OBJE" && child.pointer().is_none() {
            found.push(path.clone());
        } else {
            embedded_links(child, path, found);
        }
        path.pop();
    }
}

#[derive(Default)]
struct Names {
    by_url: HashMap<String, String>,
    taken: HashSet<String>,
}

impl Names {
    fn local(&mut self, owner: &str, url: &str) -> String {
        if let Some(name) = self.by_url.get(url) {
            return name.clone();
        }
        let name = local_name(owner, url, &mut self.taken);
        self.by_url.insert(url.to_owned(), name.clone());
        name
    }
}

/// Downloads every remote file into `dir` (skipping files already there), then turns each fully
/// downloaded link into a media record pointing at the local copy.
pub fn fetch(
    doc: &mut Document,
    dir: &Path,
    relative_dir: &str,
    report: &mut dyn FnMut(&str),
) -> Result<Outcome> {
    let remotes = remote_media(doc);
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let mut outcome = Outcome {
        downloaded: 0,
        already_there: 0,
        converted: 0,
        failed: Vec::new(),
    };
    let total: usize = remotes.iter().map(|r| r.files.len()).sum();
    let mut done = 0;
    let mut ready = Vec::new();
    for remote in remotes {
        let mut complete = true;
        for file in &remote.files {
            done += 1;
            let path = dir.join(&file.local);
            if path.exists() {
                outcome.already_there += 1;
                continue;
            }
            match download(&agent, &file.url, &path) {
                Ok(bytes) => {
                    outcome.downloaded += 1;
                    report(&format!(
                        "[{done}/{total}] {} ({} KB)",
                        file.local,
                        bytes / 1024
                    ));
                }
                Err(err) => {
                    complete = false;
                    report(&format!("[{done}/{total}] FAILED {}: {err:#}", file.local));
                    outcome
                        .failed
                        .push((remote.owner_name.clone(), format!("{}: {err:#}", file.url)));
                }
            }
        }
        if complete {
            ready.push(remote);
        }
    }
    for remote in &ready {
        convert(doc, remote, relative_dir)?;
        outcome.converted += 1;
    }
    Ok(outcome)
}

fn download(agent: &ureq::Agent, url: &str, path: &Path) -> Result<u64> {
    let bytes = agent
        .get(url)
        .call()?
        .body_mut()
        .with_config()
        .limit(MAX_FILE_BYTES)
        .read_to_vec()?;
    let partial = path.with_extension("part");
    fs::write(&partial, &bytes)?;
    fs::rename(&partial, path)?;
    Ok(bytes.len() as u64)
}

/// Replaces the embedded link with a pointer to a new media record whose files are the local copies.
fn convert(doc: &mut Document, remote: &Remote, relative_dir: &str) -> Result<()> {
    let xref = doc.next_xref(MEDIA_PREFIX);
    let owner = doc.record_mut(&remote.owner).context("record vanished")?;
    let slot = owner.at_mut(&remote.path).context("media link vanished")?;
    let inline = slot.clone();
    let mut record = Node::record(&xref, "OBJE");
    let mut link = Node::with_value("OBJE", &xref);
    for mut child in inline.children {
        forget_lines(&mut child);
        match child.tag() {
            "FILE" => {
                if let Some(file) = remote.files.iter().find(|f| f.url == child.logical_value()) {
                    child.set_logical_value(&format!("{relative_dir}/{}", file.local));
                }
                if child.first("TITL").is_none() {
                    child.push(Node::with_value("TITL", &remote.owner_name));
                }
                record.push(child);
            }
            "_PRIM" | "CROP" => link.push(child),
            _ => record.push(child),
        }
    }
    *slot = link;
    add::insert_record(doc, "OBJE", record);
    Ok(())
}

/// A node moved to another depth must be rendered afresh, children included.
fn forget_lines(node: &mut Node) {
    node.forget_verbatim();
    for child in &mut node.children {
        forget_lines(child);
    }
}

fn local_name(owner: &str, url: &str, taken: &mut HashSet<String>) -> String {
    let file = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap_or("file");
    let clean: String = file
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = format!(
        "{}-{}",
        owner.trim_matches('@'),
        if clean.is_empty() {
            "file".to_owned()
        } else {
            clean
        }
    );
    let mut name = base.clone();
    let mut n = 2;
    while !taken.insert(name.to_ascii_lowercase()) {
        name = match base.rsplit_once('.') {
            Some((stem, ext)) => format!("{stem}-{n}.{ext}"),
            None => format!("{base}-{n}"),
        };
        n += 1;
    }
    name
}

pub fn media_dir(ged: &Path, relative_dir: &str) -> PathBuf {
    ged.parent().unwrap_or(Path::new(".")).join(relative_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_names_are_unique_and_safe() {
        let mut taken = HashSet::new();
        let url = "https://media.geni.com/p13/aa/img 0023_original.jpg?hash=abc.123";
        assert_eq!(
            local_name("@I1@", url, &mut taken),
            "I1-img_0023_original.jpg"
        );
        assert_eq!(
            local_name("@I1@", url, &mut taken),
            "I1-img_0023_original-2.jpg"
        );
    }

    #[test]
    fn links_inside_citations_are_found_and_shared_files_named_once() {
        let mut doc = Document::parse(
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ann /Lee/\n1 SOUR @S1@\n2 OBJE\n3 FILE https://x.org/scan.png?h=1\n\
             1 SOUR @S1@\n2 OBJE\n3 FILE https://x.org/scan.png?h=1\n0 @S1@ SOUR\n0 TRLR\n",
        )
        .unwrap();
        let remotes = remote_media(&doc);
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0].files[0].local, remotes[1].files[0].local);
        for remote in &remotes {
            convert(&mut doc, remote, "media").unwrap();
        }
        let text = doc.to_text();
        assert!(
            text.contains("1 SOUR @S1@\n2 OBJE @OG1@\n1 SOUR @S1@\n2 OBJE @OG2@\n"),
            "{text}"
        );
        assert!(remote_media(&doc).is_empty());
    }

    #[test]
    fn embedded_link_becomes_a_media_record() {
        let mut doc = Document::parse(
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ann /Lee/\n1 OBJE\n2 FILE https://x.org/a.jpg?h=1\n3 FORM image/jpeg\n2 _PRIM Y\n1 CHAN\n2 DATE 1 JAN 2020\n0 TRLR\n",
        )
        .unwrap();
        let remotes = remote_media(&doc);
        assert_eq!(remotes.len(), 1);
        convert(&mut doc, &remotes[0], "media").unwrap();
        assert_eq!(
            doc.to_text(),
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ann /Lee/\n1 OBJE @OG1@\n2 _PRIM Y\n1 CHAN\n2 DATE 1 JAN 2020\n\
             0 @OG1@ OBJE\n1 FILE media/I1-a.jpg\n2 FORM image/jpeg\n2 TITL Ann Lee\n0 TRLR\n"
        );
        assert!(remote_media(&doc).is_empty());
    }
}
