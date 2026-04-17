use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};
use crate::platform::preferred_cjk_font_candidates_for_current_platform;

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

pub fn configure_ui_fonts(ctx: &Context) -> Option<PathBuf> {
    let candidates = preferred_cjk_font_candidates_for_current_platform();
    let chosen = candidates.iter().find(|path| path.exists()).cloned();
    let fonts = build_font_definitions_from_candidates(&candidates);
    ctx.set_fonts(fonts);
    chosen
}
