use std::sync::{Arc, Mutex};

use downlowd::ProgressHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressRecord {
    pub bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Clone)]
pub struct ProgressRecorder {
    records: Arc<Mutex<Vec<ProgressRecord>>>,
}

impl ProgressRecorder {
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn record_progress(&self, progress: &mut ProgressHandle) {
        record_progress(self.records.clone(), progress);
    }

    pub fn on_progress(&self) -> impl Fn(&mut ProgressHandle) + Send + Sync + 'static {
        let records = self.records.clone();
        move |p| record_progress(records.clone(), p)
    }

    pub fn records(&self) -> Vec<ProgressRecord> {
        let records = self.records.lock().unwrap();
        records.clone()
    }
}

fn record_progress(records: Arc<Mutex<Vec<ProgressRecord>>>, progress: &mut ProgressHandle) {
    let mut records = records.lock().unwrap();
    records.push(ProgressRecord {
        bytes: progress.bytes(),
        total_bytes: progress.remote_length(),
    });
}
