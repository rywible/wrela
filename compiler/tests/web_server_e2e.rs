use std::process::Command;

fn write_temp(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(path, contents).expect("write file");
}

#[test]
fn web_http_server_class_api_dispatch_contract_is_validated() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"fn run() -> Integer {
    return 0
}
"#,
    );

    write_temp(
        &root
            .path()
            .join("tests")
            .join("core")
            .join("web_http_server_class_api_test.wr"),
        r#"use HttpRequest, HttpResponse from host/web_server

use {
    BearerJsonWebTokenAuthenticationMethodConfiguration,
    JsonWebKeySetRouteInstallationConfiguration
}
from pkg/web/auth/installers

use {
    HttpServer,
    DefaultNotFoundRouteHandler
}
from pkg/web/core/http_server

fn create_request_context(method: String, path: String, headers: Map[Any, Any]) -> HttpRequest {
    return HttpRequest(method=method, path=path, headers=headers, body="")


}
fn create_response_context_from_dispatch_or_fallback(
    server: HttpServer,
    request_context: HttpRequest
) -> HttpResponse {
    fallback_response = HttpResponse(status_code=599)
    return server.try_to_dispatch_http_request(request=request_context) ?? fallback_response


}
fn test_http_server_defaults_and_explicit_installs_drive_dispatch() -> Nothing {
    server = HttpServer()

    not_installed_jwks_response_context = create_response_context_from_dispatch_or_fallback(
        server=server,
        request_context=create_request_context(
            method="GET",
            path="/.well-known/jwks.json",
            headers=__wr_map_new()
        )
    )
    assert value not_installed_jwks_response_context.status_code == 404

    server.install_json_web_key_set_route(
        configuration=JsonWebKeySetRouteInstallationConfiguration()
    )

    installed_jwks_response_context = create_response_context_from_dispatch_or_fallback(
        server=server,
        request_context=create_request_context(
            method="GET",
            path="/.well-known/jwks.json",
            headers=__wr_map_new()
        )
    )
    assert value installed_jwks_response_context.status_code == 200

    reserved_database_response_context = create_response_context_from_dispatch_or_fallback(
        server=server,
        request_context=create_request_context(
            method="GET",
            path="/db/internal",
            headers=__wr_map_new()
        )
    )
    assert value reserved_database_response_context.status_code == 403

    server.register_get_route(path="/api/health", route_handler=DefaultNotFoundRouteHandler())
    route_response_context = create_response_context_from_dispatch_or_fallback(
        server=server,
        request_context=create_request_context(
            method="GET",
            path="/api/health",
            headers=__wr_map_new()
        )
    )
    assert value route_response_context.status_code == 404

}
fn test_http_server_bearer_authentication_install_is_explicit() -> Nothing {
    server = HttpServer()
    assert value server.bearer_authentication_method_configurations.len() == 0

    server.install_bearer_json_web_token_authentication_method(
        configuration=BearerJsonWebTokenAuthenticationMethodConfiguration(
            authentication_method_identifier="bearer_jwt",
            enforce_revocation_check=false
        )
    )
    assert value server.bearer_authentication_method_configurations.len() == 1
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("test")
        .arg(root.path())
        .arg("--jobs=1")
        .output()
        .expect("run wrela test");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
