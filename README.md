# gedcli

Browse and edit a GEDCOM family tree from the command line. Built on
[gedcomkit](https://crates.io/crates/gedcomkit), which preserves bytes: lines
you don't edit are written back exactly, so `git diff` on the data shows only
real changes. gedcli also keeps the file's BOM and CRLF line endings.

## Install

    cargo install gedcli          # from crates.io
    cargo install --path .        # from a checkout

## File

`--file F`, else `$GEDCLI_FILE`, else the only `*.ged` in the current directory.

## Commands

    gedcli show kari lund                     # person card (picker if several match)
    gedcli show lund --all                    # every match, one after the other, no picker
    gedcli show lund --children               # descendants, 10 generations, as a tree
    gedcli show ola --parents 3               # ancestors, 3 generations
    gedcli show ola --parents --format list|json|tree
    gedcli show ola --parents --count         # ancestors per generation: found / possible / % complete
    gedcli ui kari lund                       # full-screen browser: navigate, edit, add
    gedcli edit kari                          # interactive: pick field, type value, confirm
    gedcli edit kari --set birth.place=Bergen --set "birth.date=ABT 1950" --yes
    gedcli relation "kari lund" ola           # Ola is Kari's son, with common ancestors and path
    gedcli check [--only KIND] [--all]        # data problems, each with the edit command that fixes it
    gedcli check --fix                        # rename country spellings (Norge, Nrwy. → Norway)
    gedcli compare ~/Downloads/new-export.ged # what a newer export adds, removes or changes
    gedcli html ola --parents 6 --children 3 -o tree.html   # interactive tree in the browser
    gedcli summary [--format json]            # format, counts, submitters, connections, data quality
    gedcli submitters                         # people entered per submitter account
    gedcli submitters "kari lund" [--count]   # the people one submitter entered
    gedcli submitters --claim                 # link people without a submitter to the exporter
    gedcli add "kari lund" daughter Liv Berg "12 JUN 1980"
    gedcli add per son Nils - 1955 --with "ase lund"   # pick the family when there are several
    gedcli marriage per "ase lund" --date "17 DEC 1942" --place Bergen   # creates the family if needed
    gedcli edit kari --set birth.place=Bergen --source "Letter from Ola, 2026"   # cite where facts came from
    gedcli migrate [--dry-run]                # convert the file in place to GEDCOM 7.0
    gedcli export --to 5.5.1 -o out.ged       # converted copy, e.g. for Geni

Queries match every word against all names (given, surname, married), ignoring
case and accents. An xref (`@I123@`, `I123`) selects one person directly.

## For scripts and AI

- When stdout isn't a terminal the default format is `list` (tab-separated:
  `gen path name life id info`). Use `--format json` for structured output.
- If several people match and there's no terminal, gedcli prints the numbered
  candidates and exits with code 2. Rerun with `--pick N` or the xref.
- `edit` without a terminal needs `--set` and `--yes`.
- A person reached twice in a tree (pedigree collapse) is marked `repeat` and
  not expanded again.

## Edit fields

`name.given`, `name.surname`, `sex`, `birth.date`, `birth.place`, `death.date`,
`death.place`, `burial.date`, `burial.place`, `occupation`, `note`.

An empty value removes the field. Dates are checked and written in the
file version's grammar (`about 12 june 1950` becomes `ABT 12 JUN 1950`). Each
edit updates the record's `CHAN` timestamp (UTC). Removing the last detail of a
death or burial keeps `DEAT Y` / `BURI Y`, so the fact itself stays.

## Geni-specific behaviour

gedcli works with any GEDCOM file. A few features assume a Geni export, where
that's what makes them useful:

- `migrate` turns `RFN geni:ID` into `EXID ID` + `TYPE https://www.geni.com/`.
- `submitters` and the person card treat people without a `SUBM` link as
  entered by the file's exporter (`HEAD.SUBM`), because Geni leaves that link
  out for the exporting account's own profiles.

## Migration to 7.0

gedcomkit does the conversion (CONC folded, media types, CHAR dropped, extension
tags declared). gedcli then makes two clean-ups:
- It drops the date `PHRASE`s that differ from the date only by zero-padding
  (`01 MAY 2022`).
- It turns Geni `RFN geni:ID` into `EXID ID` + `TYPE https://www.geni.com/`.

## Submitters

Geni writes a `SUBM` link on each person, pointing to the account that entered
or manages that profile. It leaves the link out for the exporting account's own
profiles. So for the exporter, `gedcli submitters <name>` counts the linked
people plus the people with no submitter, and lists the two groups separately.

## Readability

Trees, the UI, HTML, `list` and `json` show a short `info` next to each
person: their occupation, or else a short one-line note (at most 50 characters,
with Geni's `{geni:about_me}` prefix removed and source references skipped).

On a terminal, `tree` colours names by sex (blue male, pink female, white unknown) and dims dates. It leaves out xrefs unless you pass `--ids`; `list` and `json` always include them.
Piped output never has colours. For a top-down view of a family, use `gedcli ui`.

## Terminal UI

`gedcli ui [name]` opens a full-screen view centred on one person:

- Their partner sits beside them, with older siblings to the left and younger
  ones to the right. Parents are above and the children of that partnership
  below.
- Lines connect parents to their children, so you can see whose children the
  grandchildren are.
- On a tall terminal, grandparents and grandchildren appear too.
- Rows with more people than fit scroll sideways as you move; ◀3 / 4▶ show
  how many are hidden.

| Keys | Does |
|---|---|
| ←↓↑→ / h j k l | move between people |
| Enter | put the highlighted person in the centre |
| s / Enter again | show all properties of the highlighted person (read-only) |
| b / Backspace | back to the previous person |
| n / Tab | next partner (the children row follows) |
| / | find someone by name |
| e | edit the highlighted person's fields |
| c / p | add a child / a parent to the highlighted person |
| ? | help |
| q / Ctrl-C | quit, after a Y/n question (Enter = yes); Esc only closes pop-ups |

In the edit pop-up, each field you change is checked right away and marked
`*`, but nothing is written until you press `w` (or choose "write changes").
Then all of them are saved together with one change date. `Esc` with
unsaved edits asks before discarding them. Additions (`c`, `p`) are saved at once.
Both use the same checks as `edit` and `add`, and if saving fails the change is
undone in memory too.

## Adding people

`gedcli add ANCHOR RELATION GIVEN SURNAME [BIRTH] [DEATH]`. RELATION is one of
son, daughter, child, father, mother, parent, husband, wife, partner, brother
or sister. Use `-` for no surname or to skip the birth date.

- Every link is written on both sides (`FAMC`/`CHIL`, `FAMS`/`HUSB`/`WIFE`).
- A family is created when none exists.
- A parent slot that is already taken is refused.
- A relative with the same first name triggers a warning.
- New records get local ids (`@IG1@`, `@FG1@`), so they never clash with Geni's.

## License

GPL-3.0-or-later, as required by the [gedcomkit](https://crates.io/crates/gedcomkit)
dependency. See [LICENSE](LICENSE).

## Releasing

    # bump `version` in Cargo.toml, then:
    cargo test && cargo clippy --all-targets -- -D warnings
    git commit -am "chore: release X.Y.Z"
    cargo publish --target-dir target/publish --dry-run
    cargo publish --target-dir target/publish
    git tag vX.Y.Z && git push && git push --tags

Keep `--target-dir target/publish`. Without it, the build cargo runs to verify
the package shares `target/` with normal builds. It records the packaged copy's
sources there, so later `cargo build`s consider gedcli up to date and keep a
stale binary until `cargo clean -p gedcli`.
