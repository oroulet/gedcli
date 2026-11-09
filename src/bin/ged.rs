//! `ged [ARGS]` is `gedcli ui [ARGS]`; options like `--file` work before or after the name.
fn main() -> std::process::ExitCode {
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    args.insert(1, "ui".into());
    gedcli::main_with(args)
}
