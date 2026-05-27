//! M228: GET `/api/v2/app/preferences` projection tests for the 15 new
//! `QbtPreferences` fields wired from `Settings`. Each test constructs a
//! `Settings`, projects to `QbtPreferences::from(&s)`, asserts the field.
//!
//! Note: `add_stopped_enabled` from M228 is setPreferences-only; on the
//! GET side it surfaces as `start_paused_enabled`, which was wired by
//! M226 (`preferences.rs:211`). M228 adds 15 new GET fields, not 16.

use irontide::session::Settings;
use irontide_api::routes::qbt_v2::preferences::QbtPreferences;

#[test]
fn m228_get_projects_notify_on_complete() {
    let s = Settings {
        notify_on_complete: true,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.notify_on_complete);
}

#[test]
fn m228_get_projects_notify_on_error() {
    let s = Settings {
        notify_on_error: true,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.notify_on_error);
}

#[test]
fn m228_get_projects_autorun_program_some_to_path_string_none_to_empty() {
    // Some(PathBuf) → path string
    let s = Settings {
        on_complete_program: Some(std::path::PathBuf::from("/usr/local/bin/notify.sh")),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.autorun_program, "/usr/local/bin/notify.sh");

    // None → empty string
    let s = Settings {
        on_complete_program: None,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.autorun_program, "");
}

#[test]
fn m228_get_projects_temp_path_enabled() {
    let s = Settings {
        use_incomplete_dir: true,
        incomplete_dir: Some(std::path::PathBuf::from("/var/incomplete")),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.temp_path_enabled);
}

#[test]
fn m228_get_projects_temp_path_some_to_path_string_none_to_empty() {
    let s = Settings {
        incomplete_dir: Some(std::path::PathBuf::from("/var/incomplete")),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.temp_path, "/var/incomplete");

    let s = Settings {
        incomplete_dir: None,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.temp_path, "");
}

#[test]
fn m228_get_projects_add_skip_check() {
    let s = Settings {
        default_skip_hash_check: true,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.add_skip_check);
}

#[test]
fn m228_get_projects_incomplete_files_ext() {
    let s = Settings {
        incomplete_extension_enabled: true,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.incomplete_files_ext);
}

#[test]
fn m228_get_projects_scan_dirs_v2_some_to_path_string_none_to_empty() {
    let s = Settings {
        watched_folder: Some(std::path::PathBuf::from("/var/watched")),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.scan_dirs_v2, "/var/watched");

    let s = Settings {
        watched_folder: None,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.scan_dirs_v2, "");
}

#[test]
fn m228_get_projects_auto_delete_mode_bool_to_qbt_int() {
    // true → 2 (qBt "always")
    let s = Settings {
        delete_torrent_after_add: true,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.auto_delete_mode, 2);

    // false → 0 (qBt "manual")
    let s = Settings {
        delete_torrent_after_add: false,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.auto_delete_mode, 0);
}

#[test]
fn m228_get_projects_move_completed_enabled() {
    let s = Settings {
        move_completed_enabled: true,
        move_completed_to: Some(std::path::PathBuf::from("/var/moved")),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.move_completed_enabled);
}

#[test]
fn m228_get_projects_save_path_completed_some_to_path_string_none_to_empty() {
    let s = Settings {
        move_completed_to: Some(std::path::PathBuf::from("/var/moved")),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.save_path_completed, "/var/moved");

    let s = Settings {
        move_completed_to: None,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.save_path_completed, "");
}

#[test]
fn m228_get_projects_use_https() {
    let s = Settings {
        web_ui_https_enabled: true,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.use_https);
}

#[test]
fn m228_get_projects_current_network_interface_some_to_string_none_to_empty() {
    let s = Settings {
        network_interface: Some("eth0".to_owned()),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.current_network_interface, "eth0");

    let s = Settings {
        network_interface: None,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert_eq!(p.current_network_interface, "");
}

#[test]
fn m228_get_projects_preallocate_all_full_to_true_other_to_false() {
    use irontide::storage::PreallocateMode;

    // Some(Full) → true
    let s = Settings {
        preallocate_mode: Some(PreallocateMode::Full),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.preallocate_all);

    // Some(Sparse) → false (qBt's wire bool collapses non-Full to false)
    let s = Settings {
        preallocate_mode: Some(PreallocateMode::Sparse),
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(!p.preallocate_all);

    // None → false (derivation from storage_mode applies engine-side)
    let s = Settings {
        preallocate_mode: None,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(!p.preallocate_all);
}

#[test]
fn m228_get_projects_ip_filter_auto_refresh() {
    let s = Settings {
        ip_filter_auto_refresh: true,
        ..Settings::default()
    };
    let p = QbtPreferences::from(&s);
    assert!(p.ip_filter_auto_refresh);
}
