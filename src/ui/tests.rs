use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

pub fn fixture_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("family.ged");
    std::fs::copy("tests/family.ged", &path).unwrap();
    let store = Store::open(path).unwrap();
    (dir, store)
}

fn press(app: &mut App, store: &mut Store, code: KeyCode) -> String {
    let effect = {
        let people = People::new(&store.doc);
        app.key(KeyEvent::new(code, KeyModifiers::NONE), &people)
    };
    apply(store, effect).unwrap_or_else(|err| format!("error: {err:#}"))
}

fn type_text(app: &mut App, store: &mut Store, text: &str) {
    for c in text.chars() {
        press(app, store, KeyCode::Char(c));
    }
}

#[test]
fn shows_parents_person_and_children() {
    let (_dir, store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    let text = render_to_text(&store, &mut app, 100, 30);
    for expected in [
        "parents",
        "Per Lund",
        "Åse Lund",
        "Kari Lund",
        "Birth      1950",
        "children",
        "Ola Berg",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
}

#[test]
fn selected_box_is_thick_and_tight() {
    let (_dir, store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    let text = render_to_text(&store, &mut app, 100, 30);
    let top = text.lines().find(|l| l.contains('┏')).expect("a thick box");
    let width = top
        .chars()
        .skip_while(|&c| c != '┏')
        .take_while(|&c| c != '┓')
        .count()
        + 1;
    assert!(width < 40, "selected box is {width} wide:\n{text}");
}

#[test]
fn moves_up_recenters_and_goes_back() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('k'));
    press(&mut app, &mut store, KeyCode::Char('l'));
    press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(app.center, "@I1@");
    press(&mut app, &mut store, KeyCode::Char('b'));
    assert_eq!(app.center, "@I3@");
    press(&mut app, &mut store, KeyCode::Down);
    press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(app.center, "@I4@");
}

#[test]
fn editing_a_field_saves_the_file() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('e'));
    for _ in 0..4 {
        press(&mut app, &mut store, KeyCode::Down);
    }
    press(&mut app, &mut store, KeyCode::Enter);
    type_text(&mut app, &mut store, "Bergen");
    let status = press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(status, "saved birth.place: — → Bergen");
    let saved = std::fs::read_to_string(&store.path).unwrap();
    assert!(saved.contains("2 DATE 1950\r\n2 PLAC Bergen\r\n"));
}

#[test]
fn adding_a_child_saves_and_shows_it() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('c'));
    press(&mut app, &mut store, KeyCode::Up);
    press(&mut app, &mut store, KeyCode::Right);
    press(&mut app, &mut store, KeyCode::Tab);
    type_text(&mut app, &mut store, "Liv");
    press(&mut app, &mut store, KeyCode::Tab);
    press(&mut app, &mut store, KeyCode::Tab);
    type_text(&mut app, &mut store, "1980");
    let status = press(&mut app, &mut store, KeyCode::Enter);
    assert!(
        status.starts_with("saved: add Liv Lund as daughter of Kari Lund"),
        "{status}"
    );
    let text = render_to_text(&store, &mut app, 100, 30);
    assert!(text.contains("Liv Lund"), "{text}");
    assert!(
        std::fs::read_to_string(&store.path)
            .unwrap()
            .contains("1 NAME Liv /Lund/\r\n")
    );
}

#[test]
fn search_jumps_to_a_person() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('/'));
    type_text(&mut app, &mut store, "ola");
    press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(app.center, "@I4@");
}

#[test]
fn second_father_is_refused_without_writing() {
    let (_dir, mut store) = fixture_store();
    let before = std::fs::read(&store.path).unwrap();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('p'));
    type_text(&mut app, &mut store, "Jon");
    let status = press(&mut app, &mut store, KeyCode::Enter);
    assert!(
        status.starts_with("error: Kari Lund already has a father"),
        "{status}"
    );
    assert_eq!(std::fs::read(&store.path).unwrap(), before);
}

#[test]
#[ignore]
fn preview_real_file() {
    let path = std::env::var("GEDCLI_PREVIEW").unwrap();
    let store = Store::open(path.into()).unwrap();
    let start = std::env::var("GEDCLI_PREVIEW_ID").unwrap();
    let mut app = App::new(start);
    let size: Vec<u16> = std::env::var("GEDCLI_PREVIEW_SIZE")
        .unwrap_or_else(|_| "120x45".to_owned())
        .split('x')
        .map(|n| n.parse().unwrap())
        .collect();
    app.focus = (super::screen::CENTER, 0);
    println!("{}", render_to_text(&store, &mut app, size[0], size[1]));
}

#[test]
fn next_partner_switches_the_children_row() {
    let (_dir, mut store) = fixture_store();
    let request = AddRequest {
        anchor: "@I2@".to_owned(),
        relation: crate::add::Relation::Wife,
        family: None,
        given: "Mia".to_owned(),
        surname: "Strand".to_owned(),
        birth: String::new(),
        death: String::new(),
    };
    apply(&mut store, Effect::Add(request)).unwrap();
    let mut app = App::new("@I2@".to_owned());
    let first = render_to_text(&store, &mut app, 100, 30);
    assert!(
        first.contains("children with Åse Lund") && first.contains("partner 1/2"),
        "{first}"
    );
    press(&mut app, &mut store, KeyCode::Char('n'));
    let second = render_to_text(&store, &mut app, 100, 30);
    assert!(
        second.contains("children with Mia Strand") && second.contains("none recorded"),
        "{second}"
    );
}

#[test]
fn ctrl_c_quits_from_any_mode() {
    let (_dir, mut store) = fixture_store();
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    for opener in [None, Some('e'), Some('c'), Some('/')] {
        let mut app = App::new("@I3@".to_owned());
        if let Some(key) = opener {
            press(&mut app, &mut store, KeyCode::Char(key));
        }
        let people = People::new(&store.doc);
        app.key(ctrl_c, &people);
        assert!(app.quit, "Ctrl-C did not quit after {opener:?}");
    }
}

#[test]
fn grandchildren_hang_under_their_own_parent() {
    let (_dir, mut store) = fixture_store();
    let add = |store: &mut Store, anchor: &str, family: Option<&str>, given: &str| {
        let request = AddRequest {
            anchor: anchor.to_owned(),
            relation: crate::add::Relation::Child,
            family: family.map(str::to_owned),
            given: given.to_owned(),
            surname: "Lund".to_owned(),
            birth: "1960".to_owned(),
            death: String::new(),
        };
        apply(store, Effect::Add(request)).unwrap();
    };
    add(&mut store, "@I1@", Some("@F1@"), "Liv");
    add(&mut store, "@IG1@", None, "Nils");
    let mut app = App::new("@I1@".to_owned());
    let people = People::new(&store.doc);
    let screen = app.screen(&people);
    let area = ratatui::layout::Rect::new(0, 0, 120, 44);
    app.visible = family::visible_rows(area, &app, &screen);
    let names = |row: usize, items: &[usize]| -> Vec<String> {
        items
            .iter()
            .map(|&i| screen.rows[row][i].name.clone())
            .collect()
    };
    let links: Vec<(Vec<String>, Vec<String>)> = family::drawn_links(area, &app, &screen)
        .into_iter()
        .filter(|(upper, _, _, _)| *upper == screen::CHILDREN)
        .map(|(upper, lower, ups, downs)| (names(upper, &ups), names(lower, &downs)))
        .collect();
    let owned = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert!(
        links.contains(&(owned(&["Kari Lund"]), owned(&["Ola Berg"]))),
        "{links:?}"
    );
    assert!(
        links.contains(&(owned(&["Liv Lund"]), owned(&["Nils Lund"]))),
        "{links:?}"
    );
}
