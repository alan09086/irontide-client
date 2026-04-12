//! Handlers for `irontide config <action>` subcommands.

use std::path::Path;

use crate::cli_def::ConfigAction;
use irontide_config::{self as config, ConfigFile};

/// Run the `config <action>` subcommand.
///
/// Returns an exit code: 0 on success, 1 on error.
pub(crate) fn run(action: ConfigAction, global_config: Option<&Path>) -> i32 {
    match action {
        ConfigAction::Init { force } => cmd_init(global_config, force),
        ConfigAction::Path => cmd_path(global_config),
        ConfigAction::Show => cmd_show(global_config),
        ConfigAction::Validate { path } => cmd_validate(global_config, path.as_deref()),
    }
}

/// Create a default configuration file with explanatory comments.
///
/// Uses `toml_edit::DocumentMut` to produce a commented TOML document so
/// that users can understand each field without consulting external docs.
/// All values are commented out so engine defaults apply until explicitly
/// overridden.
fn cmd_init(global_config: Option<&Path>, force: bool) -> i32 {
    let target = config::resolve_config_path(global_config);

    if target.exists() && !force {
        eprintln!(
            "error: config file already exists at {}, use --force to overwrite",
            target.display()
        );
        return 1;
    }

    // Create parent directories if they don't exist.
    if let Some(parent) = target.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        eprintln!(
            "error: failed to create directory {}: {e}",
            parent.display()
        );
        return 1;
    }

    let doc = build_default_config_document();
    let content = doc.to_string();

    if let Err(e) = std::fs::write(&target, content) {
        eprintln!(
            "error: failed to write config file {}: {e}",
            target.display()
        );
        return 1;
    }

    println!("created {}", target.display());
    0
}

/// Print the resolved configuration file path.
fn cmd_path(global_config: Option<&Path>) -> i32 {
    println!("{}", config::resolve_config_path(global_config).display());
    0
}

/// Dump the fully merged configuration as pretty-printed TOML.
///
/// Loads settings using the standard four-layer pipeline (defaults, file,
/// env, CLI) with no CLI overrides, then round-trips through `ConfigFile`
/// to produce user-facing TOML output.
fn cmd_show(global_config: Option<&Path>) -> i32 {
    let settings = match config::load(global_config, &ConfigFile::default()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to load configuration: {e}");
            return 1;
        }
    };

    let config_file = ConfigFile::from_settings(&settings);
    match toml::to_string_pretty(&config_file) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            eprintln!("error: failed to serialize configuration: {e}");
            1
        }
    }
}

/// Parse and validate a configuration file.
///
/// Uses `explicit_path` if provided; otherwise falls back to the global
/// `--config` flag or the platform default path.
fn cmd_validate(global_config: Option<&Path>, explicit_path: Option<&Path>) -> i32 {
    let target = explicit_path.map_or_else(
        || config::resolve_config_path(global_config),
        std::path::Path::to_path_buf,
    );

    if !target.exists() {
        eprintln!("error: config file not found: {}", target.display());
        return 1;
    }

    let contents = match std::fs::read_to_string(&target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read {}: {e}", target.display());
            return 1;
        }
    };

    let config_file: ConfigFile = match toml::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: invalid TOML in {}: {e}", target.display());
            return 1;
        }
    };

    let settings = config_file.to_settings_overrides();
    if let Err(e) = settings.validate() {
        eprintln!("error: invalid configuration in {}: {e}", target.display());
        return 1;
    }

    println!("valid: {}", target.display());
    0
}

/// Build a `toml_edit::DocumentMut` containing the default config with
/// all values commented out and explanatory comments for each field.
fn build_default_config_document() -> toml_edit::DocumentMut {
    use toml_edit::{DocumentMut, Item, Table, value};

    let mut doc = DocumentMut::new();

    // File-level header comment.
    doc.decor_mut().set_prefix(
        "# IronTide configuration file\n# Documentation: https://codeberg.org/alan090/irontide\n\n",
    );

    // ── [session] ──────────────────────────────────────────────────
    let mut session = Table::new();
    session.set_implicit(false);

    add_commented_field(
        &mut session,
        "download_dir",
        value("."),
        "Default download directory",
    );
    add_commented_field(
        &mut session,
        "listen_port",
        value(42020),
        "TCP listen port for incoming peer connections",
    );
    add_commented_field(
        &mut session,
        "enable_dht",
        value(true),
        "Enable DHT peer discovery (BEP 5)",
    );
    add_commented_field(
        &mut session,
        "enable_lsd",
        value(true),
        "Enable Local Service Discovery (BEP 14)",
    );
    add_commented_field(
        &mut session,
        "enable_pex",
        value(true),
        "Enable Peer Exchange (BEP 11)",
    );
    add_commented_field(
        &mut session,
        "workers",
        value(0),
        "Number of tokio worker threads (0 = auto)",
    );
    add_commented_field(
        &mut session,
        "pin_cores",
        value(true),
        "Pin worker threads to CPU cores",
    );

    doc.insert("session", Item::Table(session));

    // ── [api] ──────────────────────────────────────────────────────
    let mut api = Table::new();
    api.set_implicit(false);

    add_commented_field(
        &mut api,
        "bind",
        value("127.0.0.1"),
        "HTTP API bind address",
    );
    add_commented_field(
        &mut api,
        "port",
        value(9080),
        "HTTP API port (0 = disabled in download mode)",
    );

    doc.insert("api", Item::Table(api));

    // ── [limits] ───────────────────────────────────────────────────
    let mut limits = Table::new();
    limits.set_implicit(false);

    add_commented_field(
        &mut limits,
        "max_download_rate_bps",
        value(0),
        "Global download rate limit in bytes/sec (0 = unlimited)",
    );
    add_commented_field(
        &mut limits,
        "max_upload_rate_bps",
        value(0),
        "Global upload rate limit in bytes/sec (0 = unlimited)",
    );
    add_commented_field(
        &mut limits,
        "max_peers_per_torrent",
        value(128),
        "Maximum peer connections per torrent",
    );
    add_commented_field(
        &mut limits,
        "max_active_downloads",
        value(3),
        "Maximum concurrent downloading torrents",
    );
    add_commented_field(
        &mut limits,
        "max_active_uploads",
        value(5),
        "Maximum concurrent seeding torrents",
    );

    doc.insert("limits", Item::Table(limits));

    doc
}

/// Insert a key-value pair into a `Table`, but commented out.
///
/// The entry is rendered as:
/// ```toml
/// # <comment>
/// # <key> = <value>
/// ```
///
/// This works by inserting the key with a prefix decorator that includes
/// the comment and the `# ` prefix for the key line, then the key itself
/// is removed after its comment is set. Instead, we use toml_edit's
/// decoration to prefix the key with `# ` to comment it out.
fn add_commented_field(
    table: &mut toml_edit::Table,
    key: &str,
    item: toml_edit::Item,
    comment: &str,
) {
    // Insert the real key, then decorate it to look commented-out.
    table.insert(key, item);

    // Access the key-value entry to set decorations.
    if let Some(entry) = table.get_key_value_mut(key) {
        let (mut key_decor, val) = entry;
        // Prefix: newline + "# <comment>\n# " so the key line reads "# key = value"
        key_decor
            .leaf_decor_mut()
            .set_prefix(format!("# {comment}\n# "));
        // The value's suffix is just a newline (default), which is fine.
        // Mark the value's decor to ensure no extra whitespace.
        let _ = val;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_default_config_has_all_sections() {
        let doc = build_default_config_document();
        let text = doc.to_string();
        assert!(
            text.contains("[session]"),
            "should contain [session] section"
        );
        assert!(text.contains("[api]"), "should contain [api] section");
        assert!(text.contains("[limits]"), "should contain [limits] section");
    }

    #[test]
    fn build_default_config_values_are_commented_out() {
        let doc = build_default_config_document();
        let text = doc.to_string();
        // Every key=value line should be commented out.
        for line in text.lines() {
            let trimmed = line.trim();
            // Skip empty lines, section headers, and pure-comment lines.
            if trimmed.is_empty()
                || trimmed.starts_with('[')
                || (trimmed.starts_with('#') && !trimmed.contains('='))
            {
                continue;
            }
            // Lines with `=` should be prefixed with `#`.
            if trimmed.contains('=') {
                assert!(
                    trimmed.starts_with('#'),
                    "key-value line should be commented out: {trimmed}"
                );
            }
        }
    }

    #[test]
    fn build_default_config_contains_header() {
        let doc = build_default_config_document();
        let text = doc.to_string();
        assert!(
            text.starts_with("# IronTide configuration file"),
            "should start with header comment"
        );
    }

    #[test]
    fn cmd_path_returns_zero() {
        // Smoke test: cmd_path should always succeed.
        let code = cmd_path(None);
        assert_eq!(code, 0);
    }

    #[test]
    fn cmd_init_creates_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        let code = cmd_init(Some(&config_path), false);
        assert_eq!(code, 0, "cmd_init should succeed");
        assert!(config_path.exists(), "config file should be created");

        let contents = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            contents.contains("[session]"),
            "created file should contain [session]"
        );
    }

    #[test]
    fn cmd_init_refuses_overwrite_without_force() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "existing").expect("write existing file");

        let code = cmd_init(Some(&config_path), false);
        assert_eq!(code, 1, "should fail without --force");

        // Original content should be preserved.
        let contents = std::fs::read_to_string(&config_path).expect("read config");
        assert_eq!(contents, "existing");
    }

    #[test]
    fn cmd_init_overwrites_with_force() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "existing").expect("write existing file");

        let code = cmd_init(Some(&config_path), true);
        assert_eq!(code, 0, "should succeed with --force");

        let contents = std::fs::read_to_string(&config_path).expect("read config");
        assert!(
            contents.contains("[session]"),
            "overwritten file should contain [session]"
        );
    }

    #[test]
    fn cmd_show_returns_zero() {
        // With no config file, show should still produce defaults.
        let code = cmd_show(Some(std::path::Path::new(
            "/tmp/irontide-test-nonexistent-config-show/config.toml",
        )));
        assert_eq!(code, 0);
    }

    #[test]
    fn cmd_validate_nonexistent_file_returns_one() {
        let code = cmd_validate(
            None,
            Some(std::path::Path::new(
                "/tmp/irontide-test-nonexistent-validate/config.toml",
            )),
        );
        assert_eq!(code, 1);
    }

    #[test]
    fn cmd_validate_valid_file_returns_zero() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
[session]
listen_port = 42020

[limits]
max_peers_per_torrent = 64
"#,
        )
        .expect("write config file");

        let code = cmd_validate(None, Some(&config_path));
        assert_eq!(code, 0);
    }

    #[test]
    fn cmd_validate_invalid_toml_returns_one() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, "this is not valid toml {{{").expect("write bad file");

        let code = cmd_validate(None, Some(&config_path));
        assert_eq!(code, 1);
    }
}
