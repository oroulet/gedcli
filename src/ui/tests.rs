use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::Mode;
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
fn edits_stay_in_the_form_until_written() {
    let (_dir, mut store) = fixture_store();
    let before = std::fs::read(&store.path).unwrap();
    let mut app = App::new("@I3@".to_owned());
    let row = |field| {
        crate::edit::FIELDS
            .iter()
            .position(|f| *f == field)
            .unwrap()
    };
    let birth_place = row(crate::edit::Field::BirthPlace);
    let occupation = row(crate::edit::Field::Occupation);
    press(&mut app, &mut store, KeyCode::Char('e'));
    for _ in 0..birth_place {
        press(&mut app, &mut store, KeyCode::Down);
    }
    press(&mut app, &mut store, KeyCode::Enter);
    type_text(&mut app, &mut store, "Bergen");
    press(&mut app, &mut store, KeyCode::Enter);
    assert!(
        matches!(app.mode, Mode::Edit(_)),
        "still editing after one field"
    );
    for _ in birth_place..occupation {
        press(&mut app, &mut store, KeyCode::Down);
    }
    press(&mut app, &mut store, KeyCode::Enter);
    type_text(&mut app, &mut store, "nurse");
    press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(
        std::fs::read(&store.path).unwrap(),
        before,
        "nothing written before w"
    );
    let status = press(&mut app, &mut store, KeyCode::Char('w'));
    assert!(
        status.starts_with("saved 2 change(s): birth.place — → Bergen; occupation — → nurse"),
        "{status}"
    );
    assert!(matches!(app.mode, Mode::Browse));
    let saved = std::fs::read_to_string(&store.path).unwrap();
    assert!(
        saved.contains("2 DATE 1950\r\n2 PLAC Bergen\r\n") && saved.contains("1 OCCU nurse\r\n")
    );
    assert_eq!(
        saved.matches("1 CHAN\r\n").count(),
        1,
        "one change stamp for one write"
    );
}

#[test]
fn a_bad_value_keeps_the_input_open() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('e'));
    for _ in 0..3 {
        press(&mut app, &mut store, KeyCode::Down);
    }
    press(&mut app, &mut store, KeyCode::Enter);
    press(&mut app, &mut store, KeyCode::Char('u'));
    let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
    let people = People::new(&store.doc);
    app.key(ctrl_u, &people);
    drop(people);
    type_text(&mut app, &mut store, "sometime");
    press(&mut app, &mut store, KeyCode::Enter);
    let Mode::Edit(form) = &app.mode else {
        panic!("left the edit form")
    };
    assert!(
        form.input.is_some()
            && form
                .error
                .as_deref()
                .is_some_and(|e| e.contains("cannot read date"))
    );
    assert!(form.changes.is_empty());
}

#[test]
fn esc_with_unsaved_edits_asks_before_discarding() {
    let (_dir, mut store) = fixture_store();
    let before = std::fs::read(&store.path).unwrap();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('e'));
    press(&mut app, &mut store, KeyCode::Enter);
    type_text(&mut app, &mut store, "x");
    press(&mut app, &mut store, KeyCode::Enter);
    press(&mut app, &mut store, KeyCode::Esc);
    let Mode::Edit(form) = &app.mode else {
        panic!("Esc discarded without asking")
    };
    assert!(form.confirm_discard);
    press(&mut app, &mut store, KeyCode::Char('n'));
    assert!(matches!(&app.mode, Mode::Edit(f) if !f.confirm_discard && f.changes.len() == 1));
    press(&mut app, &mut store, KeyCode::Esc);
    press(&mut app, &mut store, KeyCode::Char('y'));
    assert!(matches!(app.mode, Mode::Browse));
    assert_eq!(std::fs::read(&store.path).unwrap(), before);
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
    app.focus = (super::screen::CENTER, None);
    if let Ok(keys) = std::env::var("GEDCLI_PREVIEW_KEYS") {
        let people = People::new(&store.doc);
        for c in keys.chars() {
            app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), &people);
        }
    }
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
fn quitting_asks_first_and_defaults_to_yes() {
    let (_dir, mut store) = fixture_store();
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let people_key = |app: &mut App, store: &Store, key: KeyEvent| {
        let people = People::new(&store.doc);
        app.key(key, &people);
    };
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('q'));
    assert!(!app.quit && matches!(app.mode, Mode::ConfirmQuit(_)));
    press(&mut app, &mut store, KeyCode::Char('n'));
    assert!(!app.quit && matches!(app.mode, Mode::Browse));
    press(&mut app, &mut store, KeyCode::Char('q'));
    press(&mut app, &mut store, KeyCode::Enter);
    assert!(app.quit, "Enter is yes");
    for opener in [None, Some('e'), Some('c'), Some('/')] {
        let mut app = App::new("@I3@".to_owned());
        if let Some(key) = opener {
            press(&mut app, &mut store, KeyCode::Char(key));
        }
        people_key(&mut app, &store, ctrl_c);
        assert!(!app.quit, "Ctrl-C asks first after {opener:?}");
        press(&mut app, &mut store, KeyCode::Esc);
        assert!(
            !app.quit && !matches!(app.mode, Mode::ConfirmQuit(_)),
            "Esc returns after {opener:?}"
        );
        people_key(&mut app, &store, ctrl_c);
        people_key(&mut app, &store, ctrl_c);
        assert!(app.quit, "Ctrl-C twice quits after {opener:?}");
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

#[test]
fn siblings_sit_beside_the_person_and_link_to_the_parents() {
    let (_dir, mut store) = fixture_store();
    let request = AddRequest {
        anchor: "@I3@".to_owned(),
        relation: crate::add::Relation::Brother,
        family: None,
        given: "Nils".to_owned(),
        surname: "Lund".to_owned(),
        birth: "1955".to_owned(),
        death: String::new(),
    };
    apply(&mut store, Effect::Add(request)).unwrap();
    let mut app = App::new("@I3@".to_owned());
    let people = People::new(&store.doc);
    let screen = app.screen(&people);
    let row: Vec<&str> = screen.rows[screen::CENTER]
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(row, ["Kari Lund", "Nils Lund"]);
    assert_eq!(screen.center_pos, 0);
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    app.visible = family::visible_rows(area, &app, &screen);
    let parents_to_row: Vec<_> = family::drawn_links(area, &app, &screen)
        .into_iter()
        .filter(|(upper, ..)| *upper == screen::PARENTS)
        .collect();
    assert_eq!(
        parents_to_row,
        [(screen::PARENTS, screen::CENTER, vec![0, 1], vec![0, 1])]
    );
    drop(people);
    press(&mut app, &mut store, KeyCode::Char('l'));
    press(&mut app, &mut store, KeyCode::Enter);
    assert_eq!(app.center, "@IG1@");
}

#[test]
fn show_lists_all_properties_read_only() {
    let (_dir, mut store) = fixture_store();
    let before = std::fs::read(&store.path).unwrap();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('s'));
    let text = render_to_text(&store, &mut app, 100, 30);
    for expected in [
        "Kari Lund — j/k scroll",
        "Birth       1950",
        "Mother      Åse Lund",
        "  Son       Ola Berg",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in\n{text}");
    }
    press(&mut app, &mut store, KeyCode::Esc);
    assert!(matches!(app.mode, Mode::Browse));
    press(&mut app, &mut store, KeyCode::Enter);
    assert!(
        matches!(app.mode, Mode::Show(_)),
        "Enter on the centred person shows it"
    );
    assert_eq!(std::fs::read(&store.path).unwrap(), before);
}

#[test]
fn esc_closes_pop_ups_but_never_quits() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    for opener in ['e', 'c', 'p', 's', '/', '?'] {
        press(&mut app, &mut store, KeyCode::Char(opener));
        assert!(!matches!(app.mode, Mode::Browse), "{opener} opened nothing");
        press(&mut app, &mut store, KeyCode::Esc);
        assert!(
            matches!(app.mode, Mode::Browse),
            "Esc did not close {opener}"
        );
    }
    press(&mut app, &mut store, KeyCode::Esc);
    assert!(!app.quit);
}

#[test]
fn centre_details_show_occupation_and_notes_but_not_submitter() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("family.ged");
    let text = std::fs::read_to_string("tests/family.ged").unwrap().replacen(
        "1 FAMC @F1@\r\n1 FAMS @F2@\r\n",
        "1 OCCU nurse\r\n1 NOTE first line\r\n2 CONT second line\r\n1 FAMC @F1@\r\n1 FAMS @F2@\r\n1 SUBM @S1@\r\n",
        1,
    );
    std::fs::write(
        &path,
        text.replacen("0 TRLR", "0 @S1@ SUBM\r\n1 NAME Someone\r\n0 TRLR", 1),
    )
    .unwrap();
    let store = Store::open(path).unwrap();
    let mut app = App::new("@I3@".to_owned());
    let screen_text = render_to_text(&store, &mut app, 100, 30);
    for expected in [
        "Occupation nurse",
        "Note       first line",
        "           second line",
    ] {
        assert!(
            screen_text.contains(expected),
            "missing {expected:?} in\n{screen_text}"
        );
    }
    assert!(!screen_text.contains("Added by"), "{screen_text}");
}

#[test]
fn without_a_name_the_ui_starts_at_the_exporter() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("family.ged");
    let text = std::fs::read_to_string("tests/family.ged").unwrap();
    let text = text
        .replacen("1 CHAR UTF-8\r\n", "1 CHAR UTF-8\r\n1 SUBM @S1@\r\n", 1)
        .replacen("0 TRLR", "0 @S1@ SUBM\r\n1 NAME Kari /Lund/\r\n0 TRLR", 1);
    std::fs::write(&path, text).unwrap();
    let store = Store::open(path).unwrap();
    let people = People::new(&store.doc);
    assert_eq!(default_person(&store, &people).as_deref(), Some("@I3@"));
}

#[test]
fn search_results_show_relation_and_recent_people() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    press(&mut app, &mut store, KeyCode::Char('/'));
    type_text(&mut app, &mut store, "ola");
    let Mode::Search(form) = &app.mode else {
        panic!("no search")
    };
    assert_eq!(form.labels, ["Ola Berg — son"]);
    press(&mut app, &mut store, KeyCode::Enter);
    press(&mut app, &mut store, KeyCode::Char('/'));
    let Mode::Search(form) = &app.mode else {
        panic!("no search")
    };
    assert!(form.is_recent());
    assert_eq!(form.labels, ["Kari Lund (b. 1950) — mother"]);
}

#[test]
fn great_grandchildren_show_when_there_is_room() {
    let (_dir, mut store) = fixture_store();
    let request = AddRequest {
        anchor: "@I4@".to_owned(),
        relation: crate::add::Relation::Daughter,
        family: None,
        given: "Mia".to_owned(),
        surname: "Berg".to_owned(),
        birth: "2010".to_owned(),
        death: String::new(),
    };
    apply(&mut store, Effect::Add(request)).unwrap();
    let mut app = App::new("@I1@".to_owned());
    let people = People::new(&store.doc);
    let screen = app.screen(&people);
    let tall = ratatui::layout::Rect::new(0, 0, 100, 60);
    app.visible = family::visible_rows(tall, &app, &screen);
    assert!(app.visible[screen::GREAT_GRANDCHILDREN]);
    let links = family::drawn_links(tall, &app, &screen);
    assert!(
        links.contains(&(
            screen::GRANDCHILDREN,
            screen::GREAT_GRANDCHILDREN,
            vec![0],
            vec![0]
        )),
        "{links:?}"
    );
    let short = ratatui::layout::Rect::new(0, 0, 100, 20);
    assert!(!family::visible_rows(short, &app, &screen)[screen::GREAT_GRANDCHILDREN]);
}

#[test]
fn the_wife_stays_on_the_right_whoever_is_centred() {
    let (_dir, store) = fixture_store();
    let people = People::new(&store.doc);
    for centre in ["@I1@", "@I2@"] {
        let screen = App::new(centre.to_owned()).screen(&people);
        let row: Vec<&str> = screen.rows[screen::CENTER]
            .iter()
            .map(|b| b.name.as_str())
            .collect();
        assert_eq!(row, ["Per Lund", "Åse Lund"], "centred on {centre}");
        assert_eq!(screen.center().id, centre);
    }
}

#[test]
fn narrow_screen_with_the_husband_scrolled_away_draws() {
    let (_dir, store) = fixture_store();
    let mut app = App::new("@I1@".to_owned());
    for width in 14..40 {
        render_to_text(&store, &mut app, width, 30);
    }
}

#[test]
fn o_on_the_tree_opens_the_focused_photo() {
    let (_dir, mut store) = fixture_store();
    let mut app = App::new("@I3@".to_owned());
    let opened = press(&mut app, &mut store, KeyCode::Char('o'));
    assert_eq!(opened, "error: no photo for this person");
    assert!(matches!(app.mode, Mode::Browse));
}
