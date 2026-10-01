//! Standalone compatibility entrypoint for the optional operator command.
fn main() {
    job::operator::catalog_prepare::run(std::env::args_os().collect());
}
