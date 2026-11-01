# gedcli

Browse and edit a GEDCOM family tree from the command line. Built on
[gedcomkit](https://crates.io/crates/gedcomkit), which preserves bytes: lines
you don't edit are written back exactly, so `git diff` on the data shows only
real changes. gedcli also keeps the file's BOM and CRLF line endings.

## Install

    cargo install --path .

## File

`--file F`, else `$GEDCLI_FILE`, else the only `*.ged` in the current directory.

## Commands

    gedcli show kari lund                # person card (picker if several match)
    gedcli show lund --children          # descendants, 10 generations, as a tree
    gedcli show ola --parents 3           # ancestors, 3 generations
    gedcli show ola --parents --format list|json|tree|chart
    gedcli show ola --parents --count     # ancestors per generation: found / possible / % complete
    gedcli show ase --children 3 --format chart   # top-down chart (ancestors: root at the bottom)
    gedcli edit kari                       # interactive: pick field, type value, confirm
    gedcli edit kari --set birth.place=Bergen --set "birth.date=ABT 1950" --yes
    gedcli relation "kari lund" ola      # Ola is Kari's son, with common ancestors and path
    gedcli check [--only KIND] [--all]        # data problems, each with the edit command that fixes it
    gedcli check --fix                        # rename country spellings (Norge, Nrwy. → Norway)
    gedcli html ola --parents 6 --children 3 -o tree.html   # interactive tree in the browser
    gedcli summary [--format json]            # format, counts, submitters, connections, data quality
    gedcli submitters                         # people entered per submitter account
    gedcli submitters "kari lund" [--count]       # the people one submitter entered
    gedcli migrate [--dry-run]                # convert the file in place to GEDCOM 7.0
    gedcli export --to 5.5.1 -o out.ged       # converted copy, e.g. for Geni

Queries match every word against all names (given, surname, married), ignoring
case and accents. An xref (`@I123@`, `I123`) selects one person directly.

## For scripts and AI

- When stdout isn't a terminal the default format is `list` (tab-separated:
  `gen path name life id`). Use `--format json` for structured output.
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

On a terminal, `tree` colours names by sex (blue male, pink female, white unknown) and dims dates and ids. `chart` wraps
names to 18 columns and warns when it is wider than the terminal (pipe it to
`less -S`, or ask for fewer generations). Piped output never has colours.
