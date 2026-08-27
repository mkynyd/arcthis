use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::TempDir;

fn create_zip(workspace: &TempDir, source_name: &str, archive_name: &str) {
    let source = workspace.path().join(source_name);
    std::fs::create_dir_all(&source).expect("create source directory");
    std::fs::write(source.join("data.txt"), format!("{source_name} payload\n"))
        .expect("write source data");
    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args(["pack", source_name, "--output", archive_name, "--json"])
        .output()
        .expect("run pack");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn extract_dry_run_reports_plan_without_writing_or_deleting() {
    let workspace = TempDir::new().expect("create test directory");
    create_zip(&workspace, "project", "bundle.zip");
    std::fs::remove_dir_all(workspace.path().join("project")).expect("remove source fixture");

    let output = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "extract",
            "bundle.zip",
            "--dry-run",
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("run dry-run");

    assert!(output.status.success());
    assert!(workspace.path().join("bundle.zip").is_file());
    assert!(!workspace.path().join("project").exists());
    let value: Value = serde_json::from_slice(&output.stdout).expect("parse plan JSON");
    assert_eq!(value["operation"], "extract");
    assert_eq!(value["plan"]["will_delete_source_after_success"], true);
}

#[test]
fn extract_collision_policies_are_transactional_and_explicit() {
    let workspace = TempDir::new().expect("create test directory");
    create_zip(&workspace, "project", "bundle.zip");

    let destination = workspace.path().join("destination");
    std::fs::create_dir(&destination).expect("create collision directory");
    std::fs::write(destination.join("keep.txt"), b"keep").expect("write marker");
    let skipped = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "extract",
            "bundle.zip",
            "--output",
            "destination",
            "--skip-existing",
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("run skipped extraction");
    assert!(skipped.status.success());
    assert_eq!(
        std::fs::read(destination.join("keep.txt")).expect("read marker"),
        b"keep"
    );
    assert!(workspace.path().join("bundle.zip").is_file());

    let overwritten = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "extract",
            "bundle.zip",
            "--output",
            "destination",
            "--overwrite",
            "--json",
        ])
        .output()
        .expect("run overwrite extraction");
    assert!(overwritten.status.success());
    assert!(!destination.join("keep.txt").exists());
    assert!(destination.join("project/data.txt").is_file());

    std::fs::create_dir(workspace.path().join("renamed")).expect("create rename collision");
    let renamed = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "extract",
            "bundle.zip",
            "--output",
            "renamed",
            "--rename",
            "--json",
        ])
        .output()
        .expect("run renamed extraction");
    assert!(renamed.status.success());
    assert!(
        workspace
            .path()
            .join("renamed.1/project/data.txt")
            .is_file()
    );
}

#[test]
fn pack_delete_source_happens_only_after_verified_commit() {
    let workspace = TempDir::new().expect("create test directory");
    let source = workspace.path().join("project");
    std::fs::create_dir(&source).expect("create source directory");
    std::fs::write(source.join("data.txt"), b"payload").expect("write source");

    let failed = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "pack",
            "project",
            "--output",
            "unsupported.bin",
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("run failed pack");
    assert!(!failed.status.success());
    assert!(source.join("data.txt").is_file());

    let success = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "pack",
            "project",
            "--output",
            "project.7z",
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("run successful pack");
    assert!(success.status.success());
    assert!(!source.exists());
    assert!(workspace.path().join("project.7z").is_file());
}

#[test]
fn extract_all_honors_recursive_workers_and_delete_source() {
    let workspace = TempDir::new().expect("create test directory");
    create_zip(&workspace, "alpha", "alpha.zip");
    std::fs::remove_dir_all(workspace.path().join("alpha")).expect("remove alpha source");
    std::fs::create_dir(workspace.path().join("nested")).expect("create nested directory");
    create_zip(&workspace, "beta", "beta.zip");
    std::fs::remove_dir_all(workspace.path().join("beta")).expect("remove beta source");
    std::fs::rename(
        workspace.path().join("beta.zip"),
        workspace.path().join("nested/beta.zip"),
    )
    .expect("move nested archive");

    let plan = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "extract-all",
            ".",
            "--recursive",
            "--workers",
            "2",
            "--dry-run",
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("run batch plan");
    assert!(plan.status.success());
    let value: Value = serde_json::from_slice(&plan.stdout).expect("parse batch plan");
    assert_eq!(value["plan"]["archives"].as_array().map(Vec::len), Some(2));
    assert!(workspace.path().join("alpha.zip").is_file());

    let execute = cargo_bin_cmd!("arcthis")
        .current_dir(workspace.path())
        .args([
            "extract-all",
            ".",
            "--recursive",
            "--workers",
            "2",
            "--delete-source",
            "--json",
        ])
        .output()
        .expect("run batch extraction");
    assert!(
        execute.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&execute.stderr)
    );
    assert!(!workspace.path().join("alpha.zip").exists());
    assert!(!workspace.path().join("nested/beta.zip").exists());
    assert!(workspace.path().join("alpha/data.txt").is_file());
    assert!(workspace.path().join("nested/beta/data.txt").is_file());
}
