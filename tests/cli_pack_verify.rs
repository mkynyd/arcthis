use std::fs::File;
use std::io::Write;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn create_source(root: &Path) {
    std::fs::create_dir_all(root.join("empty")).expect("create empty directory");
    std::fs::create_dir_all(root.join("资料")).expect("create Unicode directory");
    std::fs::write(root.join("README.md"), b"hello archive\n").expect("write README");
    std::fs::write(root.join("zero.bin"), b"").expect("write empty file");
    std::fs::write(root.join("资料/说明.txt"), "你好，arcthis\n").expect("write Unicode file");
}

fn create_corrupted_zip(path: &Path) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    archive
        .start_file("data.txt", options)
        .expect("start ZIP entry");
    archive
        .write_all(b"original bytes")
        .expect("write ZIP entry");
    archive.finish().expect("finish ZIP fixture");

    let mut bytes = std::fs::read(path).expect("read ZIP bytes");
    let name_length = usize::from(u16::from_le_bytes([bytes[26], bytes[27]]));
    let extra_length = usize::from(u16::from_le_bytes([bytes[28], bytes[29]]));
    let data_offset = 30 + name_length + extra_length;
    bytes[data_offset] ^= 0xff;
    std::fs::write(path, bytes).expect("write corrupted ZIP fixture");
}

#[test]
fn pack_finalize_verify_and_reopen_all_v01_formats() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("project");
    create_source(&source);

    for (index, output_name) in ["backup.zip", "backup.tar", "backup.tar.gz"]
        .into_iter()
        .enumerate()
    {
        let output = cargo_bin_cmd!("arcthis")
            .current_dir(workspace.path())
            .args(["pack", "project", "--output", output_name, "--json"])
            .output()
            .expect("run pack");
        assert!(
            output.status.success(),
            "{output_name} stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).expect("parse pack JSON");
        assert_eq!(value["pack"]["verification"]["verified"], true);
        assert!(workspace.path().join(output_name).is_file());

        let verify = cargo_bin_cmd!("arcthis")
            .current_dir(workspace.path())
            .args(["verify", output_name, "--json"])
            .output()
            .expect("run verify");
        assert!(
            verify.status.success(),
            "{output_name} verify stderr: {}",
            String::from_utf8_lossy(&verify.stderr)
        );
        let value: Value = serde_json::from_slice(&verify.stdout).expect("parse verify JSON");
        assert_eq!(value["verification"]["verified"], true);
        assert_eq!(value["verification"]["entries_checked"], 6);

        let list = cargo_bin_cmd!("arcthis")
            .current_dir(workspace.path())
            .args(["list", output_name, "--json"])
            .output()
            .expect("run list");
        let value: Value = serde_json::from_slice(&list.stdout).expect("parse list JSON");
        assert!(
            value["entries"]
                .as_array()
                .expect("entries array")
                .iter()
                .any(|entry| entry["path"] == "project/资料/说明.txt")
        );

        let restored = format!("restored-{index}");
        let extract = cargo_bin_cmd!("arcthis")
            .current_dir(workspace.path())
            .args(["extract", output_name, "--output", restored.as_str()])
            .output()
            .expect("run extract");
        assert!(
            extract.status.success(),
            "{output_name} extract stderr: {}",
            String::from_utf8_lossy(&extract.stderr)
        );
        assert_eq!(
            std::fs::read(workspace.path().join(&restored).join("project/README.md"))
                .expect("read round-trip README"),
            b"hello archive\n"
        );
        assert!(
            workspace
                .path()
                .join(&restored)
                .join("project/empty")
                .is_dir()
        );
    }
}

#[test]
fn pack_refuses_existing_output_without_modifying_it() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("project");
    create_source(&source);
    std::fs::write(workspace.path().join("backup.zip"), b"keep").expect("write existing output");

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["pack", "project", "--output", "backup.zip", "--json"])
        .output()
        .expect("run pack");

    assert_eq!(output.status.code(), Some(9));
    assert_eq!(
        std::fs::read(workspace.path().join("backup.zip")).expect("read existing output"),
        b"keep"
    );
    assert!(source.join("README.md").is_file());
}

#[test]
fn verify_reports_crc_failure_with_stable_category() {
    let workspace = TempDir::new().expect("create test directory");
    let archive = workspace.path().join("corrupted.zip");
    create_corrupted_zip(&archive);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "verify",
            archive.to_str().expect("UTF-8 test path"),
            "--json",
        ])
        .output()
        .expect("run verify");

    assert_eq!(output.status.code(), Some(11));
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse verify error JSON");
    assert_eq!(value["error"]["code"], "verification_failed");
}

#[cfg(unix)]
#[test]
fn pack_rejects_symlinks_and_leaves_no_output() {
    use std::os::unix::fs::symlink;

    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("project");
    create_source(&source);
    symlink("README.md", source.join("link.txt")).expect("create symlink");

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["pack", "project", "--output", "backup.zip", "--json"])
        .output()
        .expect("run pack");

    assert_eq!(output.status.code(), Some(10));
    assert!(!workspace.path().join("backup.zip").exists());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse pack error JSON");
    assert_eq!(value["error"]["code"], "unsupported_operation");
}
