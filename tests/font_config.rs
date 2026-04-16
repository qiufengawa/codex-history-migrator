use std::fs;

use codex_history_migrator::ui::fonts::{
    build_font_definitions_from_candidates, preferred_windows_cjk_font_candidates,
};
use tempfile::tempdir;

#[test]
fn preferred_windows_cjk_font_candidates_prioritize_common_chinese_fonts() {
    let candidates = preferred_windows_cjk_font_candidates();

    assert!(candidates.iter().any(|path| path.ends_with("msyh.ttc")));
    assert!(candidates.iter().any(|path| path.ends_with("DENG.TTF")));
    assert!(candidates.iter().any(|path| path.ends_with("simsun.ttc")));
}

#[test]
fn preferred_windows_cjk_font_candidates_prioritize_ttf_before_ttc() {
    let candidates = preferred_windows_cjk_font_candidates();
    let deng_index = candidates
        .iter()
        .position(|path| path.ends_with("DENG.TTF"))
        .unwrap();
    let msyh_index = candidates
        .iter()
        .position(|path| path.ends_with("msyh.ttc"))
        .unwrap();

    assert!(
        deng_index < msyh_index,
        "single-font TTF fallbacks should be preferred before TTC collections"
    );
}

#[test]
fn build_font_definitions_uses_first_existing_candidate() {
    let temp = tempdir().unwrap();
    let font_path = temp.path().join("fake-cjk.ttf");
    fs::write(
        &font_path,
        b"not-a-real-font-but-good-enough-for-registration",
    )
    .unwrap();

    let fonts = build_font_definitions_from_candidates(&[font_path]);

    let proportional = fonts
        .families
        .get(&eframe::egui::FontFamily::Proportional)
        .unwrap();

    assert_eq!(proportional.first().unwrap(), "cjk_fallback");
    assert!(fonts.font_data.contains_key("cjk_fallback"));
}
