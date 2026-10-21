use ttrackii_cli::{api::ApiClient, auth::DeviceAuth};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path},
};

#[tokio::test]
async fn sends_bearer_token_and_decodes_timer_contract() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/timer"))
        .and(header("authorization", "Bearer test-session"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "running": true,
            "asOf": "2026-07-21T10:00:05.000Z",
            "elapsedMs": 5000,
            "entry": {
                "id": "entry-1",
                "taskName": "Contract test",
                "workspaceId": "personal",
                "workspaceName": "Personal",
                "projectId": null,
                "projectName": null,
                "tagId": null,
                "tagName": null,
                "startTime": 1_784_628_000_000_i64,
                "endTime": null,
                "durationMs": 5000,
                "isRunning": true,
                "isBillable": false
            }
        })))
        .mount(&server)
        .await;

    let api = ApiClient::new(&server.uri(), Some("test-session".into())).unwrap();
    let timer = api.timer().await.unwrap();

    assert!(timer.running);
    assert_eq!(timer.elapsed_ms, 5000);
    assert_eq!(timer.entry.unwrap().task_name, "Contract test");
}

#[tokio::test]
async fn requests_an_rfc_device_code_without_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/code"))
        .and(body_json(
            serde_json::json!({ "client_id": "ttrackii-cli" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "device-secret",
            "user_code": "ABCD-1234",
            "verification_uri": "https://app.ttrackii.com/device",
            "verification_uri_complete": "https://app.ttrackii.com/device?user_code=ABCD-1234",
            "expires_in": 600,
            "interval": 5
        })))
        .mount(&server)
        .await;

    let auth = DeviceAuth::new(&server.uri()).unwrap();
    let code = auth.request_code().await.unwrap();

    assert_eq!(code.user_code, "ABCD-1234");
    assert_eq!(code.expires_in, 600);
}

#[tokio::test]
async fn maps_unauthorized_responses_to_auth_required() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/session"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let api = ApiClient::new(&server.uri(), Some("expired".into())).unwrap();
    let error = api.session().await.unwrap_err();

    assert!(matches!(error, ttrackii_cli::error::CliError::AuthRequired));
}

#[tokio::test]
async fn revokes_cli_sessions_via_better_auth() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/auth/revoke-session"))
        .and(header("authorization", "Bearer cli-session"))
        .and(body_json(serde_json::json!({ "token": "cli-session" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": true
        })))
        .mount(&server)
        .await;

    let api = ApiClient::new(&server.uri(), Some("cli-session".into())).unwrap();
    api.revoke().await.unwrap();
}

#[tokio::test]
async fn skips_remote_revoke_for_api_keys() {
    let server = MockServer::start().await;
    // No mocks mounted — any HTTP call would fail.
    let api = ApiClient::new(
        &server.uri(),
        Some("tt_live_abcdefghijklmnopqrstuvwx".into()),
    )
    .unwrap();
    api.revoke().await.unwrap();
}
