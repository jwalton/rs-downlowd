pub trait Limiter {
    /// Called to notify the bucket that we consumed some bytes.
    async fn bytes_consumed(&self, bytes: u64);

    /// Called to wait until the caller can download more bytes.
    async fn wait(&self);
}
