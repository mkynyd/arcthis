use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Stdio;

use assert_cmd::cargo::cargo_bin_cmd;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::Value;
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn create_zip(path: &Path) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    archive
        .add_directory("src/", options)
        .expect("add ZIP directory");
    archive
        .start_file("README.md", options)
        .expect("add ZIP file");
    archive.write_all(b"# fixture\n").expect("write ZIP file");
    archive
        .start_file("src/lib.rs", options)
        .expect("add nested ZIP file");
    archive
        .write_all(b"pub fn fixture() {}\n")
        .expect("write nested ZIP file");
    archive.finish().expect("finish ZIP fixture");
}

fn create_tar_gzip(path: &Path) {
    let file = File::create(path).expect("create TAR.GZ fixture");
    let gzip = GzEncoder::new(file, Compression::default());
    let mut archive = tar::Builder::new(gzip);
    let content = b"hello from tar\n";
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(content.len().try_into().expect("fixture size fits u64"));
    header.set_cksum();
    archive
        .append_data(&mut header, "docs/hello.txt", &content[..])
        .expect("append TAR file");
    let gzip = archive.into_inner().expect("finish TAR archive");
    gzip.finish().expect("finish Gzip stream");
}

fn create_large_zip(path: &Path) {
    let file = File::create(path).expect("create large ZIP fixture");
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("large.bin", SimpleFileOptions::default())
        .expect("add large ZIP file");
    let block = vec![b'x'; 65_536];
    for _ in 0..64 {
        archive.write_all(&block).expect("write large ZIP entry");
    }
    archive.finish().expect("finish large ZIP fixture");
}

fn create_deep_zip(path: &Path) {
    let file = File::create(path).expect("create deep ZIP fixture");
    let mut archive = ZipWriter::new(file);
    let entry = format!("{}payload.txt", "d/".repeat(300));
    archive
        .start_file(entry, SimpleFileOptions::default())
        .expect("add deep ZIP file");
    archive.write_all(b"deep").expect("write deep ZIP file");
    archive.finish().expect("finish deep ZIP fixture");
}

#[test]
fn tree_rejects_paths_above_the_component_limit() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("deep.zip");
    create_deep_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "tree",
            archive_path.to_str().expect("UTF-8 test path"),
            "--json",
        ])
        .output()
        .expect("run arcthis tree");

    assert_eq!(output.status.code(), Some(8));
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse resource error");
    assert_eq!(value["error"]["code"], "resource_limit");
}

#[test]
fn zero_prefixed_streams_preserve_content_and_empty_tar_uses_explicit_suffix() {
    let workspace = TempDir::new().expect("test directory");
    let source = workspace.path().join("payload.bin");
    let mut content = vec![0; 1024];
    content.extend_from_slice(b"payload after zero blocks");
    for payload in [&content[..], &[0_u8; 1024][..]] {
        std::fs::write(&source, payload).expect("write zero-prefixed source");
        for (suffix, format) in [
            ("gz", "gzip"),
            ("bz2", "bzip2"),
            ("xz", "xz"),
            ("zst", "zstd"),
        ] {
            let output_path = workspace.path().join(format!("payload.bin.{suffix}"));
            cargo_bin_cmd!("arcthis")
                .arg("pack")
                .arg(&source)
                .arg("--output")
                .arg(&output_path)
                .arg("--overwrite")
                .assert()
                .success();
            let inspected = cargo_bin_cmd!("arcthis")
                .arg("inspect")
                .arg(&output_path)
                .arg("--json")
                .output()
                .expect("inspect stream");
            assert!(inspected.status.success());
            let value: Value = serde_json::from_slice(&inspected.stdout).expect("inspect JSON");
            assert_eq!(value["archive"]["format"], format);
            cargo_bin_cmd!("arcthis")
                .arg("read")
                .arg(&output_path)
                .arg("payload.bin")
                .assert()
                .success()
                .stdout(payload.to_vec());
            cargo_bin_cmd!("arcthis")
                .arg("verify")
                .arg(&output_path)
                .assert()
                .success();
        }
    }
    for (suffix, format) in [
        ("tar.gz", "tar_gzip"),
        ("tar.bz2", "tar_bzip2"),
        ("tar.xz", "tar_xz"),
        ("tar.zst", "tar_zstd"),
    ] {
        let archive = workspace.path().join(format!("empty.{suffix}"));
        // Canonical empty TAR consists only of its end-of-archive zero blocks.
        let file = File::create(&archive).expect("empty TAR fixture");
        match suffix {
            "tar.gz" => {
                let mut writer = GzEncoder::new(file, Compression::default());
                writer.write_all(&[0; 1024]).expect("zero TAR blocks");
                writer.finish().expect("finish Gzip");
            }
            "tar.bz2" => {
                let mut writer = bzip2::write::BzEncoder::new(file, bzip2::Compression::default());
                writer.write_all(&[0; 1024]).expect("zero TAR blocks");
                writer.finish().expect("finish Bzip2");
            }
            "tar.xz" => {
                let mut writer =
                    lzma_rust2::XzWriter::new(file, lzma_rust2::XzOptions::default()).expect("XZ");
                writer.write_all(&[0; 1024]).expect("zero TAR blocks");
                writer.finish().expect("finish XZ");
            }
            _ => {
                let mut writer = zstd::stream::write::Encoder::new(file, 3).expect("Zstandard");
                writer.write_all(&[0; 1024]).expect("zero TAR blocks");
                writer.finish().expect("finish Zstandard");
            }
        }
        let output = cargo_bin_cmd!("arcthis")
            .arg("list")
            .arg(&archive)
            .arg("--json")
            .output()
            .expect("list empty TAR");
        assert!(output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).expect("list JSON");
        assert_eq!(value["archive"]["format"], format);
        assert_eq!(value["entries"].as_array().expect("entries array").len(), 0);
    }
}

#[test]
fn list_uses_magic_bytes_and_emits_structured_json() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("misleading.bin");
    create_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "list",
            archive_path.to_str().expect("UTF-8 test path"),
            "--json",
        ])
        .output()
        .expect("run arcthis");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse list JSON");
    assert_eq!(value["schema_version"], "1");
    assert_eq!(value["archive"]["format"], "zip");
    assert_eq!(value["entries"].as_array().expect("entries array").len(), 3);
    assert_eq!(value["entries"][1]["path"], "README.md");
}

#[test]
fn tree_reads_tar_gzip_and_builds_recursive_nodes() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("bundle.data");
    create_tar_gzip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "tree",
            archive_path.to_str().expect("UTF-8 test path"),
            "--json",
        ])
        .output()
        .expect("run arcthis");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse tree JSON");
    assert_eq!(value["archive"]["format"], "tar_gzip");
    assert_eq!(value["tree"][0]["name"], "docs");
    assert_eq!(value["tree"][0]["children"][0]["path"], "docs/hello.txt");
}

#[test]
fn machine_error_uses_stderr_and_stable_exit_code() {
    let workspace = TempDir::new().expect("create test directory");
    let input = workspace.path().join("not-an-archive.txt");
    std::fs::write(&input, b"plain text").expect("write invalid fixture");

    let output = cargo_bin_cmd!("arcthis")
        .args(["list", input.to_str().expect("UTF-8 test path"), "--json"])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse error JSON");
    assert_eq!(value["schema_version"], "1");
    assert_eq!(value["error"]["code"], "unsupported_format");
}

#[test]
fn read_streams_exact_entry_bytes() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("source.zip");
    create_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "read",
            archive_path.to_str().expect("UTF-8 test path"),
            "src/lib.rs",
        ])
        .output()
        .expect("run arcthis");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"pub fn fixture() {}\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn stat_missing_entry_is_a_structured_machine_error() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("source.zip");
    create_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "stat",
            archive_path.to_str().expect("UTF-8 test path"),
            "missing.txt",
            "--json",
        ])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse stat error JSON");
    assert_eq!(value["error"]["code"], "entry_not_found");
    assert_eq!(value["error"]["details"]["entry"], "missing.txt");
}

#[test]
fn inspect_reports_sequential_tar_gzip_capabilities() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("source.tar.gz");
    create_tar_gzip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "inspect",
            archive_path.to_str().expect("UTF-8 test path"),
            "--json",
        ])
        .output()
        .expect("run arcthis");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse inspect JSON");
    assert_eq!(value["random_access"], false);
    assert_eq!(value["capabilities"]["streaming_read"], true);
    assert_eq!(value["warnings"][0]["code"], "sequential_access");
}

#[test]
fn read_treats_broken_pipe_as_success() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("large.zip");
    create_large_zip(&archive_path);

    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin!("arcthis"))
        .args([
            "read",
            archive_path.to_str().expect("UTF-8 test path"),
            "large.bin",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn arcthis");
    let mut stdout = child.stdout.take().expect("capture stdout");
    let mut prefix = [0_u8; 1];
    stdout.read_exact(&mut prefix).expect("read entry prefix");
    drop(stdout);

    let status = child.wait().expect("wait for arcthis");
    assert!(status.success(), "broken pipe exit status: {status}");
}
