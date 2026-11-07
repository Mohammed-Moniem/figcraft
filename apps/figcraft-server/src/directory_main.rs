//! Loopback-only local artifact service; design sharing cannot be enabled by flags or config.
fn main() -> std::process::ExitCode {
    figcraft_server::runner::main(true)
}
