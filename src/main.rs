#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::io::Cursor;

use codex_history_migrator::app::MigratorApp;
use codex_history_migrator::ui::fonts::configure_chinese_fonts;
use eframe::egui;

fn default_window_size() -> [f32; 2] {
    [1480.0, 920.0]
}

fn default_min_window_size() -> [f32; 2] {
    [1180.0, 760.0]
}

fn default_viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_inner_size(default_window_size())
        .with_min_inner_size(default_min_window_size())
}

fn main() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions::default();
    let viewport = if let Some(icon) = load_app_icon() {
        default_viewport().with_icon(icon)
    } else {
        default_viewport()
    };
    options.viewport = viewport;

    eframe::run_native(
        "Codex 历史迁移与同步工具",
        options,
        Box::new(|cc| {
            let mut app = MigratorApp::default();
            if let Some(font_path) = configure_chinese_fonts(&cc.egui_ctx) {
                app.log(format!("已加载中文字体：{}", font_path.display()));
            } else {
                app.log("未找到可用的中文字体，界面可能仍会出现显示异常。");
            }
            Ok(Box::new(app))
        }),
    )
}

fn load_app_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../assets/app-icon.png");
    let decoder = png::Decoder::new(Cursor::new(icon_bytes.as_slice()));
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).ok()?;
    let rgba = buffer[..info.buffer_size()].to_vec();

    Some(egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    })
}

#[cfg(test)]
mod tests {
    use super::{default_min_window_size, default_window_size};

    #[test]
    fn desktop_window_defaults_are_large_enough_for_manage_layout() {
        assert_eq!(default_window_size(), [1480.0, 920.0]);
        assert_eq!(default_min_window_size(), [1180.0, 760.0]);
    }
}
