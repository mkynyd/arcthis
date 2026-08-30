use std::fs::File;
use std::io::Write;

use arcthis::{
    ApplicationService, ArchiveSourceRequest, CancellationToken, ErrorCode, HashAlgorithm,
    ReadRequest, ServiceLimits,
};
use tempfile::TempDir;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn fixture() -> (TempDir, ArchiveSourceRequest) {
    let temporary = TempDir::new().expect("create temp directory");
    let path = temporary.path().join("service.zip");
    let file = File::create(&path).expect("create ZIP");
    let mut archive = ZipWriter::new(file);
    archive
        .start_file("alpha.txt", SimpleFileOptions::default())
        .expect("start alpha");
    archive
        .write_all(b"alpha line\nsecond line\n")
        .expect("write alpha");
    archive
        .start_file("nested/beta.txt", SimpleFileOptions::default())
        .expect("start beta");
    archive.write_all(b"beta value\n").expect("write beta");
    archive.finish().expect("finish ZIP");
    (temporary, ArchiveSourceRequest::file(path))
}

#[test]
fn service_exposes_all_read_only_operations_without_cli() {
    let (_temporary, source) = fixture();
    let service = ApplicationService::default();

    assert_eq!(
        service
            .inspect(&source)
            .expect("inspect")
            .inspection
            .entry_count,
        2
    );
    assert_eq!(service.list(&source).expect("list").entries.len(), 2);
    let tree = service.tree(&source).expect("tree");
    assert_eq!(tree.entries.len(), 2);
    assert_eq!(tree.tree.len(), 2);
    assert_eq!(tree.tree[1].name, "nested");
    assert_eq!(tree.tree[1].children[0].path, "nested/beta.txt");
    assert_eq!(
        service.stat(&source, "alpha.txt").expect("stat").entry.size,
        23
    );
    assert_eq!(
        service
            .find(&source, "**/*.txt")
            .expect("find")
            .find
            .matched,
        2
    );

    let grep = service
        .grep(&source, "line", &arcthis::GrepOptions::default())
        .expect("grep");
    assert_eq!(grep.grep.matches.len(), 2);
    assert_eq!(
        service
            .hash(&source, "alpha.txt", HashAlgorithm::Sha256)
            .expect("hash")
            .hash
            .bytes_hashed,
        23
    );
    assert_eq!(
        service
            .verify(&source)
            .expect("verify")
            .verification
            .entries_checked,
        2
    );
}

#[test]
fn bounded_read_returns_exact_window_and_eof() {
    let (_temporary, source) = fixture();
    let service = ApplicationService::default();
    let first = service
        .read(&ReadRequest {
            source: &source,
            entry: "alpha.txt",
            offset: 6,
            length: 4,
        })
        .expect("read window");
    assert_eq!(first.data, b"line");
    assert!(!first.eof);

    let tail = service
        .read(&ReadRequest {
            source: &source,
            entry: "alpha.txt",
            offset: 18,
            length: 32,
        })
        .expect("read tail");
    assert_eq!(tail.data, b"line\n");
    assert!(tail.eof);
}

#[test]
fn limits_and_cancellation_fail_before_content_is_returned() {
    let (_temporary, source) = fixture();
    let token = CancellationToken::default();
    let service = ApplicationService::new(
        ServiceLimits {
            max_entries: 1,
            ..ServiceLimits::default()
        },
        token.clone(),
    );
    assert_eq!(
        service.list(&source).expect_err("entry limit").code(),
        ErrorCode::ResourceLimit
    );

    token.cancel();
    assert_eq!(
        service.inspect(&source).expect_err("cancelled").code(),
        ErrorCode::UnsupportedOperation
    );
}
