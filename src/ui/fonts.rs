use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn preferred_windows_cjk_font_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Windows\Fonts\DENG.TTF"),
        PathBuf::from(r"C:\Windows\Fonts\DENGL.TTF"),
        PathBuf::from(r"C:\Windows\Fonts\DENGB.TTF"),
        PathBuf::from(r"C:\Windows\Fonts\simsunb.ttf"),
        PathBuf::from(r"C:\Windows\Fonts\SimsunExtG.ttf"),
        // egui/epaint documents the font loader around TTF/OTF inputs,
        // so we keep TTC collections as a later fallback on Windows.
        PathBuf::from(r"C:\Windows\Fonts\msyh.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\msyhbd.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\msyhl.ttc"),
        PathBuf::from(r"C:\Windows\Fonts\simsun.ttc"),
    ]
}

pub fn build_font_definitions_from_candidates(candidates: &[PathBuf]) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    if let Some(font_path) = candidates.iter().find(|path| path.exists()) {
        if let Ok(bytes) = fs::read(font_path) {
            fonts.font_data.insert(
                "cjk_fallback".to_string(),
                Arc::new(FontData::from_owned(bytes)),
            );

            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "cjk_fallback".to_string());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("cjk_fallback".to_string());
        }
    }

    fonts
}

pub fn configure_chinese_fonts(ctx: &Context) -> Option<PathBuf> {
    let candidates = preferred_windows_cjk_font_candidates();
    let chosen = candidates.iter().find(|path| path.exists()).cloned();
    let fonts = build_font_definitions_from_candidates(&candidates);
    ctx.set_fonts(fonts);
    chosen
}
