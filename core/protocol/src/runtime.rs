//! The clients' own tokio runtime, which every exported async function runs on,
//! in this crate and in the media crate above it.
//!
//! UniFFI can drive futures on a runtime of its own choosing, and that runtime
//! gives its threads the default 2 MiB stack. Joining a room on one of them
//! overflowed it in a debug build: the signalling future is large, and the
//! deflate buffer it builds for the join request lives on the stack, so the
//! app died with a bus error in the stack's guard page. The command-line probe
//! never saw it because it ran on the 8 MiB main thread. Owning the runtime
//! puts the size in one place and keeps every platform's app off the question.

use std::{future::Future, sync::OnceLock};

use tokio::runtime::{Builder, Runtime};

const STACK: usize = 8 * 1024 * 1024;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .thread_name("tomoshibi-core")
            .thread_stack_size(STACK)
            .enable_all()
            .build()
            .expect("the core's runtime starts")
    })
}

/// Runs `work` on the core's runtime and waits for it from wherever the
/// caller is, which for the apps is a UniFFI future polled by Swift.
pub async fn on_core<F>(work: F) -> F::Output
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match runtime().spawn(work).await {
        Ok(output) => output,
        Err(fault) => std::panic::resume_unwind(fault.into_panic()),
    }
}

/// Starts `work` on the core's runtime without waiting for it, from wherever
/// the caller is: a call from the app's capture thread has no runtime of its
/// own to spawn onto.
pub fn spawn<F>(work: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    runtime().spawn(work);
}
