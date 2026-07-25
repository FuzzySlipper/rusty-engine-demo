use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

use crate::{admit_stored_project_with_document, decode_project_document, AdmittedStoredProject};

use super::{sync_directory, ProjectSaveMode, ProjectStore, ProjectStoreError};

const CURRENT_PROJECT: &str =
    include_str!("../../../../../content/projects/loading-bay.project.json");

#[test]
fn competing_writer_opened_after_final_check_cannot_overwrite_the_commit() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let store = ProjectStore::default();
    let original = admitted(CURRENT_PROJECT);
    store
        .save(&target, &original, ProjectSaveMode::CreateNew)
        .unwrap();
    let expected_hash = store.load_source(&target).unwrap().source_hash();
    let first = renamed(&original, "First writer");
    let competing = renamed(&original, "Competing writer");

    let (start_competitor, competitor_start) = mpsc::channel();
    let (target_opened, opened_target) = mpsc::channel();
    let competing_result = std::thread::scope(|scope| {
        let competitor_store = store.clone();
        let competitor_target = target.clone();
        let competitor = scope.spawn(move || {
            competitor_start.recv().unwrap();
            competitor_store.replace_if_unchanged_with(
                &competitor_target,
                &competing,
                expected_hash,
                || target_opened.send(()).unwrap(),
                || {},
                sync_directory,
            )
        });

        store
            .replace_if_unchanged_with(
                &target,
                &first,
                expected_hash,
                || {},
                || {
                    start_competitor.send(()).unwrap();
                    opened_target.recv().unwrap();
                },
                sync_directory,
            )
            .unwrap();
        competitor.join().unwrap()
    });

    assert!(matches!(
        competing_result,
        Err(ProjectStoreError::StaleSource { .. })
    ));
    assert_eq!(store.load(&target).unwrap().project.name, "First writer");
}

#[test]
fn post_commit_directory_sync_failure_still_reports_the_installed_candidate() {
    let directory = TestDirectory::new();
    let target = directory.path().join("project.json");
    let store = ProjectStore::default();
    let original = admitted(CURRENT_PROJECT);
    store
        .save(&target, &original, ProjectSaveMode::CreateNew)
        .unwrap();
    let expected_hash = store.load_source(&target).unwrap().source_hash();
    let candidate = renamed(&original, "Committed despite sync failure");

    let installed_hash = store
        .replace_if_unchanged_with(
            &target,
            &candidate,
            expected_hash,
            || {},
            || {},
            |path| {
                Err(ProjectStoreError::Io {
                    operation: "injected post-commit directory sync",
                    path: path.to_path_buf(),
                    source: io::Error::other("injected failure"),
                })
            },
        )
        .unwrap();

    let reread = store.load_source(&target).unwrap();
    assert_eq!(reread.source_hash(), installed_hash);
    assert_eq!(
        reread.decoded.project.name,
        "Committed despite sync failure"
    );
}

fn admitted(source: &str) -> AdmittedStoredProject {
    let decoded = decode_project_document(source).unwrap();
    admit_stored_project_with_document(decoded.project)
        .unwrap()
        .0
}

fn renamed(project: &AdmittedStoredProject, name: &str) -> AdmittedStoredProject {
    let mut candidate = project.document().clone();
    candidate.name = name.to_string();
    admit_stored_project_with_document(candidate).unwrap().0
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "rusty-engine-demo-project-store-unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
