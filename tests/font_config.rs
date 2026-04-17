use std::fs;
use std::path::PathBuf;

use codex_history_migrator::platform::{
    DesktopPlatform, preferred_cjk_font_candidates_for_platform,
};
use codex_history_migrator::ui::fonts::build_font_definitions_from_candidates;
use tempfile::tempdir;

#[test]
fn windows_candidates_include_common_chinese_fonts() {
    let candidates = preferred_cjk_font_candidates_for_platform(DesktopPlatform::Windows);

    assert!(candidates.iter().any(|path: &PathBuf| path.ends_with("msyh.ttc")));
    assert!(candidates.iter().any(|path: &PathBuf| path.ends_with("DENG.TTF")));
    assert!(candidates.iter().any(|path: &PathBuf| path.ends_with("simsun.ttc")));
}

#[test]
fn macos_candidates_include_common_built_in_cjk_fonts() {
    let candidates = preferred_cjk_font_candidates_for_platform(DesktopPlatform::MacOS);

    assert!(candidates.iter().any(|path: &PathBuf| path.ends_with("PingFang.ttc")));
    assert!(
        candidates
            .iter()
            .any(|path: &PathBuf| path.ends_with("Hiragino Sans GB.ttc"))
    );
}

#[test]
fn linux_candidates_include_common_cjk_fonts() {
    let candidates = preferred_cjk_font_candidates_for_platform(DesktopPlatform::Linux);

    assert!(
        candidates
            .iter()
            .any(|path: &PathBuf| path.ends_with("NotoSansCJK-Regular.ttc"))
    );
    assert!(candidates.iter().any(|path: &PathBuf| path.ends_with("wqy-zenhei.ttc")));
}

#[test]
fn windows_candidates_prioritize_ttf_before_ttc() {
    let candidates = preferred_cjk_font_candidates_for_platform(DesktopPlatform::Windows);
    let deng_index = candidates
        .iter()
        .position(|path: &PathBuf| path.ends_with("DENG.TTF"))
        .unwrap();
    let msyh_index = candidates
        .iter()
        .position(|path: &PathBuf| path.ends_with("msyh.ttc"))
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
