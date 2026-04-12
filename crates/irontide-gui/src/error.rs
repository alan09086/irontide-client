use thiserror::Error;

#[derive(Debug, Error)]
pub enum GuiError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("slint error: {0}")]
    Slint(#[from] slint::PlatformError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_error_display() {
        let err = GuiError::Config("bad toml".to_string());
        assert!(err.to_string().contains("bad toml"));
    }
}
