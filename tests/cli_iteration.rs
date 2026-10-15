use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::Path;
use std::process::Output;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON result")
}
fn error(output: &Output, code: &str, exit: i32) {
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stderr).expect("JSON error");
    assert_eq!(value["schema_version"], "2");
    assert_eq!(value["error"]["code"], code);
}
fn zip(path: &Path, entries: &[(&str, &[u8])], encrypted: bool) {
    let mut writer = ZipWriter::new(File::create(path).expect("ZIP"));
    for (name, content) in entries {
        let mut options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        if encrypted {
            options = options.with_aes_encryption(zip::AesMode::Aes256, "fixture-secret");
        }
        writer.start_file(*name, options).expect("entry");
        writer.write_all(content).expect("content");
    }
    writer.finish().expect("finish ZIP");
}
fn rename_entry(path: &Path, index: usize, name: &[u8]) {
    let (local, central, length) = {
        let mut archive =
            ZipArchive::new(File::open(path).expect("open ZIP")).expect("ZIP metadata");
        let file = archive.by_index_raw(index).expect("raw entry");
        (
            file.header_start() + 30,
            file.central_header_start() + 46,
            file.name_raw().len(),
        )
    };
    assert_eq!(name.len(), length);
    let mut bytes = fs::read(path).expect("ZIP bytes");
    for position in [local, central] {
        let position = usize::try_from(position).expect("offset");
        bytes[position..position + length].copy_from_slice(name);
    }
    fs::write(path, bytes).expect("change fixture filename");
}
fn tar(path: &Path, entries: &[(&str, &[u8], bool)]) {
    let mut writer = tar::Builder::new(File::create(path).expect("TAR"));
    for (name, content, directory) in entries {
        let mut header = tar::Header::new_gnu();
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_mode(if *directory { 0o755 } else { 0o644 });
        header.set_entry_type(if *directory {
            tar::EntryType::Directory
        } else {
            tar::EntryType::Regular
        });
        header.set_size(u64::try_from(content.len()).expect("size"));
        header.set_cksum();
        writer.append(&header, *content).expect("TAR entry");
    }
    writer.finish().expect("finish TAR");
}
#[test]
fn dot_paths_work_across_extract_convert_and_batch_and_collisions_stay_closed() {
    let work = TempDir::new().expect("workspace");
    let archive = work.path().join("dot.tar");
    tar(
        &archive,
        &[
            ("./", b"", true),
            ("./docs/", b"", true),
            ("./docs/./note.txt", b"hello\n", false),
        ],
    );
    let output = work.path().join("out");
    let plan = json(
        &cargo_bin_cmd!("arcthis")
            .arg("extract")
            .arg(&archive)
            .arg("--output")
            .arg(&output)
            .args(["--dry-run", "--json"])
            .output()
            .expect("plan"),
    );
    let done = json(
        &cargo_bin_cmd!("arcthis")
            .arg("extract")
            .arg(&archive)
            .arg("--output")
            .arg(&output)
            .args(["--no-sync", "--json"])
            .output()
            .expect("extract"),
    );
    assert_eq!(
        plan["plan"]["entries_to_extract"],
        done["extraction"]["entries_extracted"]
    );
    assert_eq!(
        fs::read(output.join("docs/note.txt")).expect("content"),
        b"hello\n"
    );
    let converted = work.path().join("converted.zip");
    json(
        &cargo_bin_cmd!("arcthis")
            .arg("convert")
            .arg(&archive)
            .arg("--output")
            .arg(&converted)
            .arg("--json")
            .output()
            .expect("convert"),
    );
    let batch = work.path().join("batch");
    fs::create_dir(&batch).expect("batch");
    fs::copy(&archive, batch.join("dot.tar")).expect("copy");
    json(
        &cargo_bin_cmd!("arcthis")
            .arg("extract-all")
            .arg(&batch)
            .args(["--no-sync", "--json"])
            .output()
            .expect("batch extract"),
    );
    assert_eq!(
        fs::read(batch.join("docs/note.txt")).expect("batch content"),
        b"hello\n"
    );
    for (name, entries) in [
        (
            "collision",
            vec![
                ("./a.txt", b"a".as_slice(), false),
                ("a.txt", b"b".as_slice(), false),
            ],
        ),
        ("escape", vec![("./../escape", b"bad".as_slice(), false)]),
        ("drive", vec![("./C:/escape", b"bad".as_slice(), false)]),
    ] {
        let path = work.path().join(format!("{name}.tar"));
        tar(&path, &entries);
        let destination = work.path().join(name);
        let result = cargo_bin_cmd!("arcthis")
            .arg("extract")
            .arg(path)
            .arg("--output")
            .arg(&destination)
            .arg("--json")
            .output()
            .expect("reject unsafe");
        error(
            &result,
            if name == "collision" {
                "collision"
            } else {
                "unsafe_path"
            },
            if name == "collision" { 9 } else { 7 },
        );
        assert!(!destination.exists());
    }
}
#[test]
fn duplicate_zip_instances_are_visible_and_selectable_including_aes() {
    let work = TempDir::new().expect("workspace");
    for (name, encrypted, sfx) in [
        ("plain.zip", false, false),
        ("aes.zip", true, false),
        ("sfx.zip", false, true),
    ] {
        let archive = work.path().join(name);
        zip(
            &archive,
            &[("aa.txt", b"first\n"), ("bb.txt", b"second\n")],
            encrypted,
        );
        rename_entry(&archive, 1, b"aa.txt");
        if sfx {
            let mut bytes = b"#!/fixture\n".to_vec();
            bytes.extend(fs::read(&archive).expect("ZIP"));
            fs::write(&archive, bytes).expect("SFX");
            error(
                &cargo_bin_cmd!("arcthis")
                    .arg("list")
                    .arg(&archive)
                    .arg("--json")
                    .output()
                    .expect("unsupported SFX"),
                "unsupported_format",
                3,
            );
            continue;
        }
        let value = json(
            &cargo_bin_cmd!("arcthis")
                .arg("list")
                .arg(&archive)
                .arg("--json")
                .output()
                .expect("list duplicates"),
        );
        assert_eq!(value["entries"].as_array().expect("entries").len(), 2);
        assert_eq!(value["entries"][1]["archive_index"], 1);
        let result = cargo_bin_cmd!("arcthis")
            .arg("read")
            .arg(&archive)
            .arg("aa.txt")
            .arg("--json")
            .output()
            .expect("invalid JSON/raw flags");
        error(&result, "invalid_argument", 2);
        let destination = work.path().join(format!("{name}-out"));
        error(
            &cargo_bin_cmd!("arcthis")
                .arg("extract")
                .arg(&archive)
                .arg("--output")
                .arg(&destination)
                .arg("--json")
                .output()
                .expect("duplicate extraction"),
            "collision",
            9,
        );
        let password = work.path().join("password");
        fs::write(&password, b"fixture-secret\n").expect("password");
        for (index, expected) in [(0, b"first\n".as_slice()), (1, b"second\n".as_slice())] {
            let mut command = cargo_bin_cmd!("arcthis");
            command
                .arg("read")
                .arg(&archive)
                .args(["--entry-index", &index.to_string()]);
            if encrypted {
                command.arg("--password-file").arg(&password);
            }
            let output = command.output().expect("read duplicate instance");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected);
        }
        let mut command = cargo_bin_cmd!("arcthis");
        command.arg("verify").arg(&archive).arg("--json");
        if encrypted {
            command.arg("--password-file").arg(&password);
        }
        let verified = json(&command.output().expect("verify all duplicates"));
        assert_eq!(verified["verification"]["entries_checked"], 2);
        assert_eq!(verified["verification"]["bytes_checked"], 13);
    }
}
#[test]
fn raw_filename_identity_and_explicit_legacy_decoding_are_lossless() {
    let work = TempDir::new().expect("workspace");
    let archive = work.path().join("legacy.zip");
    zip(&archive, &[("ab", b"content")], false);
    rename_entry(&archive, 0, &[0xce, 0xc4]);
    let value = json(
        &cargo_bin_cmd!("arcthis")
            .arg("list")
            .arg(&archive)
            .arg("--json")
            .output()
            .expect("raw metadata"),
    );
    assert_eq!(value["entries"][0]["path_raw_hex"], "cec4");
    let output = work.path().join("decoded");
    json(
        &cargo_bin_cmd!("arcthis")
            .arg("extract")
            .arg(&archive)
            .arg("--encoding")
            .arg("gbk")
            .arg("--output")
            .arg(&output)
            .arg("--json")
            .output()
            .expect("decode and extract"),
    );
    assert_eq!(
        fs::read(output.join("文")).expect("decoded file"),
        b"content"
    );
    let read = cargo_bin_cmd!("arcthis")
        .arg("read")
        .arg(&archive)
        .args(["--encoding", "gbk"])
        .output()
        .expect("decoded read");
    assert_eq!(read.stdout, b"content");
    let collision = work.path().join("escaped.zip");
    zip(&collision, &[("x", b"raw"), ("%80", b"literal")], false);
    rename_entry(&collision, 0, &[0x80]);
    let value = json(
        &cargo_bin_cmd!("arcthis")
            .arg("list")
            .arg(&collision)
            .arg("--json")
            .output()
            .expect("escaped list"),
    );
    assert_eq!(value["entries"][0]["path"], value["entries"][1]["path"]);
    assert_ne!(
        value["entries"][0]["path_raw_hex"],
        value["entries"][1]["path_raw_hex"]
    );
    error(
        &cargo_bin_cmd!("arcthis")
            .arg("stat")
            .arg(&collision)
            .args(["%80", "--json"])
            .output()
            .expect("ambiguous path"),
        "collision",
        9,
    );
}
#[test]
fn solid_seven_zip_ratios_use_blocks_and_actual_output_limits_remain_enforced() {
    let work = TempDir::new().expect("workspace");
    for compressed in [false, true] {
        let archive = work
            .path()
            .join(if compressed { "bomb.7z" } else { "random.7z" });
        let mut writer = sevenz_rust2::ArchiveWriter::new(File::create(&archive).expect("7z"))
            .expect("7z writer");
        let mut seed = 1_234_567_u64;
        let data = (0..5)
            .map(|_| {
                (0..50000)
                    .map(|_| {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        if compressed { 0 } else { seed.to_le_bytes()[0] }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        writer
            .push_archive_entries(
                (0..5)
                    .map(|i| sevenz_rust2::ArchiveEntry::new_file(&format!("f{i}.bin")))
                    .collect(),
                data.into_iter()
                    .map(|bytes| sevenz_rust2::SourceReader::new(Cursor::new(bytes)))
                    .collect(),
            )
            .expect("solid block");
        writer.finish().expect("7z finish");
        let info = json(
            &cargo_bin_cmd!("arcthis")
                .arg("inspect")
                .arg(&archive)
                .arg("--json")
                .output()
                .expect("inspect solid"),
        );
        assert_eq!(
            info["warnings"]
                .as_array()
                .expect("warnings")
                .iter()
                .any(|warning| warning["code"] == "high_compression_ratio"),
            compressed
        );
        let list = json(
            &cargo_bin_cmd!("arcthis")
                .arg("list")
                .arg(&archive)
                .arg("--json")
                .output()
                .expect("list solid"),
        );
        assert!(
            list["entries"]
                .as_array()
                .expect("entries")
                .iter()
                .all(|entry| entry["compressed_size"].is_null())
        );
        let result = cargo_bin_cmd!("arcthis")
            .arg("extract")
            .arg(&archive)
            .arg("--output")
            .arg(work.path().join(if compressed { "bomb" } else { "random" }))
            .args(["--no-sync", "--max-compression-ratio", "100", "--json"])
            .output()
            .expect("ratio limit");
        if compressed {
            error(&result, "resource_limit", 8);
        } else {
            json(&result);
        }
        error(
            &cargo_bin_cmd!("arcthis")
                .arg("extract")
                .arg(&archive)
                .arg("--output")
                .arg(work.path().join("limited"))
                .args(["--max-total-size", "1", "--json"])
                .output()
                .expect("actual output limit"),
            "resource_limit",
            8,
        );
    }
}
#[test]
fn verified_read_withholds_corrupt_bytes_and_stdin_is_bounded() {
    let work = TempDir::new().expect("workspace");
    let archive = work.path().join("data.zip");
    zip(&archive, &[("data.txt", b"hello\n")], false);
    let bytes = fs::read(&archive).expect("stdin ZIP");
    let value = json(
        &cargo_bin_cmd!("arcthis")
            .args(["list", "-", "--json"])
            .write_stdin(bytes.clone())
            .output()
            .expect("stdin list"),
    );
    assert_eq!(value["archive"]["path"], "-");
    assert_eq!(value["entries"][0]["path"], "data.txt");
    error(
        &cargo_bin_cmd!("arcthis")
            .args(["list", "-", "--max-stdin-bytes", "1", "--json"])
            .write_stdin(bytes)
            .output()
            .expect("stdin limit"),
        "resource_limit",
        8,
    );
    let output = cargo_bin_cmd!("arcthis")
        .arg("read")
        .arg(&archive)
        .arg("--verify-first")
        .output()
        .expect("verified read");
    assert_eq!(output.stdout, b"hello\n");
    let offset = {
        let mut zip = ZipArchive::new(File::open(&archive).expect("ZIP")).expect("metadata");
        zip.by_index(0).expect("entry").data_start().expect("data")
    };
    let mut bytes = fs::read(&archive).expect("bytes");
    bytes[usize::try_from(offset).expect("offset")] ^= 0xff;
    fs::write(&archive, bytes).expect("corrupt");
    let output = cargo_bin_cmd!("arcthis")
        .arg("read")
        .arg(&archive)
        .arg("--verify-first")
        .output()
        .expect("corrupt verified read");
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let output = cargo_bin_cmd!("arcthis")
        .arg("extract")
        .arg(&archive)
        .arg("--output")
        .arg(work.path().join("out"))
        .arg("--json")
        .output()
        .expect("CRC extract");
    error(&output, "verification_failed", 11);
}
#[test]
fn batch_continuation_isolates_planning_failures_and_reports_partial_failure() {
    let work = TempDir::new().expect("workspace");
    let directory = work.path().join("batch");
    fs::create_dir(&directory).expect("batch");
    zip(&directory.join("good.zip"), &[("file.txt", b"ok")], false);
    fs::write(directory.join("broken.zip"), b"PK\x03\x04broken").expect("broken ZIP");
    tar(
        &directory.join("unsafe.tar"),
        &[("../escape", b"bad", false)],
    );
    let dry = cargo_bin_cmd!("arcthis")
        .arg("extract-all")
        .arg(&directory)
        .args(["--continue-on-error", "--dry-run", "--json"])
        .output()
        .expect("batch dry run");
    error(&dry, "partial_failure", 12);
    let plan: Value = serde_json::from_slice(&dry.stdout).expect("plan");
    assert_eq!(plan["plan"]["errors"].as_array().expect("errors").len(), 2);
    assert!(!directory.join("good").exists());
    let done = cargo_bin_cmd!("arcthis")
        .arg("extract-all")
        .arg(&directory)
        .args(["--continue-on-error", "--no-sync", "--json"])
        .output()
        .expect("batch continuation");
    error(&done, "partial_failure", 12);
    let value: Value = serde_json::from_slice(&done.stdout).expect("batch result");
    assert_eq!(value["result"]["discovered"], 3);
    assert_eq!(value["result"]["succeeded"], 1);
    assert_eq!(value["result"]["failed"], 2);
    assert_eq!(
        fs::read(directory.join("good/file.txt")).expect("good extraction"),
        b"ok"
    );
    assert!(directory.join("broken.zip").exists());
}
#[test]
fn gzip_header_names_permissions_and_cache_inventory_are_observable() {
    let work = TempDir::new().expect("workspace");
    let source = work.path().join("original.txt");
    fs::write(&source, b"hello\n").expect("source");
    let output = work.path().join("renamed.gz");
    json(
        &cargo_bin_cmd!("arcthis")
            .arg("pack")
            .arg(&source)
            .arg("--output")
            .arg(&output)
            .arg("--json")
            .output()
            .expect("pack gzip"),
    );
    let value = json(
        &cargo_bin_cmd!("arcthis")
            .arg("list")
            .arg(&output)
            .arg("--json")
            .output()
            .expect("FNAME"),
    );
    assert_eq!(value["entries"][0]["path"], "original.txt");
    assert!(
        value["entries"][0]["modified_time"]
            .as_str()
            .expect("mtime")
            .ends_with('Z')
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let probe = work.path().join("probe");
        fs::write(&probe, b"").expect("umask probe");
        assert_eq!(
            fs::metadata(&output)
                .expect("output mode")
                .permissions()
                .mode()
                & 0o777,
            fs::metadata(&probe)
                .expect("probe mode")
                .permissions()
                .mode()
                & 0o777
        );
    }
    let archive = work.path().join("data.zip");
    zip(&archive, &[("file.txt", b"cache")], false);
    let cache = work.path().join("cache");
    json(
        &cargo_bin_cmd!("arcthis")
            .arg("index")
            .arg(&archive)
            .arg("--index-directory")
            .arg(&cache)
            .arg("--json")
            .output()
            .expect("index create"),
    );
    let hit = json(
        &cargo_bin_cmd!("arcthis")
            .arg("list")
            .arg(&archive)
            .arg("--index-directory")
            .arg(&cache)
            .arg("--json")
            .output()
            .expect("cache consumer"),
    );
    assert_eq!(hit["archive"]["index_used"], true);
    let inventory = json(
        &cargo_bin_cmd!("arcthis")
            .args(["index", "--list", "--json"])
            .arg("--index-directory")
            .arg(&cache)
            .output()
            .expect("inventory"),
    );
    assert_eq!(inventory["items"].as_array().expect("items").len(), 1);
    let deletion = json(
        &cargo_bin_cmd!("arcthis")
            .arg("index")
            .arg(&archive)
            .arg("--delete")
            .arg("--index-directory")
            .arg(&cache)
            .arg("--json")
            .output()
            .expect("delete cache"),
    );
    assert_eq!(deletion["index"]["entries_indexed"], 1);
}

#[test]
fn cache_pruning_and_same_mtime_replacement_do_not_return_stale_names() {
    let work = TempDir::new().expect("workspace");
    let archive = work.path().join("data.zip");
    let cache = work.path().join("cache");
    zip(&archive, &[("first.txt", b"one")], false);
    let original = fs::metadata(&archive).expect("original metadata");
    let modified = original.modified().expect("original mtime");
    json(
        &cargo_bin_cmd!("arcthis")
            .arg("index")
            .arg(&archive)
            .arg("--index-directory")
            .arg(&cache)
            .arg("--json")
            .output()
            .expect("index"),
    );
    zip(&archive, &[("other.txt", b"two")], false);
    File::options()
        .write(true)
        .open(&archive)
        .expect("open replacement")
        .set_times(fs::FileTimes::new().set_modified(modified))
        .expect("restore old mtime");
    assert_eq!(
        fs::metadata(&archive).expect("replacement metadata").len(),
        original.len()
    );
    let listed = json(
        &cargo_bin_cmd!("arcthis")
            .arg("list")
            .arg(&archive)
            .arg("--index-directory")
            .arg(&cache)
            .arg("--json")
            .output()
            .expect("list replacement"),
    );
    #[cfg(unix)]
    {
        assert_eq!(listed["archive"]["index_used"], false);
        assert_eq!(listed["entries"][0]["path"], "other.txt");
    }
    let unrelated = cache.join("indexes/keep.txt");
    fs::write(&unrelated, b"do not remove").expect("unrelated cache file");
    let dry = json(
        &cargo_bin_cmd!("arcthis")
            .args([
                "index",
                "--prune",
                "--dry-run",
                "--max-age-seconds",
                "0",
                "--json",
            ])
            .arg("--index-directory")
            .arg(&cache)
            .output()
            .expect("prune dry run"),
    );
    assert_eq!(dry["items"][0]["deleted"], false);
    let done = json(
        &cargo_bin_cmd!("arcthis")
            .args(["index", "--prune", "--max-age-seconds", "0", "--json"])
            .arg("--index-directory")
            .arg(&cache)
            .output()
            .expect("prune"),
    );
    assert_eq!(done["items"][0]["deleted"], true);
    assert!(unrelated.exists());
    assert!(archive.exists());
}
#[test]
fn batch_continue_keeps_cross_plan_overlap_protection() {
    let work = TempDir::new().expect("workspace");
    zip(&work.path().join("a.zip"), &[("shared/a.txt", b"a")], false);
    zip(&work.path().join("b.zip"), &[("shared/b.txt", b"b")], false);
    zip(
        &work.path().join("good.zip"),
        &[("file.txt", b"good")],
        false,
    );
    let output = cargo_bin_cmd!("arcthis")
        .arg("extract-all")
        .arg(work.path())
        .args(["--continue-on-error", "--no-sync", "--json"])
        .output()
        .expect("batch conflicts");
    error(&output, "partial_failure", 12);
    let value: Value = serde_json::from_slice(&output.stdout).expect("batch JSON");
    assert_eq!(value["result"]["succeeded"], 1);
    assert_eq!(value["result"]["failed"], 2);
    assert!(!work.path().join("shared").exists());
    assert_eq!(
        fs::read(work.path().join("good/file.txt")).expect("independent result"),
        b"good"
    );
}
#[test]
fn pack_strip_root_and_human_metadata_match_the_help_contract() {
    let work = TempDir::new().expect("workspace");
    let source = work.path().join("source");
    fs::create_dir(&source).expect("source directory");
    fs::write(source.join("file.txt"), b"text").expect("file");
    let archive = work.path().join("out.zip");
    let output = cargo_bin_cmd!("arcthis")
        .arg("pack")
        .arg(&source)
        .arg("--strip-root")
        .arg("--output")
        .arg(&archive)
        .output()
        .expect("pack contents");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("1 entries"));
    let listed = json(
        &cargo_bin_cmd!("arcthis")
            .arg("list")
            .arg(&archive)
            .arg("--json")
            .output()
            .expect("list contents"),
    );
    assert_eq!(listed["entries"][0]["path"], "file.txt");
    assert!(listed["entries"][0]["modified_time"].is_null());
    assert!(listed["entries"][0]["modified_time_local"].is_string());
    let inspect = cargo_bin_cmd!("arcthis")
        .arg("inspect")
        .arg(&archive)
        .output()
        .expect("human inspect");
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("can verify: true"));
    let stat = cargo_bin_cmd!("arcthis")
        .arg("stat")
        .arg(&archive)
        .arg("--verbose")
        .output()
        .expect("verbose stat");
    assert!(String::from_utf8_lossy(&stat.stdout).contains("crc32:"));
    let result = cargo_bin_cmd!("arcthis")
        .arg("extract")
        .arg(&archive)
        .arg("--output")
        .arg(work.path().join("out"))
        .arg("--delete-source")
        .output()
        .expect("delete receipt");
    assert!(result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("source deleted"));
    assert!(!archive.exists());
}

#[test]
fn duplicate_zip_views_support_deflate_and_zip64_offsets_without_materialization() {
    use std::io::{Seek, SeekFrom};
    let work = TempDir::new().expect("workspace");
    for sparse in [false, true] {
        let archive = work
            .path()
            .join(if sparse { "sparse.zip" } else { "deflated.zip" });
        let mut file = File::create(&archive).expect("ZIP file");
        if sparse {
            file.write_all(b"PK\x03\x04").expect("format prefix");
            file.seek(SeekFrom::Start(u64::from(u32::MAX) + 64))
                .expect("sparse ZIP64 offset");
        }
        let mut writer = ZipWriter::new(file);
        for (name, content) in [("aa.txt", "first\n"), ("bb.txt", "second\n")] {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .expect("entry");
            writer
                .write_all(content.repeat(100).as_bytes())
                .expect("deflated content");
        }
        writer.finish().expect("finish ZIP");
        // Change both local and central names using seeks; do not read the sparse file into memory.
        let (local, central) = {
            let mut zip =
                ZipArchive::new(File::open(&archive).expect("ZIP input")).expect("ZIP metadata");
            let file = zip.by_index_raw(1).expect("entry");
            (file.header_start() + 30, file.central_header_start() + 46)
        };
        let mut file = File::options()
            .write(true)
            .open(&archive)
            .expect("modify fixture");
        for position in [local, central] {
            file.seek(SeekFrom::Start(position)).expect("name offset");
            file.write_all(b"aa.txt").expect("duplicate name");
        }
        drop(file);
        let listed = json(
            &cargo_bin_cmd!("arcthis")
                .arg("list")
                .arg(&archive)
                .arg("--json")
                .output()
                .expect("list ZIP64 duplicates"),
        );
        assert_eq!(listed["entries"].as_array().expect("entries").len(), 2);
        let output = cargo_bin_cmd!("arcthis")
            .arg("read")
            .arg(&archive)
            .args(["--entry-index", "1"])
            .output()
            .expect("read ZIP64 view");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"second\n".repeat(100));
    }
}

#[test]
fn seven_zip_empty_and_duplicate_instances_preserve_selection_and_scan_order() {
    let work = TempDir::new().expect("workspace");
    let archive = work.path().join("mixed.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::new(File::create(&archive).expect("7z file"))
        .expect("7z writer");
    writer
        .push_archive_entry::<Cursor<Vec<u8>>>(
            sevenz_rust2::ArchiveEntry::new_file("same.txt"),
            None,
        )
        .expect("empty instance");
    writer
        .push_archive_entry(
            sevenz_rust2::ArchiveEntry::new_file("same.txt"),
            Some(Cursor::new(b"payload\n".to_vec())),
        )
        .expect("data instance");
    writer.finish().expect("finish 7z");
    let output = cargo_bin_cmd!("arcthis")
        .arg("read")
        .arg(&archive)
        .args(["--entry-index", "1"])
        .output()
        .expect("select data instance");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"payload\n");
    let archive = work.path().join("order.7z");
    let mut writer = sevenz_rust2::ArchiveWriter::new(File::create(&archive).expect("7z file"))
        .expect("7z writer");
    writer
        .push_archive_entry::<Cursor<Vec<u8>>>(
            sevenz_rust2::ArchiveEntry::new_file("empty.txt"),
            None,
        )
        .expect("empty file");
    for name in ["first.txt", "later.txt"] {
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_file(name),
                Some(Cursor::new(b"hit\n".to_vec())),
            )
            .expect("data file");
    }
    writer.finish().expect("finish 7z");
    let result = json(
        &cargo_bin_cmd!("arcthis")
            .arg("grep")
            .arg(&archive)
            .args(["hit", "--max-matches", "1", "--json"])
            .output()
            .expect("ordered capped grep"),
    );
    assert_eq!(result["grep"]["files_scanned"], 2);
    assert_eq!(result["grep"]["matches"][0]["path"], "first.txt");
}
#[test]
fn lifecycle_plans_and_results_use_the_same_canonical_paths() {
    let work = TempDir::new().expect("workspace");
    zip(
        &work.path().join("source.zip"),
        &[("file.txt", b"data")],
        false,
    );
    let dry = json(
        &cargo_bin_cmd!("arcthis")
            .current_dir(work.path())
            .args([
                "extract",
                "source.zip",
                "--output",
                "out",
                "--dry-run",
                "--json",
            ])
            .output()
            .expect("relative dry-run"),
    );
    let result = json(
        &cargo_bin_cmd!("arcthis")
            .current_dir(work.path())
            .args([
                "extract",
                "source.zip",
                "--output",
                "out",
                "--no-sync",
                "--json",
            ])
            .output()
            .expect("relative extraction"),
    );
    assert_eq!(dry["plan"]["source"], result["archive"]["path"]);
    assert_eq!(
        dry["plan"]["destination"],
        result["extraction"]["destination"]
    );
}

#[test]
fn nested_entry_index_selects_the_final_archive_only() {
    let work = TempDir::new().expect("workspace");
    let inner = work.path().join("inner.zip");
    zip(
        &inner,
        &[("aa.txt", b"first"), ("bb.txt", b"second")],
        false,
    );
    rename_entry(&inner, 1, b"aa.txt");
    let outer = work.path().join("outer.zip");
    zip(
        &outer,
        &[("inner.zip", &fs::read(&inner).expect("inner bytes"))],
        false,
    );
    let output = cargo_bin_cmd!("arcthis")
        .arg("read")
        .arg(&outer)
        .args(["--within", "inner.zip", "--entry-index", "1"])
        .output()
        .expect("nested instance");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"second");
}

#[test]
fn decoded_materialization_uses_instance_identity_instead_of_escaped_display_paths() {
    let work = TempDir::new().expect("workspace");
    for extension in ["zip", "tar"] {
        let archive = work.path().join(format!("aliases.{extension}"));
        if extension == "zip" {
            zip(&archive, &[("x", b"raw"), ("%80", b"literal")], false);
            rename_entry(&archive, 0, &[0x80]);
        } else {
            let mut writer = tar::Builder::new(File::create(&archive).expect("TAR"));
            for (name, content) in [
                (&[0x80][..], b"raw".as_slice()),
                (b"%80".as_slice(), b"literal".as_slice()),
            ] {
                let mut header = tar::Header::new_gnu();
                header.as_mut_bytes()[..name.len()].copy_from_slice(name);
                header.set_mode(0o644);
                header.set_size(u64::try_from(content.len()).expect("size"));
                header.set_cksum();
                writer.append(&header, content).expect("raw-name TAR entry");
            }
            writer.finish().expect("finish TAR");
        }
        let destination = work.path().join(extension);
        json(
            &cargo_bin_cmd!("arcthis")
                .arg("extract")
                .arg(&archive)
                .args(["--encoding", "cp437", "--no-sync", "--json"])
                .arg("--output")
                .arg(&destination)
                .output()
                .expect("decoded aliases"),
        );
        assert_eq!(
            fs::read(destination.join("Ç")).expect("raw-name output"),
            b"raw"
        );
        assert_eq!(
            fs::read(destination.join("%80")).expect("literal-name output"),
            b"literal"
        );
    }
}
