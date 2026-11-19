//! The doors other than Telegram (`specs/tasks/engine-api.md`): an MCP server first. Each door
//! is served by the public HTTP server (`observability::server::start_with_doors`) behind the
//! [`guard`]: a valid personal token, and limits that keep the bot's own work safe.

pub mod guard;
pub mod mcp;
pub mod oauth;
pub mod photos;

/// The public host of the Fly app (`just-ingredients.fly.dev`): the only `Host` the doors answer
/// for, and the address `/cle` gives. `None` without an app name (local runs).
pub fn public_host(app_name: Option<&str>) -> Option<String> {
    app_name.map(|app| format!("{app}.fly.dev"))
}

/// The client's address for the limits: Fly's `Fly-Client-IP`, else the peer; an IPv6 address
/// by its /64, the block one subscriber usually holds.
pub fn address_key(headers: &axum::http::HeaderMap, peer: std::net::IpAddr) -> String {
    let address = headers
        .get("fly-client-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<std::net::IpAddr>().ok())
        .unwrap_or(peer);
    match address {
        std::net::IpAddr::V6(v6) if v6.to_ipv4_mapped().is_none() => {
            let [a, b, c, d, ..] = v6.segments();
            format!("{a:x}:{b:x}:{c:x}:{d:x}::/64")
        }
        std::net::IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or_else(|| v6.to_string(), |v4| v4.to_string()),
        std::net::IpAddr::V4(v4) => v4.to_string(),
    }
}
