#[derive(Debug, Default)]
pub struct AppState {
    pub typed_buffer: String,
    pub current_full: Option<String>,
    pub current_suffix: Option<String>,
}

impl AppState {
    pub fn clear_suggestion(&mut self) {
        self.current_full = None;
        self.current_suffix = None;
    }
}