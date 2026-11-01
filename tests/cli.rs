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
    assert_eq!(out, "Ola Berg\n└── mother: Kari Lund (b. 1950)\n");
    let out =
        stdout(gedcli(&file).args(["show", "ola", "--parents", "1", "--format", "tree", "--ids"]));
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

#[test]
fn count_ancestors_per_generation() {
    let (_dir, file) = workspace();
    let out =
        stdout(gedcli(&file).args(["show", "ola", "--parents", "--count", "--format", "list"]));
    assert_eq!(
        out,
        "# gen\tname\tfound\tpossible\n1\tparents\t1\t2\n2\tgrandparents\t2\t4\ntotal\tancestors\t3\t\n"
    );
}

#[test]
fn ancestor_chart_has_root_at_bottom() {
    let (_dir, file) = workspace();
    let out = stdout(gedcli(&file).args(["show", "ola", "--parents", "--format", "chart"]));
    assert_eq!(
        out,
        "          Åse Lund\nPer Lund  b. 1920\n    └────┬────┘\n         │\n     Kari Lund\n      b. 1950\n         │\n         │\n     Ola Berg\n"
    );
}

#[test]
fn submitters_count_unlinked_people_for_exporter() {
    let (_dir, file) = workspace();
    let text = fs::read_to_string(&file).unwrap();
    let text = text
        .replacen("1 CHAR UTF-8\r\n", "1 CHAR UTF-8\r\n1 SUBM @S1@\r\n", 1)
        .replacen("1 RFN geni:111\r\n", "1 RFN geni:111\r\n1 SUBM @S2@\r\n", 1)
        .replacen(
            "0 TRLR",
            "0 @S1@ SUBM\r\n1 NAME Me\r\n0 @S2@ SUBM\r\n1 NAME Other\r\n0 TRLR",
            1,
        );
    fs::write(&file, text).unwrap();
    assert_eq!(
        stdout(gedcli(&file).args(["submitters", "me", "--count"])),
        "3\tMe\t@S1@\n"
    );
    assert_eq!(
        stdout(gedcli(&file).args(["submitters", "other", "--count"])),
        "1\tOther\t@S2@\n"
    );
}

#[test]
fn relation_names_grandmother_and_in_law() {
    let (_dir, file) = workspace();
    let out = stdout(gedcli(&file).args(["relation", "ola", "åse", "--format", "tree"]));
    assert_eq!(
        out,
        "Åse Lund is Ola Berg's grandmother.\n  path              Ola Berg → mother Kari Lund → mother Åse Lund\n"
    );
    let out = stdout(gedcli(&file).args(["relation", "per", "åse", "--format", "list"]));
    assert!(out.starts_with("relation\twife\n"));
}

#[test]
fn check_finds_one_sided_link_and_fixes_country() {
    let (_dir, file) = workspace();
    let text = fs::read_to_string(&file).unwrap();
    let text = text
        .replacen(
            "1 FAMC @F2@\r\n",
            "1 FAMC @F2@\r\n1 BIRT\r\n2 ADDR\r\n3 CTRY Norge\r\n",
            1,
        )
        .replacen("1 CHIL @I4@\r\n", "", 1);
    fs::write(&file, text).unwrap();
    let out = stdout(gedcli(&file).args(["check", "--format", "list"]));
    assert!(
        out.contains("one-sided-link\t@I4@\tOla Berg\tFAMC @F2@, but the family has no CHIL back")
    );
    assert!(out.contains("country-spelling\t@I4@\tOla Berg\t\"Norge\" → Norway"));
    gedcli(&file)
        .args(["check", "--fix", "--yes"])
        .assert()
        .success();
    let fixed = fs::read_to_string(&file).unwrap();
    assert!(fixed.contains("3 CTRY Norway\r\n") && !fixed.contains("Norge"));
}

#[test]
fn html_embeds_tree_data() {
    let (dir, file) = workspace();
    let out = dir.path().join("tree.html");
    gedcli(&file)
        .args(["html", "ola", "-o"])
        .arg(&out)
        .assert()
        .success();
    let page = fs::read_to_string(&out).unwrap();
    assert!(page.contains("<title>Family tree of Ola Berg</title>"));
    assert!(page.contains("\"name\":\"Åse Lund\""));
    assert!(!page.contains("__DATA__"));
}

#[test]
fn compare_reports_new_and_changed_people() {
    let (dir, file) = workspace();
    let other = dir.path().join("other.ged");
    let text = fs::read_to_string(&file)
        .unwrap()
        .replacen("2 DATE 1950\r\n", "2 DATE 1951\r\n", 1)
        .replacen(
            "0 @F1@ FAM\r\n",
            "0 @I5@ INDI\r\n1 NAME Nils /Berg/\r\n1 FAMC @F2@\r\n0 @F1@ FAM\r\n",
            1,
        )
        .replacen("1 CHIL @I4@\r\n", "1 CHIL @I4@\r\n1 CHIL @I5@\r\n", 1);
    fs::write(&other, text).unwrap();
    let out = stdout(
        gedcli(&file)
            .args(["compare", "--format", "list"])
            .arg(&other),
    );
    assert!(out.contains("new\t@I5@\tNils Berg\tchild of Kari Lund (b. 1951) @I3@"));
    assert!(out.contains("removed\t@I3@\tKari Lund\tbirth: 1950"));
    assert!(out.contains("added\t@I3@\tKari Lund\tbirth: 1951"));
    assert!(out.contains("added\t@I3@\tKari Lund\tchild: Nils Berg @I5@"));
}

#[test]
fn add_child_links_both_sides() {
    let (_dir, file) = workspace();
    let out = stdout(gedcli(&file).args([
        "add",
        "kari",
        "daughter",
        "Liv",
        "Berg",
        "about 1980",
        "--yes",
    ]));
    assert!(out.contains("add Liv Berg as daughter of Kari Lund in family @F2@"));
    assert!(out.contains("added @IG1@"));
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("0 @IG1@ INDI\r\n1 NAME Liv /Berg/\r\n2 GIVN Liv\r\n2 SURN Berg\r\n1 SEX F\r\n1 BIRT\r\n2 DATE ABT 1980\r\n1 FAMC @F2@\r\n"));
    assert!(text.contains("1 CHIL @I4@\r\n1 CHIL @IG1@\r\n"));
    let tree = stdout(gedcli(&file).args(["show", "kari", "--children", "--format", "list"]));
    assert!(tree.contains("1\tdaughter\tLiv Berg\tb. 1980\t@IG1@"));
}

#[test]
fn add_parent_creates_family_and_refuses_second_father() {
    let (_dir, file) = workspace();
    gedcli(&file)
        .args(["add", "per", "father", "Jon", "Lund", "1890", "--yes"])
        .assert()
        .success();
    let out = stdout(gedcli(&file).args(["relation", "per", "jon", "--format", "list"]));
    assert!(out.starts_with("relation\tfather\n"));
    gedcli(&file)
        .args(["add", "per", "father", "Other", "Lund", "--yes"])
        .assert()
        .failure();
    let check =
        stdout(gedcli(&file).args(["check", "--only", "one-sided-link", "--format", "list"]));
    assert_eq!(check, "# kind\tid\tname\tdetail\tfix\n");
}

#[test]
fn edit_refuses_to_rewrite_a_file_with_misencoded_bytes() {
    let (_dir, file) = workspace();
    let mut bytes = fs::read(&file).unwrap();
    let at = bytes.windows(4).position(|w| w == b"Kari").unwrap();
    bytes.splice(at..at + 4, b"K\xe5ri".iter().copied());
    for bytes in [bytes.clone(), bytes[3..].to_vec()] {
        fs::write(&file, &bytes).unwrap();
        gedcli(&file)
            .args(["edit", "ola", "--set", "birth.place=Bergen", "--yes"])
            .assert()
            .failure();
        assert_eq!(fs::read(&file).unwrap(), bytes);
    }
}
