use std::path::PathBuf;

use loading_bay_game::{
    admit_stored_project_with_document, ProjectSaveMode, ProjectStore,
    STORED_PROJECT_SCHEMA_VERSION,
};

fn main() {
    let arguments = arguments();
    let store = ProjectStore::default();
    let decoded = store.load(&arguments.input).unwrap_or_else(|error| {
        panic!(
            "could not load project {}: {error}",
            arguments.input.display()
        )
    });
    let source_schema_version = decoded.source_schema_version;
    let (project, _) = admit_stored_project_with_document(decoded.project)
        .unwrap_or_else(|error| panic!("project admission failed: {error}"));
    let mode = if arguments.replace {
        ProjectSaveMode::ReplaceExisting
    } else {
        ProjectSaveMode::CreateNew
    };
    store
        .save(&arguments.output, &project, mode)
        .unwrap_or_else(|error| {
            panic!(
                "could not store project {}: {error}",
                arguments.output.display()
            )
        });
    println!(
        "project-store sourceSchema={} currentSchema={} project={} output={}",
        source_schema_version,
        STORED_PROJECT_SCHEMA_VERSION,
        project.document().project_id,
        arguments.output.display()
    );
}

struct Arguments {
    input: PathBuf,
    output: PathBuf,
    replace: bool,
}

fn arguments() -> Arguments {
    let mut input = None;
    let mut output = None;
    let mut replace = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--input" => input = Some(PathBuf::from(value(&mut arguments, "--input"))),
            "--output" => output = Some(PathBuf::from(value(&mut arguments, "--output"))),
            "--replace" => replace = true,
            _ => panic!("unknown project-store argument {argument}"),
        }
    }
    Arguments {
        input: input.expect("--input is required"),
        output: output.expect("--output is required"),
        replace,
    }
}

fn value(arguments: &mut impl Iterator<Item = String>, name: &str) -> String {
    arguments
        .next()
        .unwrap_or_else(|| panic!("{name} needs a value"))
}
