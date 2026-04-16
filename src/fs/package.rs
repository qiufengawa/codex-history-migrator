use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};
use walkdir::WalkDir;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub fn write_zip_from_dir(source_dir: &Path, output_file: &Path) -> Result<()> {
    write_zip_from_dir_with_progress(source_dir, output_file, |_, _| {})
}

pub fn write_zip_from_dir_with_progress<F>(
    source_dir: &Path,
    output_file: &Path,
    mut progress: F,
) -> Result<()>
where
    F: FnMut(usize, usize),
{
    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let files = collect_files(source_dir)?;
    let total_files = files.len();
    let file = File::create(output_file)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    if total_files == 0 {
        progress(0, 0);
    }

    for (index, path) in files.iter().enumerate() {
        let relative = path.strip_prefix(source_dir)?;
        let relative_name = relative.to_string_lossy().replace('\\', "/");
        writer.start_file(relative_name, options)?;

        let mut input = File::open(path)?;
        let mut buffer = Vec::new();
        input.read_to_end(&mut buffer)?;
        writer.write_all(&buffer)?;

        progress(index + 1, total_files);
    }

    writer.finish()?;
    Ok(())
}

pub fn unpack_zip_to_dir(package_file: &Path, output_dir: &Path) -> Result<()> {
    unpack_zip_to_dir_with_progress(package_file, output_dir, |_, _| {})
}

pub fn unpack_zip_to_dir_with_progress<F>(
    package_file: &Path,
    output_dir: &Path,
    mut progress: F,
) -> Result<()>
where
    F: FnMut(usize, usize),
{
    fs::create_dir_all(output_dir)?;
    let file = File::open(package_file)?;
    let mut archive = ZipArchive::new(file)?;
    let total_entries = archive.len();

    if total_entries == 0 {
        progress(0, 0);
    }

    for index in 0..total_entries {
        let mut entry = archive.by_index(index)?;
        let relative_path = sanitize_archive_entry_path(entry.name())?;
        let out_path = output_dir.join(relative_path);

        if entry.name().ends_with('/') {
            fs::create_dir_all(&out_path)?;
            progress(index + 1, total_entries);
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output = File::create(out_path)?;
        std::io::copy(&mut entry, &mut output)?;
        progress(index + 1, total_entries);
    }

    Ok(())
}

fn collect_files(source_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if path.is_file() {
            files.push(path.to_path_buf());
        }
    }

    Ok(files)
}

fn sanitize_archive_entry_path(entry_name: &str) -> Result<PathBuf> {
    let mut sanitized = PathBuf::new();

    for component in Path::new(entry_name).components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                bail!("archive entry points outside output directory: {entry_name}")
            }
            Component::RootDir | Component::Prefix(_) => {
                bail!("archive entry must be relative: {entry_name}")
            }
        }
    }

    if sanitized.as_os_str().is_empty() {
        bail!("archive entry path is empty");
    }

    Ok(sanitized)
}
