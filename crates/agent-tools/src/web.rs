use crate::{BuiltinToolError, WebToolPolicy};
use agent_runtime::{
    ToolDefinition, ToolExecutionContext, ToolHandler, ToolHandlerError, ToolRisk,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Url;
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub struct WebFetchTool {
    policy: WebToolPolicy,
}

impl WebFetchTool {
    pub fn new(mut policy: WebToolPolicy) -> Result<Self, BuiltinToolError> {
        let mut domains = Vec::new();
        for domain in policy.allowed_domains {
            let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
            let valid = !domain.is_empty()
                && domain.len() <= 253
                && domain.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                });
            if !valid || domains.contains(&domain) {
                return Err(BuiltinToolError::InvalidConfig("web_domain"));
            }
            domains.push(domain);
        }
        policy.allowed_domains = domains;
        Ok(Self { policy })
    }

    pub fn definition() -> ToolDefinition {
        ToolDefinition {
            name: "web.fetch".into(),
            description: "Fetch one bounded allowlisted public URL using a read-only GET".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "minLength": 8, "maxLength": 4096}
                },
                "required": ["url"],
                "additionalProperties": false
            }),
            risk: ToolRisk::External,
            timeout_ms: 30_000,
            max_result_bytes: 512 * 1024,
        }
    }

    fn domain_allowed(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.policy.allowed_domains.iter().any(|domain| {
            host == *domain
                || (self.policy.allow_subdomains
                    && host
                        .strip_suffix(domain)
                        .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1))
        })
    }
}

#[async_trait]
impl ToolHandler for WebFetchTool {
    async fn execute(
        &self,
        context: ToolExecutionContext,
        arguments: Value,
    ) -> Result<String, ToolHandlerError> {
        let raw_url = arguments
            .get("url")
            .and_then(Value::as_str)
            .ok_or(ToolHandlerError)?;
        let url = Url::parse(raw_url).map_err(|_| ToolHandlerError)?;
        let valid_scheme =
            url.scheme() == "https" || (self.policy.allow_http && url.scheme() == "http");
        if !valid_scheme || !url.username().is_empty() || url.password().is_some() {
            return Err(ToolHandlerError);
        }
        let host = url.host_str().ok_or(ToolHandlerError)?;
        if !self.domain_allowed(host) {
            return Err(ToolHandlerError);
        }
        let port = url.port_or_known_default().ok_or(ToolHandlerError)?;
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| ToolHandlerError)?
            .collect::<Vec<_>>();
        if addresses.is_empty()
            || (!self.policy.allow_private_network
                && addresses.iter().any(|address| !is_public_ip(address.ip())))
        {
            return Err(ToolHandlerError);
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|_| ToolHandlerError)?;
        let response = client
            .get(url.clone())
            .header(
                reqwest::header::ACCEPT,
                "text/plain, text/markdown, application/json;q=0.9, */*;q=0.1",
            )
            .send()
            .await
            .map_err(|_| ToolHandlerError)?;
        if response.status().is_redirection() {
            return Err(ToolHandlerError);
        }
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.chars().take(127).collect::<String>());
        let final_url = response.url().as_str().to_owned();
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if context.cancellation.is_cancelled() {
                return Err(ToolHandlerError);
            }
            let chunk = chunk.map_err(|_| ToolHandlerError)?;
            if body.len().saturating_add(chunk.len()) > self.policy.max_response_bytes {
                return Err(ToolHandlerError);
            }
            body.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(body).map_err(|_| ToolHandlerError)?;
        Ok(json!({
            "status": status,
            "url": final_url,
            "content_type": content_type,
            "body": body
        })
        .to_string())
    }

    fn summarize_arguments(&self, arguments: &Value) -> Option<String> {
        let raw = arguments.get("url").and_then(Value::as_str)?;
        let url = Url::parse(raw).ok()?;
        Some(format!("Fetch {}://{}", url.scheme(), url.host_str()?))
    }
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || ip.is_unspecified()
        || octets[0] == 0
        || octets[0] >= 240
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && (18..=19).contains(&octets[1])))
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_runtime::{NeverCancel, ToolExecutionContext};
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn context() -> ToolExecutionContext {
        ToolExecutionContext {
            run_id: "run".into(),
            call_id: "call".into(),
            cancellation: Arc::new(NeverCancel),
        }
    }

    #[test]
    fn classifies_public_and_non_public_addresses() {
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
        for private in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(private.parse().unwrap()), "{private}");
        }
    }

    #[tokio::test]
    async fn test_only_policy_fetches_bounded_loopback_without_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ok"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/large"))
            .respond_with(ResponseTemplate::new(200).set_body_string("12345678901234567"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/binary"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0xff, 0xfe]))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/ok"))
            .mount(&server)
            .await;
        let host = Url::parse(&server.uri())
            .unwrap()
            .host_str()
            .unwrap()
            .to_owned();
        let tool = WebFetchTool::new(WebToolPolicy {
            allowed_domains: vec![host],
            allow_subdomains: false,
            allow_http: true,
            allow_private_network: true,
            max_response_bytes: 16,
        })
        .unwrap();
        let output = tool
            .execute(context(), json!({"url":format!("{}/ok", server.uri())}))
            .await
            .unwrap();
        assert!(output.contains("hello"));
        assert!(tool
            .execute(
                context(),
                json!({"url":format!("{}/redirect", server.uri())})
            )
            .await
            .is_err());
        assert!(tool
            .execute(context(), json!({"url":format!("{}/large", server.uri())}))
            .await
            .is_err());
        assert!(tool
            .execute(context(), json!({"url":format!("{}/binary", server.uri())}))
            .await
            .is_err());
        assert!(tool
            .execute(
                context(),
                json!({"url":format!("{}/ok", server.uri()).replace("127.0.0.1", "localhost")})
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn production_defaults_reject_http_credentials_and_private_network() {
        let tool = WebFetchTool::new(WebToolPolicy {
            allowed_domains: vec!["localhost".into()],
            ..WebToolPolicy::default()
        })
        .unwrap();
        assert!(tool
            .execute(context(), json!({"url":"http://localhost/test"}))
            .await
            .is_err());
        assert!(tool
            .execute(context(), json!({"url":"https://user:pass@localhost/test"}))
            .await
            .is_err());
        assert!(tool
            .execute(context(), json!({"url":"https://localhost/test"}))
            .await
            .is_err());
    }
}
