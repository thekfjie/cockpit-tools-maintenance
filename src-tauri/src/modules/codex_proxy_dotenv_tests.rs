use super::*;
fn route(port: u16) -> CodexLaunchProxy { CodexLaunchProxy::Proxy(format!("socks5://127.0.0.1:{port}")) }
fn step(state: State, document: Option<String>, account: &str, proxy: CodexLaunchProxy) -> (State, Option<String>) {
    let (mut state, next) = transition(state, document, account, &proxy).unwrap();
    state.pending = None; (state, next)
}
#[test]
fn absent_empty_and_original_dotenv_roundtrip_across_accounts() {
    for original in [None, Some(String::new()), Some("# client\r\nHTTPS_PROXY=http://127.0.0.1:7890\r\nSECRET=fixture".into())] {
        let (a, projected) = step(State::default(), original.clone(), "account-a", route(41001));
        assert!(projected.as_deref().unwrap().contains("HTTPS_PROXY='socks5://127.0.0.1:41001'"));
        let (b, restored) = step(a, projected, "account-b", CodexLaunchProxy::ClientDefaults);
        assert_eq!(restored, original);
        let (a, projected) = step(b, restored, "account-a", route(41002));
        let (_, restored) = step(a, projected, "account-b", CodexLaunchProxy::Inherit);
        assert_eq!(restored, original);
    }
}
#[test]
fn account_defaults_and_unrelated_user_edits_survive_switching() {
    let original = Some("HTTPS_PROXY=http://first.invalid:8080\nUSER=original\n".into());
    let (a, projected) = step(State::default(), original, "a", route(41001));
    let edited = projected.map(|s| s.replace("USER=original", "USER=edited"));
    let (b, restored) = step(a, edited, "b", CodexLaunchProxy::ClientDefaults);
    assert!(restored.as_deref().unwrap().contains("USER=edited"));
    let b_settings = Some(restored.unwrap().replace("first.invalid", "second.invalid"));
    let (a, projected) = step(b, b_settings, "a", route(41002));
    assert!(a.active.as_ref().unwrap().original.as_deref().unwrap().contains("first.invalid"));
    let (_, restored) = step(a, projected, "b", CodexLaunchProxy::ClientDefaults);
    assert!(restored.as_deref().unwrap().contains("second.invalid"));
    assert!(restored.as_deref().unwrap().contains("USER=edited"));
}
#[test]
fn direct_overrides_dotenv_but_client_defaults_does_not() {
    let original = Some("HTTP_PROXY=http://client.invalid:7890\n".into());
    let (state, projected) = step(State::default(), original.clone(), "direct", CodexLaunchProxy::Direct);
    assert!(projected.as_deref().unwrap().contains("HTTP_PROXY=''"));
    assert!(projected.as_deref().unwrap().contains("NO_PROXY='*'"));
    let (_, restored) = step(state, projected, "original", CodexLaunchProxy::ClientDefaults);
    assert_eq!(restored, original);
}
#[test]
fn manual_changes_to_managed_keys_are_never_silently_overwritten() {
    let (state, projected) = step(State::default(), None, "a", route(41001));
    let edited = projected.map(|s| s.replace(":41001", ":41002"));
    assert_eq!(transition(state, edited, "b", &route(41003)).err().unwrap(), CONFLICT);
    assert_eq!(transition(State::default(), Some(format!("{BEGIN}\n")), "a", &route(41001)).err().unwrap(), CONFLICT);
}
#[test]
fn dotenv_grouping_preserves_quoted_multiline_values() {
    let text = "TOKEN='first\nHTTPS_PROXY=inside-token\nlast'\nexport https_proxy=old\n";
    let merged = merge_defaults(Some(text), Some("HTTP_PROXY=new\n")).unwrap().unwrap();
    assert!(merged.contains("HTTPS_PROXY=inside-token"));
    assert!(!merged.contains("https_proxy=old"));
    assert!(merged.ends_with("HTTP_PROXY=new\n"));
    assert!(parts("TOKEN='unterminated\n").is_err());
}
#[test]
fn generated_dotenv_never_contains_upstream_credentials_or_remote_endpoints() {
    for raw in ["http://user:secret@127.0.0.1:41001", "socks5://remote.invalid:443", "file:///fixture"] {
        assert!(CodexLaunchProxy::Proxy(raw.into()).dotenv_overrides().is_err());
    }
}
#[test]
fn recoverable_dotenv_journal_completes_both_crash_windows_and_retains_conflicts() {
    let _lock = super::super::test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let root = std::env::temp_dir().join(format!("proxy-dotenv-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join("home")).unwrap();
    let previous = std::env::var_os("COCKPIT_TOOLS_TEST_DATA_DIR");
    struct Cleanup(PathBuf, Option<std::ffi::OsString>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            match &self.1 { Some(v) => std::env::set_var("COCKPIT_TOOLS_TEST_DATA_DIR", v), None => std::env::remove_var("COCKPIT_TOOLS_TEST_DATA_DIR") }
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone(), previous);
    std::env::set_var("COCKPIT_TOOLS_TEST_DATA_DIR", root.join("app"));
    let home = root.join("home");
    let path = state_path(&home).unwrap();
    let (pending, after) = transition(State::default(), None, "a", &route(41001)).unwrap();
    write_state(&path, &pending).unwrap();
    apply_inner(&home, "a", &route(41001)).unwrap();
    assert_eq!(read_document(&home.join(".env")).unwrap(), after);
    assert!(!home.join(".env.bak").exists());
    write_state(&path, &pending).unwrap();
    apply_inner(&home, "b", &CodexLaunchProxy::ClientDefaults).unwrap();
    assert!(!home.join(".env").exists());
    write_state(&path, &pending).unwrap();
    fs::write(home.join(".env"), "USER=newer").unwrap();
    assert_eq!(apply_inner(&home, "a", &route(41001)).unwrap_err(), CONFLICT);
    assert_eq!(fs::read_to_string(home.join(".env")).unwrap(), "USER=newer");
    assert!(!fs::read_to_string(path).unwrap().contains("USER="));
}
