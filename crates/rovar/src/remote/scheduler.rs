use super::*;
use std::collections::VecDeque;

pub(super) const CONNECTIONS: usize = 4;
type Completion = Box<dyn FnOnce(&mut Remote, &mut Context<Remote>)>;

pub(super) struct Refresh {
    connection: String,
    generation: u64,
    selected: Option<String>,
    open: BTreeSet<PathBuf>,
}

#[derive(Default)]
pub(super) struct Scheduler {
    pub(super) active: BTreeSet<String>,
    pub(super) completed: VecDeque<Completion>,
    pub(super) order: BTreeMap<String, u64>,
    refreshes: VecDeque<Refresh>,
    sequence: u64,
}

impl Remote {
    pub fn is_busy(&self) -> bool {
        self.busy
            || !self.scheduler.active.is_empty()
            || self
                .scheduler
                .refreshes
                .iter()
                .any(|r| self.refresh_is_current(r))
    }

    pub fn connection_busy(&self, connection: &str) -> bool {
        self.busy
            || self.scheduler.active.contains(connection)
            || self
                .scheduler
                .refreshes
                .iter()
                .any(|r| r.connection == connection && self.refresh_is_current(r))
    }

    pub fn path_busy(&self, path: &Path) -> bool {
        self.busy
            || self
                .link(path)
                .is_some_and(|link| self.connection_busy(&link.connection))
    }

    pub(super) fn can_start(&self, connection: &str) -> bool {
        !self.busy
            && !self.scheduler.active.contains(connection)
            && self.scheduler.active.len() < CONNECTIONS
    }

    fn refresh_is_current(&self, refresh: &Refresh) -> bool {
        self.connection(&refresh.connection)
            .is_some_and(|c| c.authenticated && c.generation == refresh.generation)
    }

    pub(super) fn queue_refresh(
        &mut self,
        connection: String,
        open: BTreeSet<PathBuf>,
        selected: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.connection(&connection).filter(|c| c.authenticated) else {
            return;
        };
        let generation = session.generation;
        if let Some(queued) = self.scheduler.refreshes.iter_mut().find(|r| {
            r.connection == connection && r.generation == generation && r.selected == selected
        }) {
            queued.open.extend(open);
        } else {
            self.scheduler.refreshes.push_back(Refresh {
                connection,
                generation,
                selected,
                open,
            });
        }
        self.publish_completed(cx);
        cx.notify();
    }

    fn start_refreshes(&mut self, cx: &mut Context<Self>) {
        let queued = std::mem::take(&mut self.scheduler.refreshes);
        self.scheduler.refreshes = queued
            .into_iter()
            .filter(|r| self.refresh_is_current(r))
            .collect();
        while let Some(index) = self
            .scheduler
            .refreshes
            .iter()
            .position(|r| self.can_start(&r.connection))
        {
            let refresh = self.scheduler.refreshes.remove(index).unwrap();
            self.refresh_selected(
                refresh.connection,
                refresh.open,
                refresh.selected,
                refresh.generation,
                cx,
            );
        }
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
        self.start_refreshes(cx);
        if published {
            cx.notify();
        }
    }
}
