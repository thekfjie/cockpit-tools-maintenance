//! Transport-only startup checks; never send OAuth, quota, or model requests.
//! This protects managed HTTP/TCP routing, not arbitrary native UDP/WebRTC.
use super::process::CodexLaunchProxy;
use std::time::Duration;
const FAILED: &str = "PROXY_LAUNCH_CHECK_FAILED";

async fn check_url(proxy: &str, target: &str) -> Result<(), String> {
    let proxy = reqwest::Proxy::all(proxy).map_err(|_| FAILED)?;
    let client = reqwest::Client::builder().no_proxy().proxy(proxy)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(12))
        .build().map_err(|_| FAILED)?;
    // Any HTTP response proves the TLS transport; a login challenge / WAF
    // response is not an OAuth failure and must not trigger credential refresh.
    client.head(target).send().await.map_err(|_| FAILED)?;
    Ok(())
}
pub async fn preflight(proxy: &CodexLaunchProxy) -> Result<(), String> {
    if !cfg!(windows) { return Ok(()); }
    let CodexLaunchProxy::Proxy(url) = proxy else { return Ok(()); };
    // Only the stable account loopback entry is allowed in client configuration.
    proxy.dotenv_overrides()?;
    check_url(url, "https://chatgpt.com/").await
}

pub fn verify_profile(home: &std::path::Path, account: &str, proxy: &CodexLaunchProxy) -> Result<(), String> {
    if !cfg!(windows) { return Ok(()); }
    super::codex_proxy_dotenv::verify(home, account, proxy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{net::TcpListener, io::{AsyncReadExt, AsyncWriteExt}};
    #[tokio::test]
    async fn probe_uses_only_the_explicit_proxy_and_never_sends_credentials() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = vec![0; 8192];
            let n = socket.read(&mut data).await.unwrap();
            let request = String::from_utf8_lossy(&data[..n]);
            assert!(request.starts_with("HEAD http://unresolvable.invalid/ HTTP/1.1"));
            assert!(!request.to_ascii_lowercase().contains("authorization"));
            socket.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        check_url(&proxy, "http://unresolvable.invalid/").await.unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn dead_proxy_never_retries_using_the_reachable_origin() {
        let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = format!("http://{}", dead.local_addr().unwrap());
        drop(dead);
        let target = format!("http://{}/", origin.local_addr().unwrap());
        assert_eq!(check_url(&proxy, &target).await.unwrap_err(), FAILED);
        assert!(tokio::time::timeout(Duration::from_millis(100), origin.accept()).await.is_err());
    }
    #[tokio::test]
    async fn unbound_modes_do_not_open_a_network_connection() {
        for mode in [CodexLaunchProxy::Inherit, CodexLaunchProxy::ClientDefaults, CodexLaunchProxy::Direct] {
            preflight(&mode).await.unwrap();
        }
    }
}

pub fn verify_or_stop(home: &std::path::Path, account: &str, pid: u32, proxy: &CodexLaunchProxy) -> Result<(), String> {
    if let Err(error) = verify_profile(home, account, proxy) {
        super::process::close_pid(pid, 5).map_err(|_| "PROXY_LAUNCH_STOP_FAILED")?;
        return Err(error);
    }
    Ok(())
}
