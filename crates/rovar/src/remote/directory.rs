use super::*;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Directory {
    pub cursor: i64,
    /// Advancing the feed never forgets versions deferred by a local edit or
    /// failed transfer. Both the queue and its retry deadlines survive restart.
    pub pending: BTreeMap<String, Object>,
    #[serde(default)]
    failures: BTreeMap<String, Failure>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Failure {
    revision: i64,
    attempts: u32,
    retry_after: u64,
    message: String,
}

impl Directory {
    pub fn queue(&mut self, object: Object) {
        if self
            .failures
            .get(&object.id)
            .is_some_and(|failure| failure.revision != object.revision)
        {
            self.failures.remove(&object.id);
        }
        self.pending.insert(object.id.clone(), object);
    }

    pub fn attempts(&self, id: &str) -> u32 {
        self.failures.get(id).map_or(0, |failure| failure.attempts)
    }

    pub fn failed(&self, id: &str) -> bool {
        self.failures.contains_key(id)
    }

    pub fn ready(&self, id: &str) -> bool {
        self.failures
            .get(id)
            .is_none_or(|failure| failure.retry_after <= crate::platform::now())
    }

    pub fn fail(&mut self, object: &Object, error: &anyhow::Error) {
        let attempts = self
            .failures
            .get(&object.id)
            .filter(|failure| failure.revision == object.revision)
            .map_or(1, |failure| failure.attempts.saturating_add(1));
        // 30 seconds, 1 minute, ... up to 15 minutes per object. A newer
        // revision or explicit retry is allowed to bypass the deadline.
        let delay = (30u64 << attempts.saturating_sub(1).min(5)).min(15 * 60);
        self.failures.insert(
            object.id.clone(),
            Failure {
                revision: object.revision,
                attempts,
                retry_after: crate::platform::now().saturating_add(delay),
                message: error.to_string(),
            },
        );
    }

    pub fn complete(&mut self, id: &str) {
        self.pending.remove(id);
        self.failures.remove(id);
    }

    pub fn retry_now(&mut self) {
        for failure in self.failures.values_mut() {
            failure.retry_after = 0;
        }
    }

    pub fn error(&self) -> Option<String> {
        self.failures.iter().find_map(|(id, failure)| {
            self.pending
                .get(id)
                .map(|object| format!("{}: {}", object.title, failure.message))
        })
    }
}
