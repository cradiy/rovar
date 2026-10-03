use super::*;
use std::collections::VecDeque;

pub(super) const CONNECTIONS: usize = 4;
type Completion = Box<dyn FnOnce(&mut Remote, &mut Context<Remote>)>;

#[derive(Default)]
pub(super) struct Scheduler {
    pub(super) active: BTreeSet<String>,
    pub(super) completed: VecDeque<Completion>,
    pub(super) order: BTreeMap<String, u64>,
    sequence: u64,
}

impl Remote {
    pub fn is_busy(&self) -> bool {
        self.busy || !self.scheduler.active.is_empty()
    }

    pub fn connection_busy(&self, connection: &str) -> bool {
        self.busy || self.scheduler.active.contains(connection)
    }

    pub fn path_busy(&self, path: &Path) -> bool {
        self.busy
            || self
                .link(path)
                .is_some_and(|link| self.connection_busy(&link.connection))
    }

    pub(super) fn can_start(&self, connection: &str) -> bool {
        !self.connection_busy(connection) && self.scheduler.active.len() < CONNECTIONS
    }

    pub(super) fn start_network(&mut self, connection: &str) {
        debug_assert!(self.can_start(connection));
        self.scheduler.active.insert(connection.to_owned());
        self.scheduler.sequence += 1;
        self.scheduler
            .order
            .insert(connection.to_owned(), self.scheduler.sequence);
    }

    pub(super) fn finish_network(
        &mut self,
        connection: String,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        // Keep this connection reserved while its result waits for a local
        // preflight/recovery operation. Publication owns the shared journal.
        self.scheduler
            .completed
            .push_back(Box::new(move |this, cx| {
                this.scheduler.active.remove(&connection);
                done(this, cx);
            }));
        self.publish_completed(cx);
    }

    pub(super) fn publish_completed(&mut self, cx: &mut Context<Self>) {
        let mut published = false;
        while !self.busy {
            let Some(done) = self.scheduler.completed.pop_front() else {
                break;
            };
            done(self, cx);
            published = true;
        }
        if published {
            cx.notify();
        }
    }
}
