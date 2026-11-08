//! Small authenticated MCP relay: all document work runs in an existing native app.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Arg, Command};

fn address(s: &str) -> Result<String, String> {
    if s.contains(':') {
        return Ok(s.to_owned());
    }
    let port = s.parse::<u16>().map_err(|_| "expected a port or host:port".to_string())?;
    Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)).to_string())
}

fn command() -> Command {
    Command::new("figcraft-attach")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Small authenticated MCP relay for a running FigCraft app")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("mcp")
                .about("Serve MCP on stdio through a running app; no headless fallback")
                .after_help("Start the app with --control PORT. File access is confined by the app's own automation roots. This binary never installs or edits MCP client configuration.")
                .arg(Arg::new("attach").long("attach").required(true).value_name("PORT|HOST:PORT").value_parser(address))
                .arg(Arg::new("control-token-file").long("control-token-file").value_name("FILE").value_parser(clap::value_parser!(PathBuf)).help("Private control token file; else FIGCRAFT_CONTROL_TOKEN_FILE or FIGCRAFT_LOCAL_DATA_DIR/control-PORT.token"))
                .arg(Arg::new("timeout").long("timeout").value_name("SECONDS").value_parser(clap::value_parser!(u64).range(1..=600)).help("App request timeout, 1–600 seconds (default60)"))
                // Name these flags so attempted confinement is refused explicitly rather than ignored.
                .arg(Arg::new("automation-read-root").long("automation-read-root").value_name("DIR").conflicts_with("attach").help("Refused when attached: configure read roots on the app"))
                .arg(Arg::new("automation-write-root").long("automation-write-root").value_name("DIR").conflicts_with("attach").help("Refused when attached: configure write roots on the app")),
        )
}

fn parse(args: Vec<OsString>) -> Result<clap::ArgMatches, clap::Error> {
    command().try_get_matches_from(args)
}

enum RelayError {
    Usage(&'static str),
    Failed(String),
}

fn serve(matches: clap::ArgMatches) -> Result<(), RelayError> {
    let Some(("mcp", args)) = matches.subcommand() else { return Err(RelayError::Usage("mcp --attach is required")) };
    let addr = args.get_one::<String>("attach").ok_or(RelayError::Usage("mcp --attach is required"))?;
    let token = figcraft_mcp::control_token_path(addr, args.get_one::<PathBuf>("control-token-file").map(PathBuf::as_path))
        .ok_or(RelayError::Usage("attached native control requires --control-token-file or FIGCRAFT_LOCAL_DATA_DIR"))?;
    let mut backend = figcraft_mcp::Attached::connect_with_token_file(addr, &token).map_err(|error| {
        RelayError::Failed(format!("can't reach or authenticate the FigCraft app at {addr}: {error} (start it with figcraft --control PORT)"))
    })?;
    if let Some(seconds) = args.get_one::<u64>("timeout") {
        backend = backend
            .with_timeout(std::time::Duration::from_secs(*seconds))
            .map_err(|error| RelayError::Failed(format!("attached timeout: {error}")))?;
    }
    eprintln!("FigCraft attached MCP relay at {addr} on stdio");
    let mut server = figcraft_mcp::Server::new(Box::new(backend));
    let input = std::io::stdin();
    let output = std::io::stdout();
    server.serve(input.lock(), output.lock()).map_err(|error| RelayError::Failed(format!("stdio: {error}")))
}

fn main() -> ExitCode {
    let matches = match parse(std::env::args_os().collect()) {
        Ok(matches) => matches,
        Err(error) => {
            let code = u8::try_from(error.exit_code()).unwrap_or(2);
            let _ = error.print();
            return ExitCode::from(code);
        }
    };
    match serve(matches) {
        Ok(()) => ExitCode::SUCCESS,
        Err(RelayError::Usage(error)) => {
            eprintln!("error: {error}\n\nFor more information, try --help.");
            ExitCode::from(2)
        }
        Err(RelayError::Failed(error)) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn attach_is_required_and_roots_or_headless_commands_are_refused() {
        for values in [
            vec!["figcraft-attach", "mcp"],
            vec!["figcraft-attach", "export", "file.figcraft"],
            vec!["figcraft-attach", "mcp", "--attach", "1234", "--automation-read-root", "/tmp"],
            vec!["figcraft-attach", "mcp", "--attach", "1234", "--automation-write-root", "/tmp"],
            vec!["figcraft-attach", "mcp", "--attach", "1234", "--install", "codex"],
        ] {
            assert_eq!(parse(args(&values)).unwrap_err().exit_code(), 2);
        }
    }

    #[test]
    fn existing_attached_args_and_timeout_limits_are_preserved() {
        let matches = parse(args(&["figcraft-attach", "mcp", "--attach", "1234", "--timeout", "600"])).unwrap();
        let sub = matches.subcommand_matches("mcp").unwrap();
        assert_eq!(sub.get_one::<String>("attach").unwrap(), "127.0.0.1:1234");
        for timeout in ["0", "601", "-1", "nan"] {
            assert_eq!(parse(args(&["figcraft-attach", "mcp", "--attach", "1234", "--timeout", timeout])).unwrap_err().exit_code(), 2);
        }
        assert_eq!(address("localhost:1234").unwrap(), "localhost:1234");
    }
}
