//! Bounded loopback HTTP transport, normal OAuth PKCE, and safe response projection.
use super::{ProbeFailure, BODY_LIMIT};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::{header::LOCATION, Client, Response, StatusCode, Url};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

async fn read_json(mut response: Response) -> Result<Value, ProbeFailure> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|size| size > BODY_LIMIT as u64)
    {
        return Err(ProbeFailure::http(status, "入口响应超过诊断大小限制"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ProbeFailure::http(status, "无法读取本地入口响应"))?
    {
        if bytes.len().saturating_add(chunk.len()) > BODY_LIMIT {
            return Err(ProbeFailure::http(status, "入口响应超过诊断大小限制"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| ProbeFailure::http(status, "入口响应不是有效的协议 JSON"))
}

pub(super) async fn rpc(
    client: &Client,
    endpoint: &str,
    token: Option<&str>,
    method: &str,
    params: Value,
) -> Result<(Value, u16), ProbeFailure> {
    let id = uuid::Uuid::new_v4().to_string();
    let mut request = client.post(endpoint).json(&json!({"jsonrpc":"2.0", "id":id,
        "method":method, "params":params}));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|_| ProbeFailure::plain("无法连接保存的本地入口，或请求已超时"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProbeFailure::http(status, "运行中的入口拒绝了协议请求"));
    }
    let body = read_json(response).await?;
    if body["jsonrpc"] != "2.0"
        || body["id"] != id
        || body.get("error").is_some()
        || !body["result"].is_object()
    {
        return Err(ProbeFailure::http(status, "入口返回的协议响应结构无效"));
    }
    Ok((body["result"].clone(), status.as_u16()))
}

pub(super) async fn oauth_token(
    client: &Client,
    port: u16,
    client_id: &str,
    password: &str,
) -> Result<String, ProbeFailure> {
    let base = format!("http://127.0.0.1:{port}");
    let callback = format!("{base}/__taskdock_diagnostic_callback");
    let state = uuid::Uuid::new_v4().to_string();
    let verifier = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let response = client
        .post(format!("{base}/oauth/authorize"))
        .form(&[
            ("client_id", client_id),
            ("redirect_uri", callback.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
            ("password", password),
        ])
        .send()
        .await
        .map_err(|_| ProbeFailure::plain("无法连接本地 OAuth 授权入口，或请求已超时"))?;
    let status = response.status();
    if !matches!(status, StatusCode::SEE_OTHER | StatusCode::FOUND) {
        return Err(ProbeFailure::http(status, "本地 OAuth 授权未返回有效回调"));
    }
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 8192)
        .and_then(|v| Url::parse(v).ok());
    let Some(location) = location else {
        return Err(ProbeFailure::http(status, "本地 OAuth 授权回调无效"));
    };
    let mut callback_base = location.clone();
    callback_base.set_query(None);
    if callback_base.as_str() != callback || location.fragment().is_some() {
        return Err(ProbeFailure::http(
            status,
            "本地 OAuth 授权回调与诊断请求不一致",
        ));
    }
    let pairs: Vec<_> = location.query_pairs().collect();
    if pairs.len() != 2
        || pairs.iter().filter(|(key, _)| key == "state").count() != 1
        || pairs
            .iter()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.as_ref())
            != Some(state.as_str())
    {
        return Err(ProbeFailure::http(status, "本地 OAuth 授权状态校验失败"));
    }
    let code = pairs
        .iter()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.as_ref())
        .filter(|code| !code.is_empty() && code.len() <= 512)
        .ok_or_else(|| ProbeFailure::http(status, "本地 OAuth 授权码无效"))?;
    let response = client
        .post(format!("{base}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", callback.as_str()),
            ("code_verifier", verifier.as_str()),
            ("client_id", client_id),
        ])
        .send()
        .await
        .map_err(|_| ProbeFailure::plain("无法连接本地 OAuth 换票入口，或请求已超时"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProbeFailure::http(status, "本地 OAuth 换票失败"));
    }
    let body = read_json(response).await?;
    let token = body["access_token"]
        .as_str()
        .filter(|token| !token.is_empty() && token.len() <= 16384);
    if body["token_type"]
        .as_str()
        .is_none_or(|kind| !kind.eq_ignore_ascii_case("bearer"))
        || token.is_none()
    {
        return Err(ProbeFailure::http(status, "本地 OAuth 未返回有效访问令牌"));
    }
    Ok(token.unwrap().to_string())
}

pub(super) fn tool_payload(result: &Value) -> Option<Value> {
    if result["isError"] == true {
        return None;
    }
    let payload = if result["structuredContent"].is_object() {
        result["structuredContent"].clone()
    } else {
        let items = result["content"].as_array()?;
        if items.len() != 1 || items[0]["type"] != "text" {
            return None;
        }
        serde_json::from_str(items[0]["text"].as_str()?).ok()?
    };
    (payload.is_object() && payload["ok"] != false).then_some(payload)
}

pub(super) fn safe_text(value: &Value, sensitive: &[String], limit: usize) -> Option<String> {
    value
        .as_str()
        .filter(|text| {
            !text.is_empty()
                && text.len() <= limit
                && !text.chars().any(char::is_control)
                && !sensitive
                    .iter()
                    .any(|secret| !secret.is_empty() && text.contains(secret))
        })
        .map(str::to_string)
}

pub(super) fn safe_capabilities(value: &Value) -> Option<Value> {
    if !value.is_object() {
        return None;
    }
    let mut result = json!({});
    if value["tools"].is_object() {
        result["tools"] = json!({"listChanged":value["tools"]["listChanged"].as_bool()});
    }
    if value["logging"].is_object() {
        result["logging"] = json!({});
    }
    if value["resources"].is_object() {
        result["resources"] = json!({
        "subscribe":value["resources"]["subscribe"].as_bool(), "listChanged":value["resources"]["listChanged"].as_bool()});
    }
    if value["prompts"].is_object() {
        result["prompts"] = json!({"listChanged":value["prompts"]["listChanged"].as_bool()});
    }
    Some(result)
}

pub(super) fn safe_direct_workspace(value: &Value, sensitive: &[String]) -> Option<Value> {
    if !value.is_object() {
        return None;
    }
    Some(json!({"version":value["version"].as_u64(),
        "execution_path":safe_text(&value["execution_path"], sensitive, 64),
        "external_agent_required":value["external_agent_required"].as_bool(),
        "durable_bookkeeping":safe_text(&value["durable_bookkeeping"], sensitive, 128),
        "catalog_refresh":safe_text(&value["catalog_refresh"], sensitive, 128),
        "security":{"execution_isolation":safe_text(&value["security"]["execution_isolation"], sensitive, 64),
            "sandbox_enforced":value["security"]["sandbox_enforced"].as_bool(),
            "read_scope":safe_text(&value["security"]["read_scope"], sensitive, 64)}}))
}
