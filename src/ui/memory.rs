use std::fs;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "last-person";

/// The person who was in the centre when the UI last closed on this file.
pub fn remembered(ged: &Path) -> Option<String> {
    remembered_in(&state_file()?, ged)
}

/// Remembers who is in the centre for this file, keeping other files' entries.
pub fn remember(ged: &Path, xref: &str) -> std::io::Result<()> {
    match state_file() {
        Some(path) => remember_in(&path, ged, xref),
        None => Ok(()),
    }
}

fn remembered_in(state: &Path, ged: &Path) -> Option<String> {
    let key = key(ged)?;
    let text = fs::read_to_string(state).ok()?;
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .find(|(file, _)| *file == key)
        .map(|(_, xref)| xref.to_owned())
}

fn remember_in(path: &Path, ged: &Path, xref: &str) -> std::io::Result<()> {
    let Some(key) = key(ged) else {
        return Ok(());
    };
    let mut lines: Vec<String> = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.split_once('\t').is_some_and(|(file, _)| file != key))
        .map(str::to_owned)
        .collect();
    lines.push(format!("{key}\t{xref}"));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, lines.join("\n") + "\n")
}

fn key(ged: &Path) -> Option<String> {
    fs::canonicalize(ged).ok().map(|p| p.display().to_string())
}

fn state_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(base.join("gedcli").join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_one_person_per_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let state = dir.path().join("state/last-person");
        let (a, b) = (dir.path().join("a.ged"), dir.path().join("b.ged"));
        fs::write(&a, "").unwrap();
        fs::write(&b, "").unwrap();
        assert_eq!(remembered_in(&state, &a), None);
        remember_in(&state, &a, "@I1@").unwrap();
        remember_in(&state, &b, "@I2@").unwrap();
        remember_in(&state, &a, "@I3@").unwrap();
        assert_eq!(remembered_in(&state, &a).as_deref(), Some("@I3@"));
        assert_eq!(remembered_in(&state, &b).as_deref(), Some("@I2@"));
        assert_eq!(fs::read_to_string(&state).unwrap().lines().count(), 2);
    }
}
