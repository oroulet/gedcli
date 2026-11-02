mod app;
mod family;
mod forms;
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
        return people
            .individuals()
            .find_map(|r| r.xref().map(str::to_owned))
            .context("the file has no people");
    }
    let choice = crate::select::Choice {
        pick: None,
        interactive: true,
        json: false,
    };
    Ok(crate::select::choose(crate::search::find(&people, query), &choice)?.id)
}

fn run(store: &mut Store, start: String) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, store, App::new(start));
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    store: &mut Store,
    mut app: App,
) -> Result<()> {
    while !app.quit {
        let size = terminal.size()?;
        let people = People::new(&store.doc);
        let screen = app.screen(&people);
        let body = ratatui::layout::Rect::new(0, 0, size.width, size.height.saturating_sub(1));
        app.visible = family::visible_rows(body, &app, &screen);
        terminal.draw(|frame| view::draw(frame, &app, &screen))?;
        drop(screen);
        drop(people);
        let Event::Key(key) = event::read()? else {
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

pub fn apply(store: &mut Store, effect: Effect) -> Result<String> {
    match effect {
        Effect::None => Ok(String::new()),
        Effect::Write {
            person,
            record,
            changes,
        } => apply_write(store, &person, record, &changes),
        Effect::Add(request) => apply_add(store, &request),
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
        .draw(|frame| view::draw(frame, app, &screen))
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
