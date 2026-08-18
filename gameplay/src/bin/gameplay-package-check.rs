//! Gameplay package check: compiles the committed canonical package artifact
//! through the semantic compiler and proves PARITY with the item definitions
//! the current project document admits. The project composition derives its
//! item definitions from the package artifact, so this gate permanently
//! guards that derivation (and any hand edit to either side): if the authored
//! package and the project document disagree about any item, it fails.

use std::path::PathBuf;
use std::process::exit;

use loading_bay_gameplay::compile::compile_gameplay_package;
use loading_bay_gameplay::project_admission::authored_item_definition;
use loading_bay_gameplay::project_codec::decode_project_document;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn main() {
    let package_path = repo_root().join("data/gameplay/loading-bay-e1m1-core.package.json");
    let project_path = repo_root().join("content/projects/doom-e1m1.project.json");

    let package_bytes = std::fs::read(&package_path).unwrap_or_else(|error| {
        eprintln!("cannot read {}: {error}", package_path.display());
        exit(1);
    });
    let compiled = match compile_gameplay_package(&package_bytes, "e1m1-core") {
        Ok(compiled) => compiled,
        Err(error) => {
            eprintln!("gameplay package rejected: {error}");
            exit(1);
        }
    };

    let project_source = std::fs::read_to_string(&project_path).unwrap_or_else(|error| {
        eprintln!("cannot read {}: {error}", project_path.display());
        exit(1);
    });
    let decoded = decode_project_document(&project_source).unwrap_or_else(|error| {
        eprintln!("project decode failed: {error:?}");
        exit(1);
    });
    let admitted = decoded
        .project
        .item_definitions
        .iter()
        .enumerate()
        .map(|(index, definition)| authored_item_definition(definition, index))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            eprintln!("project item admission failed: {error:?}");
            exit(1);
        });

    if compiled.items != admitted {
        eprintln!(
            "PARITY FAILURE: package and project item definitions differ (package {}, project {})",
            compiled.items.len(),
            admitted.len()
        );
        for (index, (package_item, project_item)) in
            compiled.items.iter().zip(admitted.iter()).enumerate()
        {
            if package_item != project_item {
                eprintln!(
                    "  first difference at item[{index}]:\n    package: {package_item:?}\n    project: {project_item:?}"
                );
                break;
            }
        }
        exit(1);
    }

    println!(
        "gameplay package ok: {} items, fingerprint {} — parity with project admission confirmed",
        compiled.items.len(),
        compiled.fingerprint
    );
}
