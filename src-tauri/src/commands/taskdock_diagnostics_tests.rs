//! Isolated HTTP behavior tests; production OS ownership is tested separately below.
use super::{probe, probe_if_owned, Credentials};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
#[path = "taskdock_diagnostics_fixture.rs"]
mod fixture;
use fixture::*;

#[tokio::test]
async fn none_auth_reads_live_handshake_catalog_and_runtime_policy() {
    let mut fixture = Fixture::new(Auth::None, Mode::Healthy).await;
    let report = fixture.report().await;
    for stage in ["auth", "handshake", "catalog", "runtime_policy"] {
        assert_eq!(report[stage]["status"], "passed");
    }
    assert_eq!(report["runtime_policy"]["tool_profile"], "core");
    assert_eq!(report["catalog"]["count"], 1);
    assert_eq!(report["catalog"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(report["catalog"]["list_changed_supported"], false);
    assert_eq!(
        report["runtime_policy"]["direct_workspace"]["security"]["sandbox_enforced"],
        false
    );
    assert_eq!(report["client_cache"]["status"], "not_observable");
    assert_eq!(report["dot_route"]["status"], "not_observable");
    let calls = fixture.observations.calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0]["params"]["protocolVersion"], "2024-11-05");
    assert_eq!(calls[2]["params"]["name"], "server_info");
    assert_eq!(calls[2]["params"]["arguments"], json!({}));
    drop(calls);
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn bearer_success_and_rejection_do_not_expose_credentials_or_error_body() {
    let mut fixture = Fixture::new(Auth::Bearer, Mode::Healthy).await;
    let successful = fixture.report().await;
    assert_eq!(successful["auth"]["status"], "passed");
    assert_eq!(successful["runtime_policy"]["status"], "passed");
    let rejected = probe(
        &fixture.profile,
        Credentials::Bearer(Some("wrong-diagnostic-credential".into())),
    )
    .await;
    assert_eq!(rejected["auth"]["status"], "failed");
    assert_eq!(rejected["auth"]["http_status"], 401);
    assert_eq!(rejected["handshake"]["status"], "unavailable");
    assert_eq!(fixture.observations.calls.lock().unwrap().len(), 3);
    assert_no_secrets(&successful);
    assert_no_secrets(&rejected);
    fixture.stop().await;
}

#[tokio::test]
async fn noauth_listener_ignoring_saved_bearer_does_not_prove_authentication() {
    let mut fixture = Fixture::new(Auth::None, Mode::Healthy).await;
    fixture.profile.auth.auth_type = "bearer".into();
    let report = probe(&fixture.profile, Credentials::Bearer(Some(BEARER.into()))).await;
    assert_eq!(report["handshake"]["status"], "passed");
    assert_eq!(report["catalog"]["status"], "passed");
    assert_eq!(report["runtime_policy"]["status"], "passed");
    assert_eq!(report["runtime_policy"]["auth_enabled"], false);
    assert_eq!(report["runtime_policy"]["auth_type"], "noauth");
    assert_eq!(report["auth"]["mode"], "bearer");
    assert_eq!(report["auth"]["status"], "failed");
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn none_alias_with_actual_auth_enabled_true_remains_failed() {
    let mut fixture = Fixture::new(Auth::NoneAlias, Mode::Healthy).await;
    // Reuse the production AuthConfig method rather than inventing a false/none response.
    assert_eq!(fixture.profile.auth.auth_type, "none");
    assert!(fixture.profile.auth.auth_enabled());
    let report = fixture.report().await;
    assert_eq!(report["handshake"]["status"], "passed");
    assert_eq!(report["runtime_policy"]["status"], "passed");
    assert_eq!(report["runtime_policy"]["auth_enabled"], true);
    assert_eq!(report["runtime_policy"]["auth_type"], "none");
    assert_eq!(report["auth"]["mode"], "none");
    assert_eq!(report["auth"]["status"], "failed");
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn oauth_uses_real_authorization_code_pkce_exchange_without_callback_request() {
    let mut fixture = Fixture::new(Auth::OAuth, Mode::Healthy).await;
    let report = fixture.report().await;
    assert_eq!(report["auth"]["status"], "passed");
    assert_eq!(report["auth"]["mode"], "oauth");
    assert_eq!(report["runtime_policy"]["status"], "passed");
    assert_eq!(
        fixture
            .observations
            .authorize_requests
            .load(Ordering::SeqCst),
        1
    );
    assert_eq!(
        fixture.observations.token_requests.load(Ordering::SeqCst),
        1
    );
    assert_eq!(
        fixture
            .observations
            .callback_requests
            .load(Ordering::SeqCst),
        0
    );
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn wrong_oauth_password_stops_before_token_exchange() {
    let mut fixture = Fixture::new(Auth::OAuth, Mode::Healthy).await;
    let report = probe(
        &fixture.profile,
        Credentials::OAuth {
            client_id: "fixture-client".into(),
            password: Some("wrong-password".into()),
        },
    )
    .await;
    assert_eq!(report["auth"]["status"], "failed");
    assert_eq!(report["auth"]["http_status"], 401);
    assert_eq!(
        fixture.observations.token_requests.load(Ordering::SeqCst),
        0
    );
    assert!(fixture.observations.calls.lock().unwrap().is_empty());
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn oauth_rejects_unexpected_redirect_without_following_it() {
    let mut fixture = Fixture::new(Auth::OAuth, Mode::OAuthRedirect).await;
    let report = fixture.report().await;
    assert_eq!(report["auth"]["status"], "failed");
    assert_eq!(
        fixture.observations.token_requests.load(Ordering::SeqCst),
        0
    );
    assert_eq!(
        fixture
            .observations
            .callback_requests
            .load(Ordering::SeqCst),
        0
    );
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn mcp_redirect_is_not_followed() {
    let mut fixture = Fixture::new(Auth::Bearer, Mode::Redirect).await;
    let report = fixture.report().await;
    assert_eq!(report["handshake"]["status"], "failed");
    assert_eq!(
        fixture
            .observations
            .callback_requests
            .load(Ordering::SeqCst),
        0
    );
    assert_eq!(fixture.observations.calls.lock().unwrap().len(), 1);
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn invalid_and_oversized_responses_are_bounded_and_redacted() {
    for mode in [Mode::InvalidJson, Mode::Oversized] {
        let mut fixture = Fixture::new(Auth::None, mode).await;
        let report = fixture.report().await;
        assert_eq!(report["handshake"]["status"], "failed");
        assert_eq!(report["catalog"]["status"], "unavailable");
        assert_no_secrets(&report);
        fixture.stop().await;
    }
}

#[tokio::test]
async fn gateway_uses_catalog_requirement_and_checks_returned_workspace() {
    let mut fixture = Fixture::new(Auth::Bearer, Mode::Gateway).await;
    let report = fixture.report().await;
    assert_eq!(report["runtime_policy"]["status"], "passed");
    let calls = fixture.observations.calls.lock().unwrap();
    assert_eq!(
        calls[2]["params"]["arguments"]["workspace_id"],
        fixture.profile.id
    );
    drop(calls);
    fixture.stop().await;
}

#[tokio::test]
async fn mismatched_actual_workspace_is_failed_and_not_returned() {
    let mut fixture = Fixture::new(Auth::None, Mode::WrongTarget).await;
    let report = fixture.report().await;
    assert_eq!(report["handshake"]["status"], "passed");
    assert_eq!(report["catalog"]["status"], "passed");
    assert_eq!(report["runtime_policy"]["status"], "failed");
    assert!(report["runtime_policy"]["workspace"].is_null());
    fixture.stop().await;
}

#[tokio::test]
async fn missing_optional_server_info_remains_unavailable() {
    let mut fixture = Fixture::new(Auth::None, Mode::OmitServerInfo).await;
    let report = fixture.report().await;
    assert_eq!(report["handshake"]["status"], "passed");
    assert_eq!(report["catalog"]["status"], "passed");
    assert_eq!(report["runtime_policy"]["status"], "unavailable");
    assert_eq!(fixture.observations.calls.lock().unwrap().len(), 2);
    fixture.stop().await;
}

#[tokio::test]
async fn missing_credentials_sends_no_request() {
    let mut fixture = Fixture::new(Auth::Bearer, Mode::Healthy).await;
    let report = probe(&fixture.profile, Credentials::Bearer(None)).await;
    assert_eq!(report["auth"]["status"], "unavailable");
    assert!(fixture.observations.calls.lock().unwrap().is_empty());
    fixture.stop().await;
}

#[tokio::test]
async fn wrong_json_rpc_id_is_rejected() {
    let mut fixture = Fixture::new(Auth::None, Mode::WrongId).await;
    let report = fixture.report().await;
    assert_eq!(report["handshake"]["status"], "failed");
    assert_eq!(fixture.observations.calls.lock().unwrap().len(), 1);
    fixture.stop().await;
}

#[tokio::test]
async fn metadata_projection_drops_private_fields_and_echoed_bearer_value() {
    let mut fixture = Fixture::new(Auth::Bearer, Mode::Leaky).await;
    let report = fixture.report().await;
    assert_eq!(report["runtime_policy"]["status"], "passed");
    assert!(report["runtime_policy"]["tool_profile"].is_null());
    assert_no_secrets(&report);
    fixture.stop().await;
}

#[tokio::test]
async fn stopped_endpoint_reports_failure_without_starting_a_listener() {
    let mut fixture = Fixture::new(Auth::None, Mode::Healthy).await;
    fixture.stop().await;
    let report = fixture.report().await;
    assert_eq!(report["handshake"]["status"], "failed");
    assert_eq!(report["auth"]["status"], "unavailable");
    assert!(fixture.observations.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn untracked_foreign_socket_is_rejected_before_credentials_are_read_or_sent() {
    let mut fixture = Fixture::new(Auth::Bearer, Mode::Healthy).await;
    let credential_reads = AtomicUsize::new(0);
    let report = probe_if_owned(&fixture.profile, false, || {
        credential_reads.fetch_add(1, Ordering::SeqCst);
        Ok(fixture.credentials())
    })
    .await
    .unwrap();
    assert_eq!(report["auth"]["status"], "unavailable");
    assert_eq!(credential_reads.load(Ordering::SeqCst), 0);
    assert!(fixture.observations.calls.lock().unwrap().is_empty());
    assert_eq!(
        fixture
            .observations
            .authorize_requests
            .load(Ordering::SeqCst),
        0
    );
    assert_no_secrets(&report);
    fixture.stop().await;
}
