use super::VideoRuntime;
use std::collections::{BTreeSet, HashMap};

/// Media tasks and playback resources owned by one editor.
/// Decodes and imports may outlive a page switch; playback resources may not.
#[derive(Default)]
pub(in crate::editor) struct State {
    pub import_request: u64,
    pub importing: bool,
    pub decoding: BTreeSet<String>,
    pub error: Option<String>,
    pub image_request: usize,
    pub image_loading: Option<usize>,
    pub videos: HashMap<usize, VideoRuntime>,
    pub video_loading: BTreeSet<usize>,
    pub playback_generation: u64,
}

impl State {
    pub fn busy(&self) -> bool {
        self.importing || self.image_loading.is_some() || !self.video_loading.is_empty()
    }

    pub fn cancel_import(&mut self) {
        self.import_request += 1;
        self.importing = false;
        self.error = None;
    }

    pub fn clear_page(&mut self) {
        self.videos.clear();
        self.video_loading.clear();
        self.image_loading = None;
        self.error = None;
    }
}
