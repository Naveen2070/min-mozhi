//! `ir::failure`'s panic hook hands every panic it does not capture to the
//! hook it replaced. Its own test binary (one process, one test) so the
//! custom hook installed first is the one `ir::failure` chains to. See
//! `docs/superpowers/specs/2026-10-01-ir-pipeline-cli-design.local.md`.

use mimz_core::ir::failure::{Stage, catch};
use std::sync::atomic::{AtomicUsize, Ordering};

static PREVIOUS_HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn a_panic_outside_catch_reaches_the_previous_hook() {
    std::panic::set_hook(Box::new(|_| {
        PREVIOUS_HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
    }));

    let caught = catch(Stage::Lower, false, || -> u8 {
        let other = std::thread::spawn(|| -> u8 { panic!("theirs") });
        assert!(other.join().is_err());
        panic!("mine")
    });

    assert_eq!(caught.unwrap_err().message, "mine");
    assert_eq!(
        PREVIOUS_HOOK_CALLS.load(Ordering::SeqCst),
        1,
        "the other thread's panic reached the previous hook; the caught one did not"
    );
}
