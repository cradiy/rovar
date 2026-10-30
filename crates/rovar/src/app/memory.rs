use super::*;

impl Studio {
    pub(super) fn release_unused_memory(&self, cx: &mut Context<Self>) {
        if self
            .session
            .borrow()
            .windows
            .values()
            .any(|tabs| !tabs.is_empty())
        {
            return;
        }
        let session = self.session.clone();
        cx.release_unused_memory_after(Duration::from_secs(3), move |_| {
            session.borrow().windows.values().all(Vec::is_empty)
        });
    }
}
