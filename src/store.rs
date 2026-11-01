use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use gedcomkit::{Document, EncodingReport, GedcomEncoding};

const BOM: &[u8] = b"\xEF\xBB\xBF";

pub struct Store {
    pub path: PathBuf,
    pub doc: Document,
    pub encoding: String,
    pub size: u64,
    bom: bool,
    crlf: bool,
    damaged_text: Option<String>,
}

pub fn resolve(file: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(file) = file {
        return Ok(file);
    }
    let found: Vec<PathBuf> = fs::read_dir(".")?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| is_gedcom(path))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no .ged file in the current directory; pass --file or set GEDCLI_FILE"),
        _ => bail!("several .ged files in the current directory; pass --file or set GEDCLI_FILE"),
    }
}

fn is_gedcom(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ged"))
}

impl Store {
    pub fn open(path: PathBuf) -> Result<Self> {
        let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
        let (doc, report) =
            Document::from_bytes(&bytes).map_err(|err| anyhow!("{}: {err}", path.display()))?;
        Ok(Self {
            encoding: report.summary(),
            size: bytes.len() as u64,
            bom: bytes.starts_with(BOM),
            crlf: uses_crlf(&bytes),
            damaged_text: damaged_text(&report),
            path,
            doc,
        })
    }

    pub fn is_v7(&self) -> bool {
        self.doc.is_version_7_or_later()
    }

    pub fn version(&self) -> String {
        self.doc.version().unwrap_or_else(|| "unknown".to_owned())
    }

    pub fn line_endings(&self) -> &'static str {
        if self.crlf { "CRLF" } else { "LF" }
    }

    pub fn encode(&self, doc: &Document) -> Vec<u8> {
        let mut text = String::from_utf8(doc.to_bytes()).expect("gedcomkit writes UTF-8");
        if self.crlf {
            text = text.replace('\n', "\r\n");
        }
        let mut bytes = if self.bom { BOM.to_vec() } else { Vec::new() };
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }

    pub fn save(&self) -> Result<()> {
        if let Some(why) = &self.damaged_text {
            bail!(
                "{}: {why} Rewriting it would damage text in other records, so nothing was written; fix the file's encoding first",
                self.path.display()
            );
        }
        write_atomic(&self.path, &self.encode(&self.doc))
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", Path::new(&tmp).display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

fn damaged_text(report: &EncodingReport) -> Option<String> {
    let guessed = matches!(
        report.used,
        Some(GedcomEncoding::Ansi | GedcomEncoding::Ansel)
    ) && !report.warnings.is_empty();
    (guessed || report.undecodable_bytes > 0).then(|| report.warnings.join(" "))
}

fn uses_crlf(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .position(|&b| b == b'\n')
        .is_some_and(|i| i > 0 && bytes[i - 1] == b'\r')
}
