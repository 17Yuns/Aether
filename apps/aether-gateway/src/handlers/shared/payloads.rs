use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub(crate) struct InternalTunnelHeartbeatRequest {
    pub(crate) node_id: String,
    #[serde(default)]
    pub(crate) heartbeat_session_id: Option<String>,
    pub(crate) heartbeat_id: u64,
    #[serde(default)]
    pub(crate) heartbeat_interval: Option<i32>,
    #[serde(default)]
    pub(crate) active_connections: Option<i32>,
    #[serde(default)]
    pub(crate) total_requests: Option<i64>,
    #[serde(default)]
    pub(crate) window_total_requests: Option<i64>,
    #[serde(default)]
    pub(crate) avg_latency_ms: Option<f64>,
    #[serde(default)]
    pub(crate) failed_requests: Option<i64>,
    #[serde(default)]
    pub(crate) window_failed_requests: Option<i64>,
    #[serde(default)]
    pub(crate) dns_failures: Option<i64>,
    #[serde(default)]
    pub(crate) window_dns_failures: Option<i64>,
    #[serde(default)]
    pub(crate) stream_errors: Option<i64>,
    #[serde(default)]
    pub(crate) window_stream_errors: Option<i64>,
    #[serde(default)]
    pub(crate) proxy_metadata: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) proxy_version: Option<String>,
}

impl InternalTunnelHeartbeatRequest {
    pub(crate) fn is_valid(&self) -> bool {
        let node_id = self.node_id.trim();
        !node_id.is_empty()
            && node_id.len() <= 36
            && self.heartbeat_id > 0
            && !self.heartbeat_session_id.as_deref().is_some_and(|value| {
                let value = value.trim();
                value.is_empty() || value.len() > 128
            })
            && !self
                .heartbeat_interval
                .is_some_and(|value| !(5..=600).contains(&value))
            && !self.active_connections.is_some_and(|value| value < 0)
            && !self.total_requests.is_some_and(|value| value < 0)
            && !self.window_total_requests.is_some_and(|value| value < 0)
            && !self.avg_latency_ms.is_some_and(|value| value < 0.0)
            && !self.failed_requests.is_some_and(|value| value < 0)
            && !self.window_failed_requests.is_some_and(|value| value < 0)
            && !self.dns_failures.is_some_and(|value| value < 0)
            && !self.window_dns_failures.is_some_and(|value| value < 0)
            && !self.stream_errors.is_some_and(|value| value < 0)
            && !self.window_stream_errors.is_some_and(|value| value < 0)
            && !self
                .proxy_version
                .as_deref()
                .is_some_and(|value| value.chars().count() > 20)
            && !self
                .proxy_metadata
                .as_ref()
                .is_some_and(|value| !value.is_object())
    }
}

pub(crate) fn attach_tunnel_heartbeat_cursor(
    proxy_metadata: Option<serde_json::Value>,
    heartbeat_session_id: Option<&str>,
    heartbeat_id: u64,
) -> Option<serde_json::Value> {
    let Some(heartbeat_session_id) = heartbeat_session_id.map(str::trim) else {
        return proxy_metadata;
    };
    let mut metadata = proxy_metadata
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    metadata.insert(
        "heartbeat_session_id".to_string(),
        serde_json::Value::String(heartbeat_session_id.to_string()),
    );
    metadata.insert("heartbeat_id".to_string(), heartbeat_id.into());
    Some(serde_json::Value::Object(metadata))
}

#[derive(Debug, Deserialize)]
pub(crate) struct InternalTunnelNodeStatusRequest {
    pub(crate) node_id: String,
    pub(crate) connected: bool,
    #[serde(default)]
    pub(crate) conn_count: i32,
    #[serde(default)]
    pub(crate) observed_at_unix_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InternalGatewayResolveRequest {
    #[serde(default)]
    pub(crate) trace_id: Option<String>,
    pub(crate) method: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) query_string: Option<String>,
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InternalGatewayAuthContextRequest {
    #[serde(default)]
    pub(crate) trace_id: Option<String>,
    #[serde(default)]
    pub(crate) query_string: Option<String>,
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) auth_endpoint_signature: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InternalGatewayExecuteRequest {
    #[serde(default)]
    pub(crate) trace_id: Option<String>,
    pub(crate) method: String,
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) query_string: Option<String>,
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) body_json: serde_json::Value,
    #[serde(default)]
    pub(crate) body_base64: Option<String>,
    #[serde(default)]
    pub(crate) auth_context: Option<crate::control::GatewayControlAuthContext>,
}
