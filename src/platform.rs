use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopPlatform {
    Windows,
    MacOS,
    Linux,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCommand {
    pub program: String,
    pub args: Vec<String>,
}

pub fn current_platform() -> DesktopPlatform {
    #[cfg(target_os = "windows")]
    {
        return DesktopPlatform::Windows;
    }
    #[cfg(target_os = "macos")]
    {
        return DesktopPlatform::MacOS;
    }
    #[cfg(target_os = "linux")]
    {
        return DesktopPlatform::Linux;
    }
    #[allow(unreachable_code)]
    DesktopPlatform::Other
}

pub fn preferred_cjk_font_candidates_for_current_platform() -> Vec<PathBuf> {
    preferred_cjk_font_candidates_for_platform(current_platform())
}

pub fn preferred_cjk_font_candidates_for_platform(platform: DesktopPlatform) -> Vec<PathBuf> {
    match platform {
        DesktopPlatform::Windows => vec![
            PathBuf::from(r"C:\Windows\Fonts\DENG.TTF"),
            PathBuf::from(r"C:\Windows\Fonts\DENGL.TTF"),
            PathBuf::from(r"C:\Windows\Fonts\DENGB.TTF"),
            PathBuf::from(r"C:\Windows\Fonts\simsunb.ttf"),
            PathBuf::from(r"C:\Windows\Fonts\SimsunExtG.ttf"),
            PathBuf::from(r"C:\Windows\Fonts\msyh.ttc"),
            PathBuf::from(r"C:\Windows\Fonts\msyhbd.ttc"),
            PathBuf::from(r"C:\Windows\Fonts\msyhl.ttc"),
            PathBuf::from(r"C:\Windows\Fonts\simsun.ttc"),
        ],
        DesktopPlatform::MacOS => vec![
            PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
            PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
            PathBuf::from("/System/Library/Fonts/STHeiti Light.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/Songti.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/STHeiti Medium.ttc"),
            PathBuf::from("/Library/Fonts/Arial Unicode.ttf"),
        ],
        DesktopPlatform::Linux => vec![
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJKSC-Regular.otf"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc"),
            PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc"),
            PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"),
            PathBuf::from("/usr/share/fonts/opentype/source-han-sans/SourceHanSansSC-Regular.otf"),
        ],
        DesktopPlatform::Other => Vec::new(),
    }
}

pub fn open_path_command(path: &Path) -> Result<PlatformCommand, String> {
    open_path_command_for_platform(current_platform(), path)
}

pub fn open_path_command_for_platform(
    platform: DesktopPlatform,
    path: &Path,
) -> Result<PlatformCommand, String> {
    let path_string = path.to_string_lossy().to_string();
    match platform {
        DesktopPlatform::Windows => Ok(PlatformCommand {
            program: "cmd".to_string(),
            args: vec![
                "/C".to_string(),
                "start".to_string(),
                String::new(),
                path_string,
            ],
        }),
        DesktopPlatform::MacOS => Ok(PlatformCommand {
            program: "open".to_string(),
            args: vec![path_string],
        }),
        DesktopPlatform::Linux => Ok(PlatformCommand {
            program: "xdg-open".to_string(),
            args: vec![path_string],
        }),
        DesktopPlatform::Other => Err("unsupported platform for open_path".to_string()),
    }
}

pub fn open_file_location_command(path: &Path) -> Result<PlatformCommand, String> {
    open_file_location_command_for_platform(current_platform(), path)
}

pub fn open_file_location_command_for_platform(
    platform: DesktopPlatform,
    path: &Path,
) -> Result<PlatformCommand, String> {
    let path_string = path.to_string_lossy().to_string();
    match platform {
        DesktopPlatform::Windows => Ok(PlatformCommand {
            program: "explorer".to_string(),
            args: vec![format!("/select,{path_string}")],
        }),
        DesktopPlatform::MacOS => Ok(PlatformCommand {
            program: "open".to_string(),
            args: vec!["-R".to_string(), path_string],
        }),
        DesktopPlatform::Linux => {
            let directory = path
                .parent()
                .ok_or_else(|| "file location has no parent directory".to_string())?;
            Ok(PlatformCommand {
                program: "xdg-open".to_string(),
                args: vec![directory.to_string_lossy().to_string()],
            })
        }
        DesktopPlatform::Other => Err("unsupported platform for open_file_location".to_string()),
    }
}

pub fn run_platform_command(command: &PlatformCommand) -> Result<(), String> {
    Command::new(&command.program)
        .args(&command.args)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}
