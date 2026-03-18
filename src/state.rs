#[derive(Debug, Default)]
pub struct AppState {
    pub typed_buffer: String,
    pub current_full: Option<String>,
    pub current_suffix: Option<String>,
    pub engine_running: bool,
    pub overlay_visible: bool,
    #[allow(dead_code)]
    pub last_auto_completed: Option<String>,
}

impl AppState {
    pub fn clear_suggestion(&mut self) {
        self.current_full = None;
        self.current_suffix = None;
    }
}
