mod app;
mod family;
mod forms;
mod memory;
mod portrait;
mod screen;
mod view;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyEventKind};

use crate::add;
use crate::edit;
use crate::person::People;
use crate::store::Store;
pub use app::{AddRequest, App, Effect};

pub fn start(store: &mut Store, query: &str) -> Result<()> {
    use std::io::IsTerminal;
    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        anyhow::bail!("`gedcli ui` needs a terminal; use show, edit or add from scripts");
    }
    let start = starting_person(store, query)?;
    run(store, start)
}

fn starting_person(store: &Store, query: &str) -> Result<String> {
    let people = People::new(&store.doc);
    if query.trim().is_empty() {
        return default_person(store, &people).context("the file has no people");
    }
    let choice = crate::select::Choice {
        pick: None,
        interactive: true,
        json: false,
    };
    Ok(crate::select::choose(crate::search::find(&people, query), &choice)?.id)
}

/// Where to start without a name: where the UI last closed on this file, else the file's exporter, else the first person.
fn default_person(store: &Store, people: &People) -> Option<String> {
    memory::remembered(&store.path)
        .filter(|xref| people.is_individual(xref))
        .or_else(|| exporter_person(store, people))
        .or_else(|| {
            people
                .individuals()
                .find_map(|r| r.xref().map(str::to_owned))
        })
}

fn exporter_person(store: &Store, people: &People) -> Option<String> {
    let exporter = crate::submitters::exporter(&store.doc)?;
    let name = crate::search::normalize(
        &crate::submitters::name_of(&store.doc, &exporter).replace('/', " "),
    );
    let mut matches = people
        .individuals()
        .map(crate::person::brief_of)
        .filter(|b| {
            crate::search::normalize(&b.name)
                .split_whitespace()
                .eq(name.split_whitespace())
        });
    let first = matches.next()?;
    matches.next().is_none().then_some(first.id)
}

fn run(store: &mut Store, start: String) -> Result<()> {
    let mut terminal = ratatui::init();
    let mut portraits = portrait::Portraits::detect();
    let mut app = App::new(start);
    app.show_portraits = portraits.has_graphics();
    app.status = format!(
        "images: {} (i toggles portraits)",
        portraits.protocol_name()
    );
    let result = event_loop(&mut terminal, store, &mut app, &mut portraits);
    ratatui::restore();
    let _ = memory::remember(&store.path, &app.center);
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    store: &mut Store,
    app: &mut App,
    portraits: &mut portrait::Portraits,
) -> Result<()> {
    while !app.quit {
        let size = terminal.size()?;
        let people = People::new(&store.doc);
        let mut screen = app.screen(&people);
        let body = ratatui::layout::Rect::new(0, 0, size.width, size.height.saturating_sub(1));
        let shown = viewed_photo(app, &people, &store.path, portraits, body);
        let photos = if app.show_portraits && shown.is_none() {
            load_photos(&screen, &people, &store.path, portraits)
        } else {
            Vec::new()
        };
        screen.photos = photos.iter().map(|(id, _, _)| id.clone()).collect();
        let viewer = shown.map(|(path, size, title)| view::Viewer {
            image: portraits.cached(&path, size),
            note: if portraits.request_failed(&path, size) {
                "this image cannot be shown here; o opens it outside"
            } else {
                "loading…"
            },
            title,
        });
        let images: std::collections::HashMap<String, _> = photos
            .iter()
            .filter_map(|(id, path, size)| {
                portraits
                    .cached(path, *size)
                    .map(|image| (id.clone(), image))
            })
            .collect();
        app.visible = family::visible_rows(body, app, &screen);
        terminal.draw(|frame| view::draw(frame, app, &screen, &images, viewer.as_ref()))?;
        drop(screen);
        drop(people);
        let Some(Event::Key(key)) = next_event(portraits)? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let effect = {
            let people = People::new(&store.doc);
            app.status.clear();
            app.key(key, &people)
        };
        app.status = apply(store, effect).unwrap_or_else(|err| format!("error: {err:#}"));
    }
    Ok(())
}

/// Queues the image the photo viewer shows; returns its file, size and title, or closes the viewer when there is none.
fn viewed_photo(
    app: &mut App,
    people: &People,
    ged: &std::path::Path,
    portraits: &mut portrait::Portraits,
    body: ratatui::layout::Rect,
) -> Option<(std::path::PathBuf, (u16, u16), String)> {
    let app::Mode::Photo { person, index } = &app.mode else {
        return None;
    };
    let mut paths = portrait::image_paths(people, ged, person);
    if paths.is_empty() {
        app.mode = app::Mode::Browse;
        app.status = "no photo for this person".to_owned();
        return None;
    }
    let count = paths.len();
    let shown = index.rem_euclid(count as isize) as usize;
    let path = paths.swap_remove(shown);
    let size = (body.width.saturating_sub(2), body.height.saturating_sub(2));
    portraits.request(&path, size);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let title = format!(
        " {name} — {}/{count} · h/l previous/next · o open outside · any key closes ",
        shown + 1
    );
    Some((path, size, title))
}

fn open_outside(store: &Store, person: &str, index: isize) -> Result<String> {
    let people = People::new(&store.doc);
    let paths = portrait::image_paths(&people, &store.path, person);
    anyhow::ensure!(!paths.is_empty(), "no photo for this person");
    let path = &paths[index.rem_euclid(paths.len() as isize) as usize];
    std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("running xdg-open")?;
    Ok(format!("opened {}", path.display()))
}

/// Waits for a terminal event; returns None when new photos arrived and the screen should be redrawn.
fn next_event(portraits: &mut portrait::Portraits) -> Result<Option<Event>> {
    loop {
        if event::poll(std::time::Duration::from_millis(40))? {
            return Ok(Some(event::read()?));
        }
        if portraits.receive() {
            return Ok(None);
        }
    }
}

/// Requests the photo of everyone on screen (large for the centre person); returns (upper-cased id, file, size).
fn load_photos(
    screen: &screen::Screen,
    people: &People,
    ged: &std::path::Path,
    portraits: &mut portrait::Portraits,
) -> Vec<(String, std::path::PathBuf, (u16, u16))> {
    if let Some(centre) = screen.rows[screen::CENTER].get(screen.center_pos)
        && let Some(path) = portrait::portrait_path(people, ged, &centre.id)
    {
        portraits.request(&path, portrait::PORTRAIT);
    }
    let mut photos = Vec::new();
    for (row, persons) in screen.rows.iter().enumerate() {
        for (index, person) in persons.iter().enumerate() {
            let size = if row == screen::CENTER && index == screen.center_pos {
                portrait::PORTRAIT
            } else {
                portrait::THUMB
            };
            if let Some(path) = portrait::portrait_path(people, ged, &person.id)
                && portraits.request(&path, size)
            {
                photos.push((person.id.to_ascii_uppercase(), path, size));
            }
        }
    }
    photos
}

pub fn apply(store: &mut Store, effect: Effect) -> Result<String> {
    match effect {
        Effect::None => Ok(String::new()),
        Effect::Write {
            person,
            record,
            changes,
        } => apply_write(store, &person, record, &changes),
        Effect::Add(request) => apply_add(store, &request),
        Effect::Open { person, index } => open_outside(store, &person, index),
    }
}

fn apply_write(
    store: &mut Store,
    person: &str,
    mut record: gedcomkit::Node,
    changes: &[edit::Change],
) -> Result<String> {
    edit::touch(&mut record);
    save_changed(store, |doc| {
        *doc.record_mut(person).context("record vanished")? = record;
        Ok(())
    })?;
    let shown = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".to_owned());
    let summary: Vec<String> = changes
        .iter()
        .map(|c| {
            format!(
                "{} {} → {}",
                c.field.key(),
                shown(&c.before),
                shown(&c.after)
            )
        })
        .collect();
    Ok(format!(
        "saved {} change(s): {}",
        changes.len(),
        summary.join("; ")
    ))
}

fn apply_add(store: &mut Store, request: &AddRequest) -> Result<String> {
    let person = add::new_person(
        &request.given,
        &request.surname,
        Some(&request.birth),
        Some(&request.death),
        store.is_v7(),
    )?;
    let plan = {
        let people = People::new(&store.doc);
        let anchor = people.brief(&request.anchor);
        add::plan(
            &people,
            &anchor,
            request.relation,
            &person,
            request.family.clone(),
        )?
    };
    let mut xref = String::new();
    save_changed(store, |doc| {
        xref = add::apply(doc, &plan, &person)?;
        Ok(())
    })?;
    let mut message = format!("saved: {} ({xref})", plan.lines.join(" "));
    for warning in &plan.warnings {
        message.push_str(&format!(" · warning: {warning}"));
    }
    Ok(message)
}

fn save_changed(
    store: &mut Store,
    change: impl FnOnce(&mut gedcomkit::Document) -> Result<()>,
) -> Result<()> {
    let before = store.doc.clone();
    let result = change(&mut store.doc).and_then(|()| store.save());
    if result.is_err() {
        store.doc = before;
    }
    result
}

#[cfg(test)]
pub fn render_to_text(store: &Store, app: &mut App, width: u16, height: u16) -> String {
    use ratatui::backend::TestBackend;
    let mut terminal =
        ratatui::Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    let people = People::new(&store.doc);
    let screen = app.screen(&people);
    let body = ratatui::layout::Rect::new(0, 0, width, height.saturating_sub(1));
    app.visible = family::visible_rows(body, app, &screen);
    terminal
        .draw(|frame| view::draw(frame, app, &screen, &std::collections::HashMap::new(), None))
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
