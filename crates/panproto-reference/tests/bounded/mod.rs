//! A wall-clock bound for test bodies.
//!
//! A defect that makes a reader or an arithmetic loop lose its progress
//! measure turns a test into one that never ends. Running the body on a
//! worker thread and waiting with a deadline turns that into an ordinary
//! failure, reported in seconds, while the result of a body that does end is
//! returned unchanged. The deadlines are far above the time the bodies take,
//! so they bound only runaway work, never ordinary slowness.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

/// Run `work` and return its result, failing the test when it panics or has
/// not returned after `seconds`.
pub fn within<T: Send + 'static>(seconds: u64, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("bounded".to_owned())
        .stack_size(64 << 20)
        .spawn(move || {
            let _ = sender.send(work());
        })
        .expect("spawn the bounded worker");
    match receiver.recv_timeout(Duration::from_secs(seconds)) {
        Ok(value) => value,
        Err(RecvTimeoutError::Timeout) => panic!("the bounded work did not end within {seconds} s"),
        Err(RecvTimeoutError::Disconnected) => panic!("the bounded work panicked"),
    }
}
