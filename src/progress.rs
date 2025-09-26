pub trait Progress {
    fn progress(&mut self, bytes_downloaded: u64, total_bytes: u64);
}

impl<T> Progress for T
where
    T: FnMut(u64, u64),
{
    fn progress(&mut self, bytes_downloaded: u64, total_bytes: u64) {
        self(bytes_downloaded, total_bytes);
    }
}

