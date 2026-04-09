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
        r#"use ping from net/http

fn run() -> Integer {
    return ping()
}
"#,
    );
    write_temp(
        &mod_path,
        r#"fn ping() -> Integer {
    return 7
}
"#,
    );

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
fn project_provenance_tracks_owner_paths_for_merged_symbols() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let mod_path = base.path().join("src").join("domain").join("orders.wr");

    write_temp(
        &entry_path,
        r#"use run_app from domain/orders

fn run() -> Integer {
    return run_app()
}
"#,
    );
    write_temp(
        &mod_path,
        r#"fn run_app() -> Integer {
    return 1

}
class Order {
    id: Integer
}
enum State {
    Open

}
class Renderer {
    must draw() -> String
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let (func_idx, _) = project
        .module
        .functions
        .iter()
        .find(|(_, func)| func.name == "run_app")
        .expect("missing run_app");
    let owner = project
        .provenance
        .function_owner_path_by_id
        .get(&func_idx.into_raw())
        .expect("missing function owner path");
    assert_eq!(owner, &mod_path);
    let owner_by_name = project
        .provenance
        .function_owner_path_by_name
        .get("run_app")
        .expect("missing function owner path by name");
    assert_eq!(owner_by_name, &mod_path);

    let class_owner = project
        .provenance
        .class_owner_path_by_name
        .get("Order")
        .expect("missing class owner path");
    assert_eq!(class_owner, &mod_path);

    let enum_owner = project
        .provenance
        .enum_owner_path_by_name
        .get("State")
        .expect("missing enum owner path");
    assert_eq!(enum_owner, &mod_path);

    let interface_owner = project
        .provenance
        .interface_owner_path_by_name
        .get("Renderer")
        .expect("missing interface owner path");
    assert_eq!(interface_owner, &mod_path);
}

#[test]
fn project_allows_duplicate_private_function_names_across_modules() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let orders_path = base.path().join("src").join("domain").join("orders.wr");
    let payments_path = base.path().join("src").join("domain").join("payments.wr");

    write_temp(
        &entry_path,
        r#"use run_orders from domain/orders
use run_payments from domain/payments

fn run() -> Integer {
    return run_orders() + run_payments()
}
"#,
    );
    write_temp(
        &orders_path,
        r#"private {
    fn load_value() -> Integer {
        return 1

    }
}
fn run_orders() -> Integer {
    return load_value()
}
"#,
    );
    write_temp(
        &payments_path,
        r#"private {
    fn load_value() -> Integer {
        return 2

    }
}
fn run_payments() -> Integer {
    return load_value()
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let mut private_helpers = project
        .module
        .functions
        .iter()
        .filter_map(|(_, func)| {
            if func.name.starts_with("load_value_m_") {
                Some(func.name.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    private_helpers.sort();
    private_helpers.dedup();
    assert_eq!(
        private_helpers.len(),
        2,
        "expected private helper names to be module-scoped"
    );
}

#[test]
fn project_missing_module_has_span() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use foo from missing/module

fn run() -> Integer {
    return 1
}
"#,
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
        r#"use AuthProvider from auth

fn run() -> Integer {
    return 1
}
"#,
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
        r#"fn run() -> Integer {
    return 1

}
fn f(x: Foo) -> Integer {
    return 1
}
"#,
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
        r#"use foo from bar

fn run() -> Integer {
    return 1
}
"#,
    );
    write_temp(
        &mod_path,
        r#"private {
    fn foo() -> Integer {
        return 1
    }
}
"#,
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
        r#"use foo from bar

fn run() -> Integer {
    return 1
}
"#,
    );
    write_temp(
        &mod_path,
        r#"fn foo() -> Integer {
    return 1
}
"#,
    );

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
        r#"use * from bar

fn run() -> Integer {
    return 1
}
"#,
    );
    write_temp(
        &mod_path,
        r#"fn foo() -> Integer {
    return 1
}
"#,
    );

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
        r#"use run_service from application/service

fn run() -> Integer {
    return run_service()
}
"#,
    );
    write_temp(
        &app_path,
        r#"use fetch from infrastructure/db

fn run_service() -> Integer {
    return fetch()
}
"#,
    );
    write_temp(
        &infra_path,
        r#"fn fetch() -> Integer {
    return 7
}
"#,
    );

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
        r#"use fetch from infrastructure/db

fn run() -> Integer {
    return fetch()
}
"#,
    );
    write_temp(
        &infra_path,
        r#"fn fetch() -> Integer {
    return 7
}
"#,
    );

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
        r#"use run_domain from domain/service
use run_app from application/service

fn run() -> Integer {
    return run_domain() + run_app()
}
"#,
    );
    write_temp(
        &domain_path,
        r#"use get_environment_variable_or_default from host/env

fn run_domain() -> Integer {
    value = get_environment_variable_or_default("X", "1")
    return 1
}
"#,
    );
    write_temp(
        &app_path,
        r#"use get_environment_variable_or_default from host/env

fn run_app() -> Integer {
    value = get_environment_variable_or_default("Y", "2")
    return 2
}
"#,
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
        r#"use get_environment_variable_or_default from host/env
use read_env from infrastructure/service

fn run() -> Integer {
    value = get_environment_variable_or_default("Z", "3")
    return read_env() + 1
}
"#,
    );
    write_temp(
        &infra_path,
        r#"use get_environment_variable_or_default from host/env

fn read_env() -> Integer {
    value = get_environment_variable_or_default("I", "4")
    return 4
}
"#,
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
        r#"use run_service from domain/service

fn run() -> Integer {
    return run_service()
}
"#,
    );
    write_temp(
        &domain_path,
        r#"use fetch from infrastructure/db

fn run_service() -> Integer {
    return fetch()
}
"#,
    );
    write_temp(
        &infra_path,
        r#"fn fetch() -> Integer {
    return 7
}
"#,
    );

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
        r#"fn pure_add(x: Integer, y: Integer) -> Integer {
    return x + y

}
fn run() -> Integer {
    return pure_add(1, 2)
}
"#,
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
        r#"use get_environment_variable_or_default from host/env

fn read_env() -> String {
    return get_environment_variable_or_default("X", "fallback")

}
fn run() -> Integer {
    value = read_env()
    return 1
}
"#,
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
fn function_effect_classifies_host_fs_read_as_host_read() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use try_to_read_text from host/fs

fn read_file(path: String) -> String {
    return try_to_read_text(path) ?? ""

}
fn run() -> Integer {
    value = read_file("/tmp/nope")
    return 1
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let read_file = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "read_file")
        .expect("missing read_file effect");
    assert_eq!(read_file.effect, FunctionEffect::HostRead);
}

#[test]
fn function_effect_classifies_host_fs_write_as_host_write() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use try_to_write_text from host/fs

fn write_file(path: String) -> Integer {
    write_result = try_to_write_text(path, "ok")
    match write_result {
        Ok(_) {
            return 1
        }
        default {
            return 0
        }
    }
}
fn run() -> Integer {
    return write_file("/tmp/nope")
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let write_file = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "write_file")
        .expect("missing write_file effect");
    assert_eq!(write_file.effect, FunctionEffect::HostWrite);
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
        r#"use call_api from infrastructure/integrations/http_client

fn run() -> Integer {
    return call_api()
}
"#,
    );
    write_temp(
        &integration_path,
        r#"use try_to_http_call from host/http

fn call_api() -> Integer {
    headers = {}
    response = try_to_http_call("svc", "ep", "GET", "http://127.0.0.1:9/ping", headers, "", 250) ?? "fallback"
    if response == "fallback" {
        return 0
    }
    return 204
}
"#,
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
fn function_effect_classifies_host_time_sleep_as_host_write() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use sleep from host/time

fn delay() -> Integer {
    sleep(milliseconds=1)
    return 1

}
fn run() -> Integer {
    return delay()
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let delay = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "delay")
        .expect("missing delay effect");
    assert_eq!(delay.effect, FunctionEffect::HostWrite);
}

#[test]
fn function_effect_classifies_runtime_actor_pause_as_host_write() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use pause from runtime/actor

fn orchestrate() -> Integer {
    pause(handle=nothing)
    return 1

}
fn run() -> Integer {
    return orchestrate()
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let orchestrate = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "orchestrate")
        .expect("missing orchestrate effect");
    assert_eq!(orchestrate.effect, FunctionEffect::HostWrite);
}

#[test]
fn function_effect_classifies_runtime_actor_mailbox_len_as_host_read() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use get_mailbox_length from runtime/actor

fn sample() -> Integer {
    return get_mailbox_length(handle=nothing)

}
fn run() -> Integer {
    return sample()
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let sample = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "sample")
        .expect("missing sample effect");
    assert_eq!(sample.effect, FunctionEffect::HostRead);
}

#[test]
fn function_effect_classifies_runtime_pool_auto_size_as_host_read() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use auto_size from runtime/pool

fn capacity() -> Integer {
    return auto_size(objective=1, min=1, max=8, weight=1)

}
fn run() -> Integer {
    return capacity()
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let capacity = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "capacity")
        .expect("missing capacity effect");
    assert_eq!(capacity.effect, FunctionEffect::HostRead);
}

#[test]
fn function_effect_classifies_runtime_pool_normalize_queue_capacity_as_pure() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"use normalize_queue_capacity from runtime/pool

fn pure_capacity() -> Integer {
    return normalize_queue_capacity(requested_queue_capacity=0, default_queue_capacity=32, objective=0)

}
fn run() -> Integer {
    return pure_capacity()
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let pure_capacity = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "pure_capacity")
        .expect("missing pure_capacity effect");
    assert_eq!(pure_capacity.effect, FunctionEffect::Pure);
}

#[test]
fn function_effect_classifies_virtual_gpu_builtins() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");

    write_temp(
        &entry_path,
        r#"fn build_schedule() -> GpuDispatchSchedule {
    return gpu_schedule_reverse()
}

fn read_counter(counter: GpuAtomicI32) -> I32 {
    return gpu_atomic_i32_load(atomic=counter)
}

kernel fn run_kernel(counter: GpuAtomicI32, observed: GpuBuffer[I32]) -> Nothing {
    gid = global_invocation_id()
    previous = gpu_atomic_i32_fetch_add(atomic=counter, delta=i32(1))
    gpu_buffer_set(buffer=observed, index=gid[0], value=previous)
}

fn launch() -> Nothing {
    counter = gpu_atomic_i32_new(initial=i32(0))
    observed = gpu_buffer_new(length=4, default_value=i32(0))
    dispatch_compute(
        kernel=run_kernel,
        counter=counter,
        observed=observed,
        schedule=build_schedule(),
        workgroups_x=u32(2),
        workgroups_y=u32(1),
        workgroups_z=u32(1),
        workgroup_size_x=u32(2),
        workgroup_size_y=u32(1),
        workgroup_size_z=u32(1)
    )
}

fn run() -> Integer {
    launch()
    return 0
}
"#,
    );

    let project = load_project(&entry_path).expect("load project");
    let build_schedule = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "build_schedule")
        .expect("missing build_schedule effect");
    assert_eq!(build_schedule.effect, FunctionEffect::Pure);

    let read_counter = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "read_counter")
        .expect("missing read_counter effect");
    assert_eq!(read_counter.effect, FunctionEffect::HostRead);

    let kernel = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "run_kernel")
        .expect("missing kernel effect");
    assert_eq!(kernel.effect, FunctionEffect::HostWrite);

    let launch = project
        .function_effects
        .iter()
        .find(|entry| entry.module == "main" && entry.function == "launch")
        .expect("missing launch effect");
    assert_eq!(launch.effect, FunctionEffect::HostWrite);
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
        r#"use run_service from infrastructure/service

fn run() -> Integer {
    return run_service()
}
"#,
    );
    write_temp(
        &infra_path,
        r#"use try_to_http_call from host/http

fn run_service() -> Integer {
    headers = __wr_map_new()
    response = try_to_http_call("svc", "ep", "GET", "http://127.0.0.1:9/ping", headers, "", 250) ?? "fallback"
    if response == "fallback" {
        return 0
    }
    return 200
}
"#,
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
        r#"use run_service from application/service

fn run() -> Integer {
    return run_service()
}
"#,
    );
    write_temp(
        &app_path,
        r#"fn run_service() -> Integer {
    return try_to_http_call()
}
"#,
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
        r#"use run_app from application/service
use run_domain from domain/service

fn run() -> Integer {
    return run_app() + run_domain()
}
"#,
    );
    write_temp(
        &app_path,
        r#"fn run_app() -> Integer {
    headers = __wr_map_new()
    __wr_external_call("billing", "charge", "POST", "https://api.example/charge", headers, "", 500)
    return 1
}
"#,
    );
    write_temp(
        &domain_path,
        r#"fn run_domain() -> Integer {
    headers = __wr_map_new()
    __wr_external_call("catalog", "items", "GET", "https://api.example/items", headers, "", 300)
    return 2
}
"#,
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
        r#"use call_partner from infrastructure/integrations/external_client

fn run() -> Integer {
    return call_partner()
}
"#,
    );
    write_temp(
        &integration_path,
        r#"use try_to_call_external from host/external

fn call_partner() -> Integer {
    headers = {}
    try_to_call_external("partner", "sync", "POST", "https://api.partner/sync", headers, "", 1200) ?? ""
    return 204
}
"#,
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
        r#"use call_partner from infrastructure/integrations/external_client

fn run() -> Integer {
    return call_partner()
}
"#,
    );
    write_temp(
        &integration_path,
        r#"fn call_partner() -> Integer {
    headers = __wr_map_new()
    base = "https://api.partner"
    timeout = 1200
    url = base + "/sync"
    __wr_external_call("partner", "sync", "POST", url, headers, "", timeout)
    return 204
}
"#,
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
        r#"use run_service from domain/service

fn run() -> Integer {
    return run_service()
}
"#,
    );
    write_temp(
        &domain_path,
        r#"use try_to_http_call from host/http

fn run_service() -> Integer {
    headers = __wr_map_new()
    response = try_to_http_call("svc", "ep", "GET", "http://127.0.0.1:9/ping", headers, "", 250) ?? "fallback"
    if response == "fallback" {
        return 0
    }
    return 200
}
"#,
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected domain network diagnostic"),
        Err(err) => err,
    };
    let network_diag = errors
        .iter()
        .find(|err| err.message.contains("Network effect"))
        .expect("missing network effect diagnostic");
    assert!(network_diag.message.contains("domain code must stay pure"));
}

#[test]
fn domain_scene_queries_are_classified_as_host_reads() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let domain_path = base.path().join("src").join("domain").join("service.wr");

    write_temp(
        &entry_path,
        r#"use sample_scene from domain/service

fn run() -> Integer {
    return sample_scene()
}
"#,
    );
    write_temp(
        &domain_path,
        r#"field exact distance shell_field(p: Vec3) -> F32 {
    sphere(radius=0.5)
}

material shade_surface(hit: Hit3) -> Surface {
    return Surface()
}

radiance field scene_radiance(p: Vec3, direction: Vec3, feature_id: U32) -> Vec3 {
    return direction + vec3(f32(feature_id) * 0.0 + p.x * 0.0, 0.0, 0.0)
}

volume field scene_volume(p: Vec3, surface_distance: F32) -> Medium {
    return Medium(density=0.1, emission=vec3(0.0, 0.0, 0.0), anisotropy=0.0)
}

shape scene_shape {
    field = shell_field
    material = shade_surface
    radiance = scene_radiance
    volume = scene_volume
    payload = Payload()
}

fn sample_scene() -> Integer {
    scene = capture scene_shape
    glow = radiance_at(
        capture=scene,
        point=vec3(0.0, 0.0, 0.0),
        direction=vec3(0.0, 0.0, -1.0)
    )
    fog = medium_at(capture=scene, point=vec3(0.0, 0.0, 0.0))
    if glow.z > fog.density {
        return 1
    }
    return 0
}
"#,
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected domain host-read diagnostic"),
        Err(err) => err,
    };
    let host_read_diag = errors
        .iter()
        .find(|err| err.message.contains("HostRead effect"))
        .expect("missing host-read effect diagnostic");
    assert!(
        host_read_diag
            .message
            .contains("domain code must stay pure")
    );
}

#[test]
fn domain_async_orchestration_keywords_are_rejected() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let domain_path = base.path().join("src").join("domain").join("service.wr");

    write_temp(
        &entry_path,
        r#"use run_service from domain/service

fn run() -> Integer {
    return run_service()
}
"#,
    );
    write_temp(
        &domain_path,
        r#"class Worker {
    fn ping() -> Integer {
        return 1

    }
}
fn run_service() -> Integer {
    worker = detach Worker() * 1
    return 1
}
"#,
    );

    let errors = match load_project(&entry_path) {
        Ok(_) => panic!("expected domain async orchestration diagnostic"),
        Err(err) => err,
    };
    assert!(errors.iter().any(|err| {
        err.message.contains("uses 'detach'")
            && err.message.contains("domain deterministic and synchronous")
    }));
}

#[test]
fn domain_result_modeling_is_allowed() {
    let base = tempfile::tempdir().expect("tempdir");
    let entry_path = base.path().join("src").join("main.wr");
    let domain_path = base.path().join("src").join("domain").join("service.wr");

    write_temp(
        &entry_path,
        r#"use compute from domain/service

fn run() -> Integer {
    compute()
    return 1
}
"#,
    );
    write_temp(
        &domain_path,
        r#"fn compute() -> Result[Integer, Error] {
    return 7
}
"#,
    );

    match load_project(&entry_path) {
        Ok(_) => {}
        Err(err) => panic!("domain Result modeling should be allowed: {err:?}"),
    }
}
