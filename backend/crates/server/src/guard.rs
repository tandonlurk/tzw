//! Request guard (SEC-02): Host allowlist and Origin check, always on, plus a
//! bearer token that is required whenever one is configured. `main` refuses
//! to bind a non-loopback address without a token.
//!
//! - **Host** (DNS rebinding): a browser tricked into resolving
//!   `evil.example` to 127.0.0.1 still sends `Host: evil.example`, so only
//!   allowlisted host names are served. A request without any Host is not a
//!   browser request (browsers always send one) and is let through to the
//!   next checks.
//! - **Origin** (CSRF): a request that carries an `Origin` other than the
//!   configured frontend origin (`CORS_ORIGIN`) is refused before it reaches
//!   a handler, whatever its method. Requests without `Origin` (curl, server
//!   side code, top-level navigation) are unaffected.
//! - **Token**: with `AEN_API_TOKEN` set, every request except
//!   `GET /api/health` and `OPTIONS` needs `Authorization: Bearer <token>`.

use std::fmt;
use std::net::IpAddr;

use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, HOST, ORIGIN, WWW_AUTHENTICATE};
use axum::http::{HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;

/// Host names served by default (any port).
pub const DEFAULT_ALLOWED_HOSTS: [&str; 3] = ["localhost", "127.0.0.1", "[::1]"];
/// Shortest accepted `AEN_API_TOKEN`.
pub const MIN_TOKEN_LEN: usize = 32;

/// The API token. Its `Debug` never prints the value.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiToken(String);

impl ApiToken {
    /// `None` when the token is shorter than `MIN_TOKEN_LEN` (after trim).
    pub fn new(token: &str) -> Option<ApiToken> {
        let token = token.trim();
        (token.len() >= MIN_TOKEN_LEN).then(|| ApiToken(token.to_owned()))
    }

    fn matches(&self, presented: &[u8]) -> bool {
        constant_time_eq(self.0.as_bytes(), presented)
    }
}

impl fmt::Debug for ApiToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiToken(<redacted>)")
    }
}

/// What the guard enforces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardConfig {
    /// Lowercase host names without port (`localhost`, `[::1]`, ...).
    pub allowed_hosts: Vec<String>,
    pub api_token: Option<ApiToken>,
}

impl Default for GuardConfig {
    fn default() -> Self {
        GuardConfig {
            allowed_hosts: DEFAULT_ALLOWED_HOSTS.map(str::to_owned).to_vec(),
            api_token: None,
        }
    }
}

impl GuardConfig {
    /// Adds a comma-separated list of extra host names (`ALLOWED_HOSTS`).
    pub fn with_extra_hosts(mut self, list: &str) -> GuardConfig {
        for host in list.split(',').map(str::trim).filter(|h| !h.is_empty()) {
            let host = host_name(host).to_ascii_lowercase();
            if !self.allowed_hosts.contains(&host) {
                self.allowed_hosts.push(host);
            }
        }
        self
    }
}

/// Why a server configuration is refused at startup, if it is.
pub fn startup_error(bind: IpAddr, config: &GuardConfig) -> Option<String> {
    if !bind.is_loopback() && config.api_token.is_none() {
        return Some(format!(
            "BIND_ADDR {bind} is not loopback: set AEN_API_TOKEN (at least {MIN_TOKEN_LEN} \
             characters) so the API is not open to the network"
        ));
    }
    // An extra host name means requests arrive through something else (a
    // tunnel or proxy, e.g. for the QuickNode webhook), so the rest of the
    // API must not be open to it.
    let extra = config
        .allowed_hosts
        .iter()
        .any(|h| !DEFAULT_ALLOWED_HOSTS.contains(&h.as_str()));
    if extra && config.api_token.is_none() {
        return Some(format!(
            "ALLOWED_HOSTS adds host names reachable from outside: set AEN_API_TOKEN (at least \
             {MIN_TOKEN_LEN} characters) so only the HMAC-signed webhook works without it"
        ));
    }
    None
}

/// `host[:port]` without the port; IPv6 literals keep their brackets.
fn host_name(host: &str) -> &str {
    if host.starts_with('[') {
        return host.find(']').map_or(host, |end| &host[..=end]);
    }
    host.split_once(':').map_or(host, |(name, _)| name)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn forbidden(code: &'static str, message: &str) -> Response {
    ApiError::new(StatusCode::FORBIDDEN, code, message).into_response()
}

/// Shared state of the guard middleware.
#[derive(Clone)]
pub struct Guard {
    pub config: GuardConfig,
    pub origin: HeaderValue,
}

pub async fn guard(State(guard): State<Guard>, req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host = headers
        .get(HOST)
        .and_then(|h| h.to_str().ok().map(str::to_owned))
        .or_else(|| req.uri().authority().map(|a| a.as_str().to_owned()));
    if let Some(host) = host {
        let name = host_name(host.trim()).to_ascii_lowercase();
        if !guard.config.allowed_hosts.contains(&name) {
            return forbidden(
                "host_not_allowed",
                "this Host is not served (add it to ALLOWED_HOSTS if it is yours)",
            );
        }
    }
    if let Some(origin) = headers.get(ORIGIN)
        && origin != guard.origin
    {
        return forbidden(
            "origin_not_allowed",
            "requests from this Origin are refused (only CORS_ORIGIN may call the API)",
        );
    }
    if let Some(token) = &guard.config.api_token {
        // The QuickNode webhook authenticates itself with its HMAC signature
        // (QuickNode cannot send our bearer token); Host and Origin still apply.
        let exempt = req.method() == Method::OPTIONS
            || (req.method() == Method::GET && req.uri().path() == "/api/health")
            || (req.method() == Method::POST && req.uri().path() == crate::quicknode::PATH);
        let presented = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.as_bytes().strip_prefix(b"Bearer "));
        if !exempt && !presented.is_some_and(|p| token.matches(p)) {
            let mut res = ApiError::new(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "a valid `Authorization: Bearer <AEN_API_TOKEN>` header is required",
            )
            .into_response();
            res.headers_mut()
                .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            return res;
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_names_drop_the_port() {
        assert_eq!(host_name("localhost:8080"), "localhost");
        assert_eq!(host_name("127.0.0.1"), "127.0.0.1");
        assert_eq!(host_name("[::1]:8080"), "[::1]");
        assert_eq!(host_name("evil.example:80"), "evil.example");
    }

    #[test]
    fn extra_hosts_are_normalized() {
        let c = GuardConfig::default().with_extra_hosts(" API.example.com:443 , ,10.0.0.5");
        assert!(c.allowed_hosts.contains(&"api.example.com".to_owned()));
        assert!(c.allowed_hosts.contains(&"10.0.0.5".to_owned()));
        assert_eq!(c.allowed_hosts.len(), 5);
    }

    #[test]
    fn non_loopback_bind_needs_a_token() {
        let open = GuardConfig::default();
        assert!(startup_error("127.0.0.1".parse().unwrap(), &open).is_none());
        assert!(startup_error("::1".parse().unwrap(), &open).is_none());
        assert!(startup_error("0.0.0.0".parse().unwrap(), &open).is_some());
        let locked = GuardConfig {
            api_token: ApiToken::new(&"x".repeat(MIN_TOKEN_LEN)),
            ..GuardConfig::default()
        };
        assert!(startup_error("0.0.0.0".parse().unwrap(), &locked).is_none());
        // A tunnel host on a loopback bind also needs the token.
        let tunnel = GuardConfig::default().with_extra_hosts("abc.trycloudflare.com");
        assert!(startup_error("127.0.0.1".parse().unwrap(), &tunnel).is_some());
        let tunnel = GuardConfig {
            api_token: ApiToken::new(&"x".repeat(MIN_TOKEN_LEN)),
            ..tunnel
        };
        assert!(startup_error("127.0.0.1".parse().unwrap(), &tunnel).is_none());
    }

    #[test]
    fn tokens_are_long_and_never_printed() {
        assert!(ApiToken::new("short").is_none());
        let t = ApiToken::new(&"s".repeat(40)).unwrap();
        assert!(!format!("{t:?}").contains("sss"));
        assert!(t.matches("s".repeat(40).as_bytes()));
        assert!(!t.matches("s".repeat(39).as_bytes()));
    }
}
