use crate::maybe_async;

pub struct StdSystem;

impl maybe_async::System for StdSystem {
    async fn sleep(duration: std::time::Duration) {
        std::thread::sleep(duration);
    }
}
