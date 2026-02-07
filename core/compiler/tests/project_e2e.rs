use std::fs;
use std::path::Path;
use wrela::hir::project::load_project;

fn write_temp(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn project_imports_from_subdir() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let mod_path = base.path().join("src").join("net").join("http.wr");

    write_temp(
        &entry_path,
        "use ping from net/http\n\nto run() -> Integer:\n    return ping()\n",
    );
    write_temp(&mod_path, "to ping() -> Integer:\n    return 7\n");

    let project = load_project(&entry_path).expect("load project");
    let mut found = false;
    for (_, func) in project.module.functions.iter() {
        if func.name == "ping" {
            found = true;
            break;
        }
    }
    assert!(found);
}

#[test]
fn project_missing_module_has_span() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        "use foo from missing/module\n\nto run() -> Integer:\n    return 1\n",
    );

    let err = match load_project(&entry_path) {
        Ok(_) => panic!("expected error"),
        Err(err) => err,
    };
    let msg = err[0].message.clone();
    assert!(msg.contains("module 'missing/module' not found"));
    assert_ne!(err[0].span, miette::SourceSpan::from((0usize, 0usize)));
}

#[test]
fn project_removed_core_stdlib_module_has_migration_hint() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        "use AuthProvider from auth\n\nto run() -> Integer:\n    return 1\n",
    );

    let err = match load_project(&entry_path) {
        Ok(_) => panic!("expected error"),
        Err(err) => err,
    };
    let msg = err[0].message.clone();
    assert!(msg.contains("removed from core stdlib"));
    assert_ne!(err[0].span, miette::SourceSpan::from((0usize, 0usize)));
}

#[test]
fn project_missing_type_import_has_span() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        "to run() -> Integer:\n    return 1\n\nto f(x: Foo) -> Integer:\n    return 1\n",
    );

    let err = match load_project(&entry_path) {
        Ok(_) => panic!("expected error"),
        Err(err) => err,
    };
    let msg = err[0].message.clone();
    assert!(msg.contains("requires an explicit import"));
    assert_ne!(err[0].span, miette::SourceSpan::from((0usize, 0usize)));
}

#[test]
fn project_private_import_has_span() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let mod_path = base.path().join("src").join("bar.wr");

    write_temp(
        &entry_path,
        "use foo from bar\n\nto run() -> Integer:\n    return 1\n",
    );
    write_temp(
        &mod_path,
        "private:\n    to foo() -> Integer:\n        return 1\n",
    );

    let err = match load_project(&entry_path) {
        Ok(_) => panic!("expected error"),
        Err(err) => err,
    };
    let msg = err[0].message.clone();
    assert!(msg.contains("cannot import private"));
    assert_ne!(err[0].span, miette::SourceSpan::from((0usize, 0usize)));
}

#[test]
fn project_unused_import_has_span() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let mod_path = base.path().join("src").join("bar.wr");

    write_temp(
        &entry_path,
        "use foo from bar\n\nto run() -> Integer:\n    return 1\n",
    );
    write_temp(&mod_path, "to foo() -> Integer:\n    return 1\n");

    let project = load_project(&entry_path).expect("load project");
    let warn = project
        .warnings
        .iter()
        .find(|warn| warn.message.contains("unused import 'foo'"))
        .expect("missing unused import warning");
    assert_ne!(warn.span, miette::SourceSpan::from((0usize, 0usize)));
}

#[test]
fn project_unused_glob_import_has_span() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let mod_path = base.path().join("src").join("bar.wr");

    write_temp(
        &entry_path,
        "use * from bar\n\nto run() -> Integer:\n    return 1\n",
    );
    write_temp(&mod_path, "to foo() -> Integer:\n    return 1\n");

    let project = load_project(&entry_path).expect("load project");
    let warn = project
        .warnings
        .iter()
        .find(|warn| warn.message.contains("unused glob import from 'bar'"))
        .expect("missing unused glob import warning");
    assert_ne!(warn.span, miette::SourceSpan::from((0usize, 0usize)));
}
