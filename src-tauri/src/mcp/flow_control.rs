//! Dedicated control capacity does not grant permissions; normal dispatch and
//! authentication still run. Slow commands cannot impersonate a control method.
use serde_json::Value;

pub(crate) fn is_control(body:&Value)->bool {
    match body["method"].as_str() {
        Some("initialize"|"ping"|"tools/list"|"notifications/initialized")=>true,
        Some("tools/call")=>matches!(body["params"]["name"].as_str(),
            Some("server_info"|"task_status"|"task_checkpoint"|"kill_session"|"read_output")),
        _=>false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_known_control_methods_use_reserved_capacity() {
        for method in ["initialize","ping","tools/list"] {assert!(is_control(&json!({"method":method})));}
        for name in ["server_info","task_status","task_checkpoint","kill_session","read_output"] {
            assert!(is_control(&json!({"method":"tools/call","params":{"name":name}})));
        }
        for name in ["exec_command","apply_patch","write_stdin","search_text","upstream__server_info"] {
            assert!(!is_control(&json!({"method":"tools/call","params":{"name":name}})));
        }
        assert!(!is_control(&json!({"method":"unknown"})));
    }
}
