use crate::maybe_async;

pub struct TokioSystem;

impl maybe_async::System for TokioSystem {
    async fn sleep(duration: std::time::Duration) {
        tokio::time::sleep(duration).await;
    }
}
