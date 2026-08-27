use std::fs::File;
use std::io::Write;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn create_multi_root_zip(path: &Path) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    archive
        .start_file("README.md", options)
        .expect("add root file");
    archive.write_all(b"read me\n").expect("write root file");
    archive
        .start_file("src/lib.rs", options)
        .expect("add nested file");
    archive
        .write_all(b"pub fn library() {}\n")
        .expect("write nested file");
    archive.finish().expect("finish ZIP fixture");
}

fn create_single_root_zip(path: &Path) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    archive
        .add_directory("project/", options)
        .expect("add root directory");
    archive
        .start_file("project/README.md", options)
        .expect("add project file");
    archive
        .write_all(b"project readme\n")
        .expect("write project file");
    archive.finish().expect("finish ZIP fixture");
}

fn create_traversal_zip(path: &Path) {
    let file = File::create(path).expect("create traversal ZIP fixture");
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("../escape.txt", SimpleFileOptions::default())
        .expect("add traversal entry");
    archive
        .write_all(b"must not escape\n")
        .expect("write traversal entry");
    archive.finish().expect("finish traversal fixture");
}

fn create_duplicate_tar(path: &Path) {
    let file = File::create(path).expect("create duplicate TAR fixture");
    let mut archive = tar::Builder::new(file);
    for content in [b"first".as_slice(), b"second".as_slice()] {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(u64::try_from(content.len()).expect("fixture size fits u64"));
        header.set_cksum();
        archive
            .append_data(&mut header, "same.txt", content)
            .expect("add duplicate entry");
    }
    archive.finish().expect("finish duplicate TAR fixture");
}

fn create_casefolded_parent_conflict_tar(path: &Path) {
    let file = File::create(path).expect("create case-folded conflict TAR fixture");
    let mut archive = tar::Builder::new(file);
    for (entry_path, content) in [
        ("Root", b"file".as_slice()),
        ("root/child", b"child".as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(u64::try_from(content.len()).expect("fixture size fits u64"));
        header.set_cksum();
        archive
            .append_data(&mut header, entry_path, content)
            .expect("add conflicting entry");
    }
    archive.finish().expect("finish conflict TAR fixture");
}

fn create_symlink_zip(path: &Path) {
    let file = File::create(path).expect("create symlink ZIP fixture");
    let mut archive = ZipWriter::new(file);
    archive
        .add_symlink("link.txt", "../outside.txt", SimpleFileOptions::default())
        .expect("add ZIP symlink");
    archive.finish().expect("finish symlink fixture");
}

#[cfg(unix)]
fn create_non_utf8_tar(path: &Path) {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let file = File::create(path).expect("create non-UTF-8 TAR fixture");
    let mut archive = tar::Builder::new(file);
    let content = b"escaped name";
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(u64::try_from(content.len()).expect("fixture size fits u64"));
    header.set_cksum();
    let raw_name = Path::new(OsStr::from_bytes(b"bad\xff.txt"));
    archive
        .append_data(&mut header, raw_name, content.as_slice())
        .expect("append non-UTF-8 entry");
    archive.finish().expect("finish non-UTF-8 TAR fixture");
}

#[test]
fn full_extract_uses_archive_stem_for_multiple_roots() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("bundle.zip");
    create_multi_root_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "bundle.zip", "--json"])
        .output()
        .expect("run arcthis");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(workspace.path().join("bundle/README.md")).expect("read root output"),
        b"read me\n"
    );
    assert_eq!(
        std::fs::read(workspace.path().join("bundle/src/lib.rs")).expect("read nested output"),
        b"pub fn library() {}\n"
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse extraction JSON");
    assert_eq!(value["extraction"]["entries_extracted"], 2);
    assert!(
        value["extraction"]["destination"]
            .as_str()
            .expect("destination string")
            .ends_with("/bundle")
    );
}

#[test]
fn full_extract_preserves_one_existing_top_level_directory() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("download.zip");
    create_single_root_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "download.zip"])
        .output()
        .expect("run arcthis");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(workspace.path().join("project/README.md")).expect("read output"),
        b"project readme\n"
    );
    assert!(!workspace.path().join("download").exists());
}

#[test]
fn single_entry_extract_commits_exact_output_file() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("bundle.zip");
    create_multi_root_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "extract",
            "bundle.zip",
            "README.md",
            "--output",
            "selected.txt",
        ])
        .output()
        .expect("run arcthis");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(workspace.path().join("selected.txt")).expect("read selected output"),
        b"read me\n"
    );
}

#[test]
fn extraction_refuses_collision_without_modifying_destination() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("bundle.zip");
    create_multi_root_zip(&archive_path);
    std::fs::create_dir(workspace.path().join("bundle")).expect("create collision directory");
    std::fs::write(workspace.path().join("bundle/keep.txt"), b"keep")
        .expect("write collision marker");

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "bundle.zip", "--json"])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(9));
    assert!(output.stdout.is_empty());
    assert_eq!(
        std::fs::read(workspace.path().join("bundle/keep.txt")).expect("read marker"),
        b"keep"
    );
    assert!(!workspace.path().join("bundle/README.md").exists());
}

#[test]
fn traversal_is_rejected_before_any_destination_is_committed() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("hostile.zip");
    create_traversal_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "hostile.zip", "--json"])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(7));
    assert!(output.stdout.is_empty());
    assert!(!workspace.path().join("hostile").exists());
    assert!(!workspace.path().join("escape.txt").exists());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse unsafe path JSON");
    assert_eq!(value["error"]["code"], "unsafe_path");
}

#[test]
fn declared_resource_limit_stops_before_staging() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("bundle.zip");
    create_multi_root_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "bundle.zip", "--max-total-size", "1", "--json"])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(8));
    assert!(!workspace.path().join("bundle").exists());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse limit JSON");
    assert_eq!(value["error"]["code"], "resource_limit");
}

#[test]
fn duplicate_paths_are_rejected_before_extraction() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("duplicate.tar");
    create_duplicate_tar(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "duplicate.tar", "--json"])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(9));
    assert!(!workspace.path().join("duplicate").exists());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse collision JSON");
    assert_eq!(value["error"]["code"], "collision");
}

#[test]
fn case_insensitive_file_parent_conflicts_are_rejected_before_extraction() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("casefold.tar");
    create_casefolded_parent_conflict_tar(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "casefold.tar", "--json"])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(9));
    assert!(!workspace.path().join("casefold").exists());
    let value: Value = serde_json::from_slice(&output.stderr).expect("parse collision JSON");
    assert_eq!(value["error"]["code"], "collision");
}

#[test]
fn archive_symlinks_are_rejected_before_extraction() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("links.zip");
    create_symlink_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "links.zip", "--json"])
        .output()
        .expect("run arcthis");

    assert_eq!(output.status.code(), Some(7));
    assert!(!workspace.path().join("links").exists());
    assert!(!workspace.path().join("outside.txt").exists());
}

#[test]
fn inspect_warns_about_paths_safe_extraction_will_reject() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("hostile.zip");
    create_traversal_zip(&archive_path);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "inspect",
            archive_path.to_str().expect("UTF-8 test path"),
            "--json",
        ])
        .output()
        .expect("run arcthis");

    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse inspect JSON");
    assert!(
        value["warnings"]
            .as_array()
            .expect("warning array")
            .iter()
            .any(|warning| warning["code"] == "unsafe_entry_paths")
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_entry_names_are_explicit_and_not_materialized() {
    let workspace = TempDir::new().expect("create test directory");
    let archive_path = workspace.path().join("bytes.tar");
    create_non_utf8_tar(&archive_path);

    let list = cargo_bin_cmd!("arcthis")
        .args([
            "list",
            archive_path.to_str().expect("UTF-8 test path"),
            "--json",
        ])
        .output()
        .expect("run list");
    assert!(list.status.success());
    let value: Value = serde_json::from_slice(&list.stdout).expect("parse list JSON");
    assert_eq!(value["entries"][0]["path"], "bad%FF.txt");
    assert_eq!(value["entries"][0]["path_encoding"], "escaped_bytes");

    let extract = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["extract", "bytes.tar", "--json"])
        .output()
        .expect("run extract");
    assert_eq!(extract.status.code(), Some(7));
    assert!(!workspace.path().join("bytes").exists());
}
