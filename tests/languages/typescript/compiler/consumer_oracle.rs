#![cfg(feature = "lsp-tests")]

use crate::typescript_consumers::TscBatch;

fn project(source: &str) -> tempfile::TempDir {
    let directory = tempfile::Builder::new()
        .prefix("pledge tsc ")
        .tempdir()
        .unwrap();
    std::fs::write(directory.path().join("index.ts"), source).unwrap();
    std::fs::write(
        directory.path().join("package.json"),
        r#"{"type":"module"}"#,
    )
    .unwrap();
    std::fs::write(
        directory.path().join("tsconfig.json"),
        serde_json::json!({
            "compilerOptions": { "strict": true, "noEmit": true, "skipLibCheck": true,
                "target": "ES2022", "module": "NodeNext", "moduleResolution": "NodeNext" },
            "files": ["index.ts"],
        })
        .to_string(),
    )
    .unwrap();
    directory
}

#[test]
#[should_panic(expected = "queued TypeScript consumers were not checked")]
fn queued_consumers_cannot_be_silently_discarded() {
    let mut batch = TscBatch::default();
    batch.push(
        project("export {};"),
        &["--project", "tsconfig.json"],
        true,
        "unchecked consumer".into(),
    );
    drop(batch);
}

#[test]
fn batched_consumers_match_cli_diagnostics_and_isolate_options_and_roots() {
    let mut batch = TscBatch::default();
    let optional = "const api: {value?: number} = {value: undefined}; export {};";
    for (extra, succeeds, label) in [
        (
            vec!["--exactOptionalPropertyTypes"],
            false,
            "exact optional",
        ),
        (vec![], true, "ordinary optional after strict check"),
    ] {
        let mut args = vec!["--project", "tsconfig.json"];
        args.extend(extra);
        batch.push(project(optional), &args, succeeds, label.into());
    }
    for (extra, source, succeeds, label) in [
        (
            vec!["--locale", "ja"],
            "const api: string = 1;",
            false,
            "localized diagnostics",
        ),
        (
            vec![],
            "const api: string = 1;",
            false,
            "default locale after localized diagnostics",
        ),
        (
            vec!["--pretty", "true"],
            "const api: string = 1;",
            false,
            "explicit pretty output",
        ),
        (
            vec![],
            "declare global { var onlyHere: number; } export {}; onlyHere.toFixed();",
            true,
            "ambient declaration",
        ),
        (
            vec![],
            "onlyHere.toFixed();",
            false,
            "ambient declaration does not leak",
        ),
        (vec![], "export const api: = ;", false, "syntax diagnostic"),
        (
            vec!["--unknownConsumerOption"],
            "export {};",
            false,
            "option diagnostic",
        ),
        (
            vec![],
            "export const api: number = 1;",
            true,
            "valid consumer after failures",
        ),
    ] {
        let mut args = vec!["--project", "tsconfig.json"];
        args.extend(extra);
        batch.push(project(source), &args, succeeds, label.into());
    }
    for (dependency, succeeds) in [("number", true), ("string", false)] {
        let directory = project("import {api} from 'contract'; api.toFixed();");
        std::fs::write(
            directory.path().join("dependency.ts"),
            format!("export declare const api: {dependency};"),
        )
        .unwrap();
        std::fs::write(
            directory.path().join("tsconfig.json"),
            serde_json::json!({
                "compilerOptions": { "strict": true, "noEmit": true, "skipLibCheck": true,
                    "module": "NodeNext", "moduleResolution": "NodeNext",
                    "paths": { "contract": ["./dependency.ts"] } },
                "files": ["index.ts"],
            })
            .to_string(),
        )
        .unwrap();
        batch.push(
            directory,
            &["--project", "tsconfig.json"],
            succeeds,
            format!("isolated path alias to {dependency}"),
        );
    }
    batch.push(
        project("export {};"),
        &["--project", "missing.json"],
        false,
        "missing project".into(),
    );
    batch.assert_matches_cli();
}
