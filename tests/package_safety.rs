use std::fs;
use std::fs::File;

use codex_history_migrator::fs::package::unpack_zip_to_dir;
use tempfile::tempdir;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[test]
fn unpack_rejects_entries_that_escape_output_directory() {
    let temp = tempdir().unwrap();
    let package = temp.path().join("malicious.codexhist");
    let output_dir = temp.path().join("out");
    let escaped_path = temp.path().join("escaped.txt");

    let file = File::create(&package).unwrap();
    let mut writer = ZipWriter::new(file);
    writer
        .start_file(
            "../escaped.txt",
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .unwrap();
    std::io::Write::write_all(&mut writer, b"owned").unwrap();
    writer.finish().unwrap();

    let error = unpack_zip_to_dir(&package, &output_dir).unwrap_err();

    assert!(error.to_string().contains("outside"));
    assert!(!escaped_path.exists());
    assert!(
        !fs::read_dir(&output_dir)
            .map(|mut it| it.next().is_some())
            .unwrap_or(false)
    );
}
