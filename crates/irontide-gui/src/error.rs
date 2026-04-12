use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum GuiError {
    #[error("failed to start session: {0}")]
    SessionStart(String),

    #[error("configuration error: {0}")]
    Config(#[from] anyhow::Error),

    #[error("slint error: {0}")]
    Slint(#[from] slint::PlatformError),

    #[error("failed to load resume state: {0}")]
    Resume(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_error_display() {
        let err = GuiError::SessionStart("connection refused".to_string());
        assert!(err.to_string().contains("connection refused"));

        let err = GuiError::Resume("corrupt file".to_string());
        assert!(err.to_string().contains("corrupt file"));
    }
}
