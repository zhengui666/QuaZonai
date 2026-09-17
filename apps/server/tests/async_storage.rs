//! Layout regression for the actual queue entry points. No database, task,
//! model or Runtime is created or polled by these checks.
use server::worker::Worker;
use std::{future::Future, mem::size_of};
use store::lifecycle::RunMessage;
use tokio::sync::watch;

// Inferring F from a factory measures the native compiler's layout without
// constructing a potentially oversized Future on the test thread's stack.
fn inline_bytes<F: Future>(
    _: impl FnOnce(&'static Worker, RunMessage, &'static str, watch::Receiver<bool>) -> F,
) -> usize {
    size_of::<F>()
}

#[test]
fn queue_entry_futures_do_not_embed_the_entire_research_pipeline() {
    let layouts = [
        (
            "process_mission_message",
            inline_bytes(|worker, message, owner, shutdown| {
                worker.process_mission_message(message, owner, shutdown)
            }),
        ),
        (
            "process_message",
            inline_bytes(|worker, message, owner, shutdown| {
                worker.process_message(message, owner, shutdown)
            }),
        ),
    ];
    // Bound each queue entry to 64 KiB of inline storage; large native stages
    // belong behind owned pins, not in every parent state machine and select.
    // This is not a thread-stack override or a replacement for Mission tests.
    for (name, bytes) in layouts {
        println!("{name}: {bytes} bytes of inline Future storage");
    }
    assert!(
        layouts.iter().all(|(_, bytes)| *bytes <= 64 * 1024),
        "queue entry Future is too large: {layouts:?}"
    );
}
