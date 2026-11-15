use super::*;
use tokio::{net::TcpListener, io::{AsyncReadExt, AsyncWriteExt}};

#[test]
fn country_parser_and_mismatch_never_guess_from_an_ip_or_node_name() {
    assert_eq!(parse_country(b"ip=192.0.2.1\nloc=JP\n").unwrap(), "JP");
    for body in [b"".as_slice(), b"Tokyo", b"loc=XX", b"loc=JP\nloc=SG", b"<html>blocked</html>"] {
        assert_eq!(parse_country(body).unwrap_err(), UNKNOWN);
    }
    assert!(compare_country("JP", "JP").is_ok());
    assert_eq!(compare_country("JP", "SG").unwrap_err(), "PROXY_COUNTRY_MISMATCH:JP:SG");
}
#[tokio::test]
async fn country_probe_uses_only_the_selected_proxy_without_account_credentials() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut data = Vec::new();
        while !data.ends_with(b"\r\n\r\n") {
            data.push(socket.read_u8().await.unwrap());
            assert!(data.len() < 8192);
        }
        let request = String::from_utf8(data).unwrap();
        assert!(request.starts_with("GET http://unresolvable.invalid/ HTTP/1.1"));
        assert!(!request.to_ascii_lowercase().contains("authorization"));
        assert!(!request.to_ascii_lowercase().contains("cookie:"));
        let body = "ip=192.0.2.1\nloc=JP\n";
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let country = query_country(&probe_client(Some(&proxy), None).unwrap(), "http://unresolvable.invalid/").await.unwrap();
    assert_eq!(country, "JP");
    server.await.unwrap();
}
#[tokio::test]
async fn unavailable_proxy_cannot_fall_back_to_a_reachable_direct_origin() {
    let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", dead.local_addr().unwrap()); drop(dead);
    let url = format!("http://{}/", origin.local_addr().unwrap());
    assert_eq!(query_country(&probe_client(Some(&proxy), None).unwrap(), &url).await.unwrap_err(), UNKNOWN);
    assert!(tokio::time::timeout(Duration::from_millis(100), origin.accept()).await.is_err());
}
#[tokio::test]
async fn redirects_fail_without_contacting_a_different_target() {
    let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
    let redirect = format!("http://{}/", target.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = proxy.accept().await.unwrap();
        let mut data = vec![0;8192]; let _ = socket.read(&mut data).await.unwrap();
        socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: {redirect}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    });
    assert_eq!(query_country(&probe_client(Some(&proxy_url), None).unwrap(), "http://unresolvable.invalid/").await.unwrap_err(), UNKNOWN);
    server.await.unwrap();
    assert!(tokio::time::timeout(Duration::from_millis(100), target.accept()).await.is_err());
}
#[tokio::test]
async fn country_policy_is_optional_per_account_and_normalizes_gateway_bindings() {
    let _lock = crate::modules::test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("country-policy-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let keys = ["COCKPIT_TOOLS_TEST_DATA_DIR", "COCKPIT_TOOLS_DATA_DIR"];
    let previous = keys.map(std::env::var_os);
    struct Cleanup(PathBuf, [Option<std::ffi::OsString>;2]);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for (key, value) in ["COCKPIT_TOOLS_TEST_DATA_DIR", "COCKPIT_TOOLS_DATA_DIR"].into_iter().zip(&self.1) {
                match value { Some(v) => std::env::set_var(key,v), None => std::env::remove_var(key) }
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone(), previous);
    for key in keys { std::env::set_var(key, &root); }
    assert_eq!(policy("a").unwrap().expected_country, None);
    // No account file and no profile exists: disabled checking performs no
    // route resolution, credential lookup, profile write or network request.
    check_for_launch(Some("not-an-account"), &root.join("missing-profile")).await.unwrap();
    assert!(!policy_path().unwrap().exists());
    assert_eq!(save_policy("a",Some(" jp ".into())).unwrap().expected_country.as_deref(), Some("JP"));
    let wrapped = super::super::codex_instance::provider_gateway_bind_account_id("a").unwrap();
    assert_eq!(policy(&wrapped).unwrap().expected_country.as_deref(), Some("JP"));
    save_policy("b",Some("SG".into())).unwrap();
    assert!(save_policy("a",Some("XX".into())).is_err());
    assert_eq!(policy("a").unwrap().expected_country.as_deref(),Some("JP"));
    save_policy("a",None).unwrap();
    assert_eq!(policy("a").unwrap().expected_country,None);
    assert_eq!(policy("b").unwrap().expected_country.as_deref(),Some("SG"));
    assert!(!root.join("missing-profile").exists());
}
