//! Standalone compatibility entrypoint for the optional operator command.
fn main() {
    job::operator::polymarket_history::run(std::env::args_os().collect());
}
