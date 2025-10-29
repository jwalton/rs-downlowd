use std::time::Duration;

pub trait System {
    async fn sleep(duration: Duration);
}
