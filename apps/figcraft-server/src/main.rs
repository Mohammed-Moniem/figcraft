//! Generic upstream collaboration server. The local product uses figcraft-directory.
fn main() -> std::process::ExitCode {
    figcraft_server::runner::main(false)
}
