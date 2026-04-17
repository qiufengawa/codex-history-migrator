use std::path::Path;

use codex_history_migrator::platform::{
    DesktopPlatform, open_file_location_command_for_platform, open_path_command_for_platform,
};

#[test]
fn windows_open_path_uses_cmd_start() {
    let command =
        open_path_command_for_platform(DesktopPlatform::Windows, Path::new(r"C:\Temp\demo.txt"))
            .unwrap();

    assert_eq!(command.program, "cmd");
    assert_eq!(command.args[0], "/C");
    assert_eq!(command.args[1], "start");
}

#[test]
fn windows_open_file_location_uses_explorer_select() {
    let command = open_file_location_command_for_platform(
        DesktopPlatform::Windows,
        Path::new(r"C:\Temp\demo.txt"),
    )
    .unwrap();

    assert_eq!(command.program, "explorer");
    assert!(command.args[0].starts_with("/select,"));
}

#[test]
fn macos_open_file_location_reveals_in_finder() {
    let command = open_file_location_command_for_platform(
        DesktopPlatform::MacOS,
        Path::new("/tmp/demo.txt"),
    )
    .unwrap();

    assert_eq!(command.program, "open");
    assert_eq!(command.args, vec!["-R".to_string(), "/tmp/demo.txt".to_string()]);
}

#[test]
fn linux_open_file_location_falls_back_to_parent_directory() {
    let command = open_file_location_command_for_platform(
        DesktopPlatform::Linux,
        Path::new("/tmp/demo.txt"),
    )
    .unwrap();

    assert_eq!(command.program, "xdg-open");
    assert_eq!(command.args, vec!["/tmp".to_string()]);
}

#[test]
fn unsupported_platform_returns_error() {
    let error = open_path_command_for_platform(DesktopPlatform::Other, Path::new("/tmp/demo.txt"))
        .unwrap_err();

    assert!(error.contains("unsupported"));
}
