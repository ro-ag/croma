use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(usage());
    };

    match command.as_str() {
        "xml" => {
            let path = args.next().ok_or_else(usage)?;
            let source = fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
            let export = croma_core::export_musicxml(&source).map_err(|error| error.to_string())?;
            print!("{}", export.musicxml);
            Ok(())
        }
        "--help" | "-h" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: croma xml <file.abc>".to_owned()
}
