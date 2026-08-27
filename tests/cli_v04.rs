use std::fs::File;
use std::io::Write;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn create_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = ZipWriter::new(file);
    for (name, content) in entries {
        archive
            .start_file(*name, SimpleFileOptions::default())
            .expect("start ZIP entry");
        archive.write_all(content).expect("write ZIP entry");
    }
    archive.finish().expect("finish ZIP fixture");
}

fn create_encrypted_zip(path: &Path, password: &str) {
    let file = File::create(path).expect("create encrypted ZIP fixture");
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, password);
    archive
        .start_file("secret.txt", options)
        .expect("start encrypted ZIP entry");
    archive
        .write_all(b"encrypted ZIP archive\n")
        .expect("write encrypted ZIP entry");
    archive.finish().expect("finish encrypted ZIP fixture");
}

fn parse_stdout(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse JSON stdout")
}

#[test]
fn encrypted_seven_zip_requires_and_accepts_password_file() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("secret.txt");
    let archive = workspace.path().join("secret.7z");
    let password_file = workspace.path().join("password.txt");
    let wrong_password_file = workspace.path().join("wrong.txt");
    std::fs::write(&source, b"encrypted archive\n").expect("write source");
    std::fs::write(&password_file, b"correct horse\n").expect("write password");
    std::fs::write(&wrong_password_file, b"wrong\n").expect("write wrong password");
    sevenz_rust2::compress_to_path_encrypted(
        &source,
        &archive,
        sevenz_rust2::Password::new("correct horse"),
    )
    .expect("create encrypted 7z");

    let missing = cargo_bin_cmd!("arcthis")
        .args(["verify", archive.to_str().expect("path"), "--json"])
        .output()
        .expect("verify without password");
    assert_eq!(missing.status.code(), Some(10));
    let error: Value = serde_json::from_slice(&missing.stderr).expect("parse password error");
    assert_eq!(
        error["error"]["code"],
        "password_required",
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );

    let wrong = cargo_bin_cmd!("arcthis")
        .args([
            "verify",
            archive.to_str().expect("path"),
            "--password-file",
            wrong_password_file.to_str().expect("path"),
            "--json",
        ])
        .output()
        .expect("verify wrong password");
    assert_eq!(wrong.status.code(), Some(10));
    let error: Value = serde_json::from_slice(&wrong.stderr).expect("parse wrong password error");
    assert_eq!(error["error"]["code"], "wrong_password");

    let read = cargo_bin_cmd!("arcthis")
        .args([
            "read",
            archive.to_str().expect("path"),
            "secret.txt",
            "--password-file",
            password_file.to_str().expect("path"),
        ])
        .output()
        .expect("read encrypted entry");
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert_eq!(read.stdout, b"encrypted archive\n");

    let extracted = workspace.path().join("restored.txt");
    let extract = cargo_bin_cmd!("arcthis")
        .args([
            "extract",
            archive.to_str().expect("path"),
            "secret.txt",
            "--output",
            extracted.to_str().expect("path"),
            "--password-file",
            password_file.to_str().expect("path"),
            "--json",
        ])
        .output()
        .expect("extract encrypted entry");
    parse_stdout(&extract);
    assert_eq!(
        std::fs::read(extracted).expect("read extraction"),
        b"encrypted archive\n"
    );
}

#[test]
fn encrypted_zip_inspects_and_reads_with_password_file() {
    let workspace = TempDir::new().expect("create test directory");
    let archive = workspace.path().join("secret.zip");
    let password_file = workspace.path().join("password.txt");
    create_encrypted_zip(&archive, "zip secret");
    std::fs::write(&password_file, b"zip secret\n").expect("write password");

    let inspect = cargo_bin_cmd!("arcthis")
        .args(["inspect", archive.to_str().expect("path"), "--json"])
        .output()
        .expect("inspect encrypted ZIP");
    let value = parse_stdout(&inspect);
    assert_eq!(value["encrypted"], true);
    assert_eq!(value["capabilities"]["encrypted"], true);
    assert_eq!(value["warnings"][0]["code"], "encrypted_entries");

    let missing = cargo_bin_cmd!("arcthis")
        .args(["verify", archive.to_str().expect("path"), "--json"])
        .output()
        .expect("verify encrypted ZIP without password");
    assert_eq!(missing.status.code(), Some(10));
    let error: Value = serde_json::from_slice(&missing.stderr).expect("parse password error");
    assert_eq!(error["error"]["code"], "password_required");

    let read = cargo_bin_cmd!("arcthis")
        .args([
            "read",
            archive.to_str().expect("path"),
            "secret.txt",
            "--password-file",
            password_file.to_str().expect("path"),
        ])
        .output()
        .expect("read encrypted ZIP");
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert_eq!(read.stdout, b"encrypted ZIP archive\n");
}

#[test]
fn explicit_byte_stream_volumes_support_list_read_and_verify() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("volume.txt");
    let whole = workspace.path().join("whole.7z");
    std::fs::write(&source, b"multipart archive\n").expect("write source");
    sevenz_rust2::compress_to_path(&source, &whole).expect("create 7z");
    let bytes = std::fs::read(&whole).expect("read complete 7z");
    let first_end = bytes.len() / 3;
    let second_end = first_end * 2;
    let first = workspace.path().join("split.7z.001");
    let second = workspace.path().join("split.7z.002");
    let third = workspace.path().join("split.7z.003");
    std::fs::write(&first, &bytes[..first_end]).expect("write first volume");
    std::fs::write(&second, &bytes[first_end..second_end]).expect("write second volume");
    std::fs::write(&third, &bytes[second_end..]).expect("write third volume");

    let volume_args = [
        "--volume",
        second.to_str().expect("path"),
        "--volume",
        third.to_str().expect("path"),
    ];
    let list = cargo_bin_cmd!("arcthis")
        .args(["list", first.to_str().expect("path"), "--json"])
        .args(volume_args)
        .output()
        .expect("list multipart archive");
    let value = parse_stdout(&list);
    assert_eq!(value["archive"]["format"], "seven_zip");
    assert_eq!(value["entries"][0]["path"], "volume.txt");

    let inspect = cargo_bin_cmd!("arcthis")
        .args(["inspect", first.to_str().expect("path"), "--json"])
        .args(volume_args)
        .output()
        .expect("inspect multipart archive");
    let value = parse_stdout(&inspect);
    assert_eq!(value["multipart"], true);
    assert_eq!(value["volume_count"], 3);

    let read = cargo_bin_cmd!("arcthis")
        .args(["read", first.to_str().expect("path"), "volume.txt"])
        .args(volume_args)
        .output()
        .expect("read multipart archive");
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert_eq!(read.stdout, b"multipart archive\n");

    let verify = cargo_bin_cmd!("arcthis")
        .args(["verify", first.to_str().expect("path"), "--json"])
        .args(volume_args)
        .output()
        .expect("verify multipart archive");
    let value = parse_stdout(&verify);
    assert_eq!(value["verification"]["verified"], true);

    let incomplete = cargo_bin_cmd!("arcthis")
        .args([
            "verify",
            first.to_str().expect("path"),
            "--volume",
            second.to_str().expect("path"),
            "--json",
        ])
        .output()
        .expect("verify incomplete multipart archive");
    assert!(!incomplete.status.success());

    let restored = workspace.path().join("restored");
    let rejected_delete = cargo_bin_cmd!("arcthis")
        .args([
            "extract",
            first.to_str().expect("path"),
            "--output",
            restored.to_str().expect("path"),
            "--delete-source",
            "--json",
        ])
        .args(volume_args)
        .output()
        .expect("reject multipart source deletion");
    assert_eq!(rejected_delete.status.code(), Some(10));
    assert!(first.is_file() && second.is_file() && third.is_file());
}

#[test]
fn persistent_index_create_reuse_refresh_dry_run_and_delete() {
    let workspace = TempDir::new().expect("create test directory");
    let archive = workspace.path().join("indexed.zip");
    let cache = workspace.path().join("cache");
    create_zip(&archive, &[("one.txt", b"one")]);

    let run_index = |extra: &[&str]| {
        let mut command = cargo_bin_cmd!("arcthis");
        command.args([
            "index",
            archive.to_str().expect("path"),
            "--index-directory",
            cache.to_str().expect("path"),
            "--json",
        ]);
        command.args(extra).output().expect("run index command")
    };
    let created = parse_stdout(&run_index(&[]));
    assert_eq!(created["index"]["action"], "created");
    assert_eq!(created["index"]["entries_indexed"], 1);
    let index_path = Path::new(created["index"]["index_path"].as_str().expect("index path"));
    assert!(index_path.is_file());

    let reused = parse_stdout(&run_index(&[]));
    assert_eq!(reused["index"]["action"], "reused");

    std::fs::write(index_path, b"not valid JSON").expect("corrupt cached index");
    let fallback = cargo_bin_cmd!("arcthis")
        .args([
            "list",
            archive.to_str().expect("path"),
            "--index-directory",
            cache.to_str().expect("path"),
            "--json",
        ])
        .output()
        .expect("list with malformed index");
    let fallback = parse_stdout(&fallback);
    assert_eq!(fallback["entries"][0]["path"], "one.txt");

    create_zip(
        &archive,
        &[("one.txt", b"one"), ("two.txt", b"second entry")],
    );
    let refreshed = parse_stdout(&run_index(&[]));
    assert_eq!(refreshed["index"]["action"], "refreshed");
    assert_eq!(refreshed["index"]["entries_indexed"], 2);

    let dry_delete = parse_stdout(&run_index(&["--delete", "--dry-run"]));
    assert_eq!(dry_delete["index"]["action"], "would_delete");
    assert!(index_path.is_file());

    let deleted = parse_stdout(&run_index(&["--delete"]));
    assert_eq!(deleted["index"]["action"], "deleted");
    assert!(!index_path.exists());
}

#[test]
fn convert_preserves_paths_verifies_and_deletes_only_after_success() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("source.zip");
    let destination = workspace.path().join("converted.tar.zst");
    create_zip(&source, &[("root/data.txt", b"converted archive\n")]);

    let dry_run = cargo_bin_cmd!("arcthis")
        .args([
            "convert",
            source.to_str().expect("path"),
            "--output",
            destination.to_str().expect("path"),
            "--delete-source",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("plan conversion");
    let value = parse_stdout(&dry_run);
    assert_eq!(value["operation"], "convert");
    assert_eq!(value["plan"]["will_delete_source_after_success"], true);
    assert!(source.is_file());
    assert!(!destination.exists());

    let converted = cargo_bin_cmd!("arcthis")
        .args([
            "convert",
            source.to_str().expect("path"),
            "--output",
            destination.to_str().expect("path"),
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("convert archive");
    let value = parse_stdout(&converted);
    assert_eq!(value["convert"]["verification"]["verified"], true);
    assert_eq!(value["convert"]["source_deleted"], true);
    assert!(!source.exists());
    assert!(destination.is_file());

    let read = cargo_bin_cmd!("arcthis")
        .args(["read", destination.to_str().expect("path"), "root/data.txt"])
        .output()
        .expect("read converted entry");
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert_eq!(read.stdout, b"converted archive\n");
}

#[test]
fn failed_conversion_keeps_source_and_does_not_create_target() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("source.zip");
    let unsupported = workspace.path().join("target.rar");
    create_zip(&source, &[("data.txt", b"retain source")]);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "convert",
            source.to_str().expect("path"),
            "--output",
            unsupported.to_str().expect("path"),
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("run unsupported conversion");
    assert!(!output.status.success());
    assert!(source.is_file());
    assert!(!unsupported.exists());
}

#[test]
fn conversion_collision_refuses_or_renames_without_touching_existing_target() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("source.zip");
    let destination = workspace.path().join("target.tar.zst");
    create_zip(&source, &[("data.txt", b"conversion collision")]);
    std::fs::write(&destination, b"existing target").expect("write existing target");

    let refused = cargo_bin_cmd!("arcthis")
        .args([
            "convert",
            source.to_str().expect("path"),
            "--output",
            destination.to_str().expect("path"),
            "--json",
        ])
        .output()
        .expect("run refusing conversion");
    assert_eq!(refused.status.code(), Some(9));
    assert_eq!(
        std::fs::read(&destination).expect("read existing target"),
        b"existing target"
    );

    let renamed = cargo_bin_cmd!("arcthis")
        .args([
            "convert",
            source.to_str().expect("path"),
            "--output",
            destination.to_str().expect("path"),
            "--rename",
            "--json",
        ])
        .output()
        .expect("run renamed conversion");
    let value = parse_stdout(&renamed);
    let renamed_path = workspace.path().join("target.1.tar.zst");
    assert_eq!(
        value["convert"]["destination"],
        renamed_path.to_str().expect("path")
    );
    assert!(renamed_path.is_file());
    assert_eq!(
        std::fs::read(&destination).expect("read existing target"),
        b"existing target"
    );
}

#[test]
fn conversion_dry_run_enforces_extraction_path_safety() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("unsafe.zip");
    let destination = workspace.path().join("target.tar");
    create_zip(&source, &[("../escape.txt", b"unsafe")]);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "convert",
            source.to_str().expect("path"),
            "--output",
            destination.to_str().expect("path"),
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("plan unsafe conversion");
    assert_eq!(output.status.code(), Some(7));
    let error: Value = serde_json::from_slice(&output.stderr).expect("parse safety error");
    assert_eq!(error["error"]["code"], "unsafe_path");
    assert!(source.is_file());
    assert!(!destination.exists());
}

#[test]
fn pack_rejects_password_option_instead_of_ignoring_it() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("source.txt");
    let password = workspace.path().join("password.txt");
    let output_path = workspace.path().join("archive.zip");
    std::fs::write(&source, b"plain source").expect("write source");
    std::fs::write(&password, b"secret").expect("write password");

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "pack",
            source.to_str().expect("path"),
            "--output",
            output_path.to_str().expect("path"),
            "--password-file",
            password.to_str().expect("path"),
            "--json",
        ])
        .output()
        .expect("run rejected encrypted pack");
    assert_eq!(output.status.code(), Some(10));
    assert!(!output_path.exists());
}
