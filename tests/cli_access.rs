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
