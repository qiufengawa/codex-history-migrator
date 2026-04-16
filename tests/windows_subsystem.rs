use std::fs;

#[test]
fn main_binary_declares_windows_gui_subsystem() {
    let main_rs = fs::read_to_string("src/main.rs").expect("should read src/main.rs");

    assert!(
        main_rs.contains(r#"#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]"#),
        "Windows GUI builds should opt into the windows subsystem to avoid spawning a console window"
    );
}
