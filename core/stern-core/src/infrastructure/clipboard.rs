pub struct ClipboardProvider;

impl ClipboardProvider {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn set_text(&self, text: &str) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard.set_text(text).map_err(|e| e.to_string())
    }

    #[must_use]
    pub fn get_text(&self) -> Option<String> {
        arboard::Clipboard::new()
            .ok()
            .and_then(|mut c| c.get_text().ok())
    }
}

impl Default for ClipboardProvider {
    fn default() -> Self {
        Self::new()
    }
}
