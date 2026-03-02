use std::process::Command;

fn write_temp(path: &std::path::Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(path, contents).expect("write file");
}

#[test]
fn web_package_entrypoints_typecheck_from_project_code() {
    let root = tempfile::tempdir().expect("tempdir");

    write_temp(
        &root.path().join("src").join("main.wr"),
        r#"use {
    HttpServer
}
from pkg/web/core/http_server

use {
    BearerJsonWebTokenAuthenticationMethodConfiguration,
    JsonWebKeySetRouteInstallationConfiguration,
    OAuthClientCredentialsAuthenticationInstallationConfiguration,
    RefreshTokenExchangeInstallationConfiguration
}
from pkg/web/auth/installers

fn run() -> Integer {
    server = HttpServer(bind_address="127.0.0.1:8080")

    server.install_bearer_json_web_token_authentication_method(
        configuration=BearerJsonWebTokenAuthenticationMethodConfiguration(
            authentication_method_identifier="bearer_jwt"
        )
    )

    server.install_oauth_client_credentials_authentication(
        configuration=OAuthClientCredentialsAuthenticationInstallationConfiguration(
            issuer="acme-auth",
            audience="acme-api",
            key_identifier="acme-key-2026"
        )
    )

    server.install_refresh_token_exchange(
        configuration=RefreshTokenExchangeInstallationConfiguration(
            issuer="acme-auth",
            audience="acme-api",
            key_identifier="acme-key-2026"
        )
    )

    server.install_json_web_key_set_route(
        configuration=JsonWebKeySetRouteInstallationConfiguration()
    )

    start_result = server.start()
    match start_result {
        Ok(_) {
            server.stop()
        }
        Err(_) {
            nothing

        }
    }
    return 1
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_wrela"))
        .arg("check")
        .arg(root.path())
        .output()
        .expect("run wrela check");

    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
