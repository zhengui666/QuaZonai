//! Isolated acceptance launcher using the unchanged public production entry.
//! No test node, factory, fixture operation or account credentials are supplied.
#[cfg(feature = "account-runtime")]
fn main() {
    job::native_runner::main_with_hooks(None, None, None);
}
#[cfg(not(feature = "account-runtime"))]
fn main() {
    job::native_runner::main_with_hooks(None, None);
}
