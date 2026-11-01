use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use tempfile::TempDir;

fn workspace() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("family.ged");
    fs::copy("tests/family.ged", &file).unwrap();
    (dir, file)
}

fn gedcli(file: &PathBuf) -> Command {
    let mut cmd = Command::cargo_bin("gedcli").unwrap();
    cmd.env_remove("GEDCLI_FILE").arg("--file").arg(file);
    cmd
}

fn stdout(cmd: &mut Command) -> String {
    String::from_utf8(cmd.assert().success().get_output().stdout.clone()).unwrap()
}

#[test]
fn ambiguous_query_lists_candidates_and_exits_2() {
    let (_dir, file) = workspace();
    let out = gedcli(&file)
        .args(["show", "lund"])
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("2\tKari Lund\tb. 1950\t@I3@"));
    assert!(out.contains("@I2@"));
}

#[test]
fn accents_are_ignored() {
    let (_dir, file) = workspace();
    let out = stdout(gedcli(&file).args(["show", "ase"]));
    assert!(out.starts_with("Åse Lund (b. 1920) @I1@"));
}

#[test]
fn descendants_as_list() {
    let (_dir, file) = workspace();
    let out = stdout(gedcli(&file).args(["show", "@I1@", "--children", "--format", "list"]));
    assert_eq!(
        out,
        "# gen\tpath\tname\tlife\tid\n\
         0\tself\tÅse Lund\tb. 1920\t@I1@\n\
         0\tpartner\tPer Lund\t\t@I2@\n\
         1\tdaughter\tKari Lund\tb. 1950\t@I3@\n\
         2\tdaughter>son\tOla Berg\t\t@I4@\n"
    );
}

#[test]
fn generations_limit_depth() {
    let (_dir, file) = workspace();
    let out = stdout(gedcli(&file).args(["show", "ola", "--parents", "1", "--format", "tree"]));
    assert_eq!(out, "Ola Berg @I4@\n└── mother: Kari Lund (b. 1950) @I3@\n");
}

#[test]
fn unchanged_export_is_byte_identical() {
    let (dir, file) = workspace();
    let out = dir.path().join("out.ged");
    gedcli(&file)
        .args(["export", "--to", "5.5.1", "-o"])
        .arg(&out)
        .assert()
        .success();
    assert_eq!(fs::read(&out).unwrap(), fs::read(&file).unwrap());
}

#[test]
fn edit_with_set_changes_only_that_person() {
    let (_dir, file) = workspace();
    let before = fs::read_to_string(&file).unwrap();
    gedcli(&file)
        .args(["edit", "kari", "--set", "birth.place=Bergen", "--yes"])
        .assert()
        .success();
    let after = fs::read_to_string(&file).unwrap();
    assert!(after.contains("2 DATE 1950\r\n2 PLAC Bergen\r\n"));
    assert!(after.contains("0 @I3@ INDI\r\n") && after.contains("1 CHAN\r\n"));
    assert_eq!(before.lines().count() + 4, after.lines().count());
}

#[test]
fn edit_without_yes_off_terminal_refuses() {
    let (_dir, file) = workspace();
    let before = fs::read(&file).unwrap();
    gedcli(&file)
        .args(["edit", "kari", "--set", "sex=M"])
        .assert()
        .failure();
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn migrate_converts_to_v7() {
    let (_dir, file) = workspace();
    gedcli(&file).arg("migrate").assert().success();
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("2 VERS 7.0\r\n"));
    assert!(text.contains("1 EXID 111\r\n2 TYPE https://www.geni.com/\r\n"));
    assert!(text.contains("2 DATE 1 MAY 1920\r\n1 FAMS"));
    assert!(!text.contains("CHAR"));
}

#[test]
fn summary_counts_people_and_connections() {
    let (_dir, file) = workspace();
    let out = stdout(gedcli(&file).arg("summary"));
    assert!(out.contains("Records       2 FAM, 4 INDI"));
    assert!(out.contains("People        4"));
    assert!(out.contains("1 connected group(s) of 4 people; 0 isolated"));
    assert!(out.contains("deepest     3 generations"));
    let json = stdout(gedcli(&file).args(["summary", "--format", "json"]));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["people"]["total"], 4);
}
