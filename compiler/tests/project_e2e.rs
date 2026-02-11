use std::fs;
use std::path::Path;
use wrela::hir::project::{FunctionEffect, load_project};

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

#[test]
fn app_importing_infra_fails_with_span_and_help() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let app_path = base
        .path()
        .join("src")
        .join("application")
        .join("service.wr");
    let infra_path = base.path().join("src").join("infrastructure").join("db.wr");

    write_temp(
        &entry_path,
        "use run_service from application/service\n\nto run() -> Integer:\n    return run_service()\n",
    );
    write_temp(
        &app_path,
        "use fetch from infrastructure/db\n\nto run_service() -> Integer:\n    return fetch()\n",
    );
    write_temp(&infra_path, "to fetch() -> Integer:\n    return 7\n");

    let err = match load_project(&entry_path) {
        Ok(_) => panic!("expected architecture error"),
        Err(err) => err,
    };
    let architecture = err
        .iter()
        .find(|err| {
            err.message
                .contains("application modules cannot import infrastructure modules")
        })
        .expect("missing architecture error");
    assert!(architecture.message.contains("help:"));
    assert_ne!(
        architecture.span,
        miette::SourceSpan::from((0usize, 0usize))
    );
}

#[test]
fn composition_importing_infra_passes() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base
        .path()
        .join("src")
        .join("application")
        .join("composition")
        .join("main.wr");
    let infra_path = base.path().join("src").join("infrastructure").join("db.wr");

    write_temp(
        &entry_path,
        "use fetch from infrastructure/db\n\nto run() -> Integer:\n    return fetch()\n",
    );
    write_temp(&infra_path, "to fetch() -> Integer:\n    return 7\n");

    let project = load_project(&entry_path);
    assert!(
        project.is_ok(),
        "composition root should be allowed to wire infrastructure"
    );
}

#[test]
fn host_import_from_domain_or_application_fails() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let domain_path = base.path().join("src").join("domain").join("service.wr");
    let app_path = base
        .path()
        .join("src")
        .join("application")
        .join("service.wr");

    write_temp(
        &entry_path,
        "use run_domain from domain/service\nuse run_app from application/service\n\nto run() -> Integer:\n    return run_domain() + run_app()\n",
    );
    write_temp(
        &domain_path,
        "use get_environment_variable_or_default from host/env\n\nto run_domain() -> Integer:\n    value = get_environment_variable_or_default(\"X\", \"1\")\n    return 1\n",
    );
    write_temp(
        &app_path,
        "use get_environment_variable_or_default from host/env\n\nto run_app() -> Integer:\n    value = get_environment_variable_or_default(\"Y\", \"2\")\n    return 2\n",
    );

    let err = match load_project(&entry_path) {
        Ok(_) => panic!("expected host import errors"),
        Err(err) => err,
    };
    assert!(err.iter().any(|err| {
        err.message
            .contains("domain modules cannot import host module 'host/env'")
    }));
    assert!(err.iter().any(|err| {
        err.message
            .contains("application modules cannot import host module 'host/env'")
    }));
}

#[test]
fn host_import_from_infra_or_composition_passes() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base
        .path()
        .join("src")
        .join("application")
        .join("composition")
        .join("main.wr");
    let infra_path = base
        .path()
        .join("src")
        .join("infrastructure")
        .join("service.wr");

    write_temp(
        &entry_path,
        "use get_environment_variable_or_default from host/env\nuse read_env from infrastructure/service\n\nto run() -> Integer:\n    value = get_environment_variable_or_default(\"Z\", \"3\")\n    return read_env() + 1\n",
    );
    write_temp(
        &infra_path,
        "use get_environment_variable_or_default from host/env\n\nto read_env() -> Integer:\n    value = get_environment_variable_or_default(\"I\", \"4\")\n    return 4\n",
    );

    match load_project(&entry_path) {
        Ok(_) => {}
        Err(err) => {
            panic!("infrastructure and composition should be allowed to import host/*: {err:?}")
        }
    }
}

#[test]
fn single_file_mode_bypasses_architecture_layer_rules() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("spec.wr");
    let domain_path = base.path().join("domain").join("service.wr");
    let infra_path = base.path().join("infrastructure").join("db.wr");

    write_temp(
        &entry_path,
        "use run_service from domain/service\n\nto run() -> Integer:\n    return run_service()\n",
    );
    write_temp(
        &domain_path,
        "use fetch from infrastructure/db\n\nto run_service() -> Integer:\n    return fetch()\n",
    );
    write_temp(&infra_path, "to fetch() -> Integer:\n    return 7\n");

    let project = load_project(&entry_path);
    assert!(
        project.is_ok(),
        "single-file mode should bypass architecture rules"
    );
}

#[test]
fn function_effect_classifies_pure() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        "to pure_add(x: Integer, y: Integer) -> Integer:\n    return x + y\n\nto run() -> Integer:\n    return pure_add(1, 2)\n",
    );

    let project = load_project(&entry_path).expect("load project");
    let pure = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "pure_add")
        .expect("missing pure_add effect");
    assert_eq!(pure.effect, FunctionEffect::Pure);
}

#[test]
fn function_effect_classifies_host_env_as_host_read() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        "use get_environment_variable_or_default from host/env\n\nto read_env() -> String:\n    return get_environment_variable_or_default(\"X\", \"fallback\")\n\nto run() -> Integer:\n    value = read_env()\n    return 1\n",
    );

    let project = load_project(&entry_path).expect("load project");
    let read_env = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "read_env")
        .expect("missing read_env effect");
    assert_eq!(read_env.effect, FunctionEffect::HostRead);
}

#[test]
fn function_effect_classifies_host_http_as_network_in_integrations() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let integration_path = base
        .path()
        .join("src")
        .join("infrastructure")
        .join("integrations")
        .join("http_client.wr");

    write_temp(
        &entry_path,
        "use call_api from infrastructure/integrations/http_client\n\nto run() -> Integer:\n    return call_api()\n",
    );
    write_temp(
        &integration_path,
        "use try_to_http_call from host/http\n\nto call_api() -> Integer:\n    headers = __wr_map_new()\n    response = try_to_http_call(\"svc\", \"ep\", \"GET\", \"http://127.0.0.1:9/ping\", headers, \"\", 250) otherwise \"fallback\"\n    if response == \"fallback\":\n        return 0\n    return 204\n",
    );

    let project = load_project(&entry_path).expect("load project");
    let call_api = project
        .function_effects
        .iter()
        .find(|entry| {
            entry.module == "infrastructure/integrations/http_client"
                && entry.function == "call_api"
        })
        .expect("missing call_api effect");
    assert_eq!(call_api.effect, FunctionEffect::Network);
}

#[test]
fn host_http_import_outside_integrations_fails() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let infra_path = base
        .path()
        .join("src")
        .join("infrastructure")
        .join("service.wr");

    write_temp(
        &entry_path,
        "use run_service from infrastructure/service\n\nto run() -> Integer:\n    return run_service()\n",
    );
    write_temp(
        &infra_path,
        "use try_to_http_call from host/http\n\nto run_service() -> Integer:\n    headers = __wr_map_new()\n    response = try_to_http_call(\"svc\", \"ep\", \"GET\", \"http://127.0.0.1:9/ping\", headers, \"\", 250) otherwise \"fallback\"\n    if response == \"fallback\":\n        return 0\n    return 200\n",
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected host/http integration boundary error"),
        Err(err) => err,
    };
    assert!(errors.iter().any(|err| {
        err.message
            .contains("cannot import host/http outside infrastructure/integrations")
    }));
}

#[test]
fn try_to_http_call_outside_integrations_fails() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let app_path = base
        .path()
        .join("src")
        .join("application")
        .join("service.wr");

    write_temp(
        &entry_path,
        "use run_service from application/service\n\nto run() -> Integer:\n    return run_service()\n",
    );
    write_temp(
        &app_path,
        "to run_service() -> Integer:\n    return try_to_http_call()\n",
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected network boundary error"),
        Err(err) => err,
    };
    assert!(errors.iter().any(|err| {
        err.message
            .contains("uses external network I/O outside infrastructure/integrations")
    }));
}

#[test]
fn external_connector_call_from_application_or_domain_fails() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let app_path = base
        .path()
        .join("src")
        .join("application")
        .join("service.wr");
    let domain_path = base.path().join("src").join("domain").join("service.wr");

    write_temp(
        &entry_path,
        "use run_app from application/service\nuse run_domain from domain/service\n\nto run() -> Integer:\n    return run_app() + run_domain()\n",
    );
    write_temp(
        &app_path,
        "to run_app() -> Integer:\n    headers = __wr_map_new()\n    __wr_external_call(\"billing\", \"charge\", \"POST\", \"https://api.example/charge\", headers, \"\", 500)\n    return 1\n",
    );
    write_temp(
        &domain_path,
        "to run_domain() -> Integer:\n    headers = __wr_map_new()\n    __wr_external_call(\"catalog\", \"items\", \"GET\", \"https://api.example/items\", headers, \"\", 300)\n    return 2\n",
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected external connector quarantine errors"),
        Err(err) => err,
    };
    assert!(errors.iter().any(|err| {
        err.message
            .contains("external connector call 'application/service::run_app' is outside")
    }));
    assert!(errors.iter().any(|err| {
        err.message
            .contains("external connector call 'domain/service::run_domain' is outside")
    }));
}

#[test]
fn external_connector_call_from_integrations_succeeds() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let integration_path = base
        .path()
        .join("src")
        .join("infrastructure")
        .join("integrations")
        .join("external_client.wr");

    write_temp(
        &entry_path,
        "use call_partner from infrastructure/integrations/external_client\n\nto run() -> Integer:\n    return call_partner()\n",
    );
    write_temp(
        &integration_path,
        "to call_partner() -> Integer:\n    headers = __wr_map_new()\n    __wr_external_call(\"partner\", \"sync\", \"POST\", \"https://api.partner/sync\", headers, \"\", 1200)\n    return 204\n",
    );

    match load_project(&entry_path) {
        Ok(_) => {}
        Err(err) => panic!("integrations module should allow external calls: {err:?}"),
    }
}

#[test]
fn external_connector_call_requires_literal_url_and_timeout() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let integration_path = base
        .path()
        .join("src")
        .join("infrastructure")
        .join("integrations")
        .join("external_client.wr");

    write_temp(
        &entry_path,
        "use call_partner from infrastructure/integrations/external_client\n\nto run() -> Integer:\n    return call_partner()\n",
    );
    write_temp(
        &integration_path,
        "to call_partner() -> Integer:\n    headers = __wr_map_new()\n    base = \"https://api.partner\"\n    timeout = 1200\n    url = base + \"/sync\"\n    __wr_external_call(\"partner\", \"sync\", \"POST\", url, headers, \"\", timeout)\n    return 204\n",
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected literal metadata validation errors"),
        Err(err) => err,
    };
    assert!(errors.iter().any(|err| {
        err.message
            .contains("external call metadata field 'url' must be a string literal")
    }));
    assert!(errors.iter().any(|err| {
        err.message
            .contains("external call metadata field 'timeout_ms' must be an integer literal")
    }));
}

#[test]
fn domain_network_effect_reports_teacher_fix_recipe() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let domain_path = base.path().join("src").join("domain").join("service.wr");

    write_temp(
        &entry_path,
        "use run_service from domain/service\n\nto run() -> Integer:\n    return run_service()\n",
    );
    write_temp(
        &domain_path,
        "use try_to_http_call from host/http\n\nto run_service() -> Integer:\n    headers = __wr_map_new()\n    response = try_to_http_call(\"svc\", \"ep\", \"GET\", \"http://127.0.0.1:9/ping\", headers, \"\", 250) otherwise \"fallback\"\n    if response == \"fallback\":\n        return 0\n    return 200\n",
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected domain network diagnostic"),
        Err(err) => err,
    };
    let network_diag = errors
        .iter()
        .find(|err| err.message.contains("Network effect"))
        .expect("missing network effect diagnostic");
    assert!(network_diag.message.contains("teacher fix recipe:"));
}
