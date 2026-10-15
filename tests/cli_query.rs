use std::fs::File;
use std::io::Write;
use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::Value;
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn create_query_zip(path: &Path) {
    let file = File::create(path).expect("create ZIP fixture");
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    for (name, content) in [
        ("src/lib.rs", &b"fn main() {}\n// TODO: agent\n"[..]),
        ("docs/README.md", &b"Agent notes\nno marker\n"[..]),
        ("assets/data.bin", &b"prefix\0TODO\n"[..]),
    ] {
        archive.start_file(name, options).expect("start ZIP file");
        archive.write_all(content).expect("write ZIP file");
    }
    archive.finish().expect("finish ZIP fixture");
}

fn create_nested_zip(path: &Path) {
    let gzip = GzEncoder::new(Vec::new(), Compression::default());
    let mut tar = tar::Builder::new(gzip);
    let content = b"nested TODO\n";
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(content.len().try_into().expect("fixture size"));
    header.set_cksum();
    tar.append_data(&mut header, "inner/notes.txt", &content[..])
        .expect("append nested entry");
    let gzip = tar.into_inner().expect("finish nested TAR");
    let inner = gzip.finish().expect("finish nested Gzip");

    let file = File::create(path).expect("create outer ZIP");
    let mut outer = ZipWriter::new(file);
    outer
        .start_file("payload.tar.gz", SimpleFileOptions::default())
        .expect("start inner archive entry");
    outer.write_all(&inner).expect("write inner archive");
    outer.finish().expect("finish outer ZIP");
}

#[test]
fn find_returns_full_entry_metadata_for_glob_matches() {
    let workspace = TempDir::new().expect("create test directory");
    let archive = workspace.path().join("query.zip");
    create_query_zip(&archive);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "find",
            archive.to_str().expect("UTF-8 path"),
            "--glob",
            "**/*.rs",
            "--json",
        ])
        .output()
        .expect("run find");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse JSON");
    assert_eq!(value["schema_version"], "2");
    assert_eq!(value["find"]["matched"], 1);
    assert_eq!(value["find"]["entries"][0]["path"], "src/lib.rs");
    assert_eq!(value["find"]["entries"][0]["kind"], "file");
}

#[test]
fn grep_streams_text_and_reports_binary_and_size_limits() {
    let workspace = TempDir::new().expect("create test directory");
    let archive = workspace.path().join("query.zip");
    create_query_zip(&archive);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "grep",
            archive.to_str().expect("UTF-8 path"),
            "TODO",
            "--json",
        ])
        .output()
        .expect("run grep");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse JSON");
    assert_eq!(value["grep"]["matches"][0]["path"], "src/lib.rs");
    assert_eq!(value["grep"]["matches"][0]["line_number"], 2);
    assert_eq!(value["grep"]["binary_files_skipped"], 1);

    let limited = cargo_bin_cmd!("arcthis")
        .args([
            "grep",
            archive.to_str().expect("UTF-8 path"),
            "TODO",
            "--max-entry-size",
            "1",
            "--json",
        ])
        .output()
        .expect("run limited grep");
    let value: Value = serde_json::from_slice(&limited.stdout).expect("parse limited JSON");
    assert_eq!(value["grep"]["oversized_files_skipped"], 3);
    assert_eq!(value["grep"]["files_scanned"], 0);
}

#[test]
fn grep_filters_encrypted_entries_and_stops_before_later_content() {
    let workspace = TempDir::new().expect("test directory");
    let archive = workspace.path().join("mixed.zip");
    let password = workspace.path().join("password.txt");
    let wrong_password = workspace.path().join("wrong.txt");
    std::fs::write(&password, b"secret\n").expect("password");
    std::fs::write(&wrong_password, b"wrong\n").expect("wrong password");
    let mut writer = ZipWriter::new(File::create(&archive).expect("ZIP"));
    for (name, encrypted) in [("z.txt", false), ("secret.bin", true), ("a.txt", false)] {
        let options = if encrypted {
            SimpleFileOptions::default().with_aes_encryption(zip::AesMode::Aes256, "secret")
        } else {
            SimpleFileOptions::default()
        };
        writer.start_file(name, options).expect("entry");
        writer.write_all(b"hit\n").expect("content");
    }
    writer.finish().expect("finish ZIP");

    for (args, paths, truncated) in [
        (vec!["--glob", "*.txt"], vec!["z.txt", "a.txt"], false),
        (vec!["--max-matches", "1"], vec!["z.txt"], true),
        (vec!["--max-matches", "0"], vec![], true),
        (
            vec!["--password-file", password.to_str().expect("path")],
            vec!["z.txt", "secret.bin", "a.txt"],
            false,
        ),
    ] {
        let output = cargo_bin_cmd!("arcthis")
            .arg("grep")
            .arg(&archive)
            .arg("hit")
            .arg("--json")
            .args(args)
            .output()
            .expect("grep mixed ZIP");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
        let actual = value["grep"]["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|item| item["path"].as_str().expect("path"))
            .collect::<Vec<_>>();
        assert_eq!(actual, paths);
        assert_eq!(value["grep"]["files_scanned"], paths.len());
        assert_eq!(value["grep"]["bytes_scanned"], paths.len() * 4);
        assert_eq!(value["grep"]["matches_truncated"], truncated);
    }
    for (args, code) in [
        (vec![], "password_required"),
        (
            vec!["--password-file", wrong_password.to_str().expect("path")],
            "wrong_password",
        ),
    ] {
        let output = cargo_bin_cmd!("arcthis")
            .arg("grep")
            .arg(&archive)
            .arg("hit")
            .arg("--json")
            .args(args)
            .output()
            .expect("grep password error");
        assert_eq!(output.status.code(), Some(10));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).expect("JSON error");
        assert_eq!(error["error"]["code"], code);
    }
}

#[test]
fn grep_propagates_crc_errors_even_without_matches() {
    let workspace = TempDir::new().expect("test directory");
    let archive = workspace.path().join("bad-crc.zip");
    let mut writer = ZipWriter::new(File::create(&archive).expect("ZIP"));
    for name in ["a.txt", "b.txt"] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .expect("entry");
        writer.write_all(b"content\n").expect("content");
    }
    writer.finish().expect("finish ZIP");
    let data_start = {
        let mut zip =
            zip::ZipArchive::new(File::open(&archive).expect("open ZIP")).expect("metadata");
        zip.by_index(1)
            .expect("second entry")
            .data_start()
            .expect("data offset")
    };
    let mut bytes = std::fs::read(&archive).expect("ZIP bytes");
    bytes[usize::try_from(data_start).expect("offset")] ^= 0xff;
    std::fs::write(&archive, bytes).expect("corrupt content");
    let output = cargo_bin_cmd!("arcthis")
        .arg("grep")
        .arg(&archive)
        .arg("absent")
        .arg("--json")
        .output()
        .expect("grep CRC error");
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).expect("JSON error");
    assert_eq!(error["error"]["code"], "corrupted_archive");
}

#[test]
fn grep_still_rejects_ambiguous_tar_paths_when_scanned() {
    let workspace = TempDir::new().expect("test directory");
    let archive = workspace.path().join("duplicates.tar");
    let mut writer = tar::Builder::new(File::create(&archive).expect("TAR"));
    for content in [b"first\n".as_slice(), b"second\n".as_slice()] {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(content.len().try_into().expect("size"));
        header.set_cksum();
        writer
            .append_data(&mut header, "same.txt", content)
            .expect("entry");
    }
    writer.finish().expect("finish TAR");
    let output = cargo_bin_cmd!("arcthis")
        .arg("grep")
        .arg(&archive)
        .arg("first")
        .arg("--json")
        .output()
        .expect("grep duplicate path");
    assert_eq!(output.status.code(), Some(9));
    let error: Value = serde_json::from_slice(&output.stderr).expect("JSON error");
    assert_eq!(error["error"]["code"], "collision");
}

#[test]
fn hash_streams_sha256_and_sha512() {
    let workspace = TempDir::new().expect("create test directory");
    let archive = workspace.path().join("query.zip");
    create_query_zip(&archive);

    let output = cargo_bin_cmd!("arcthis")
        .args([
            "hash",
            archive.to_str().expect("UTF-8 path"),
            "src/lib.rs",
            "--json",
        ])
        .output()
        .expect("run hash");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse JSON");
    assert_eq!(value["hash"]["algorithm"], "sha256");
    assert_eq!(value["hash"]["bytes_hashed"], 28);
    assert_eq!(value["hash"]["digest"].as_str().expect("digest").len(), 64);

    let sha512 = cargo_bin_cmd!("arcthis")
        .args([
            "hash",
            archive.to_str().expect("UTF-8 path"),
            "src/lib.rs",
            "--algorithm",
            "sha512",
            "--json",
        ])
        .output()
        .expect("run sha512");
    let value: Value = serde_json::from_slice(&sha512.stdout).expect("parse JSON");
    assert_eq!(value["hash"]["digest"].as_str().expect("digest").len(), 128);
}

#[test]
fn grep_byte_budget_counts_escaped_lossy_text_across_files() {
    let workspace = TempDir::new().expect("grep budget directory");
    let archive = workspace.path().join("bounded.zip");
    let mut writer = ZipWriter::new(File::create(&archive).expect("create grep budget ZIP"));
    for name in ["a.txt", "b.txt"] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start grep entry");
        writer
            .write_all(b"hit \xff\"\\\n")
            .expect("write grep text");
    }
    writer.finish().expect("finish grep budget ZIP");
    let first = serde_json::json!({
        "path":"a.txt", "line_number":1, "text":"hit \u{fffd}\"\\", "line_truncated":false
    });
    let first_bytes = serde_json::to_vec(&first).expect("measure match").len() + 1;
    for (budget, count) in [(first_bytes, 1), (first_bytes - 1, 0), (0, 0)] {
        let output = cargo_bin_cmd!("arcthis")
            .args([
                "grep",
                archive.to_str().expect("archive path"),
                "hit",
                "--max-result-bytes",
                &budget.to_string(),
                "--json",
            ])
            .output()
            .expect("run byte-bounded grep");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).expect("grep result");
        assert_eq!(value["schema_version"], "2");
        assert_eq!(value["grep"]["matches_truncated"], true);
        assert_eq!(
            value["grep"]["matches"].as_array().expect("matches").len(),
            count
        );
        if count == 1 {
            assert_eq!(value["grep"]["matches"][0], first);
        }
    }
}

#[test]
fn within_traverses_inner_archive_in_memory() {
    let workspace = TempDir::new().expect("create test directory");
    let archive = workspace.path().join("outer.zip");
    create_nested_zip(&archive);

    let read = cargo_bin_cmd!("arcthis")
        .args([
            "read",
            archive.to_str().expect("UTF-8 path"),
            "inner/notes.txt",
            "--within",
            "payload.tar.gz",
        ])
        .output()
        .expect("read nested entry");
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert_eq!(read.stdout, b"nested TODO\n");

    let find = cargo_bin_cmd!("arcthis")
        .args([
            "find",
            archive.to_str().expect("UTF-8 path"),
            "--within",
            "payload.tar.gz",
            "--glob",
            "**/*.txt",
            "--json",
        ])
        .output()
        .expect("find nested entry");
    assert!(
        find.status.success(),
        "{}",
        String::from_utf8_lossy(&find.stderr)
    );
    let value: Value = serde_json::from_slice(&find.stdout).expect("parse nested JSON");
    assert_eq!(value["archive"]["format"], "tar_gzip");
    assert_eq!(value["find"]["entries"][0]["path"], "inner/notes.txt");

    let limited = cargo_bin_cmd!("arcthis")
        .args([
            "list",
            archive.to_str().expect("UTF-8 path"),
            "--within",
            "payload.tar.gz",
            "--max-nested-entry-size",
            "1",
            "--json",
        ])
        .output()
        .expect("run limited nested access");
    assert_eq!(limited.status.code(), Some(8));
    let value: Value = serde_json::from_slice(&limited.stderr).expect("parse limit error");
    assert_eq!(value["error"]["code"], "resource_limit");
}
