use axum::{http::StatusCode, response::IntoResponse};
use std::{fs, os::unix::fs::PermissionsExt, process::Command};

#[tokio::test]
async fn remote_authorization_has_a_terminal_fallback_and_preserves_existing_rules() {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    let rules = temp.path().join("sudoers.d");
    fs::create_dir(&bin).unwrap();
    fs::create_dir(&rules).unwrap();
    for (name, script) in [
        (
            "sudo",
            r#"#!/usr/bin/python3
import os, sys
if sys.argv[1] == '-n':
    if '-k' not in sys.argv and os.path.exists(os.environ['TEST_CACHED']):
        sys.exit(0)
    sys.exit(0 if os.path.exists(os.environ['TEST_GRANT']) else 1)
assert sys.argv[1:3] == ['sh', '-c'], sys.argv
args = sys.argv[2:]
args[1] = args[1].replace('/etc/sudoers.d', os.environ['TEST_RULES'])
os.execv('/bin/sh', ['sh'] + args)
"#,
        ),
        (
            "pkexec",
            r#"#!/bin/sh
touch "$TEST_PKEXEC"
echo 'No authentication agent found.' >&2
exit 127
"#,
        ),
        (
            "visudo",
            r#"#!/bin/sh
if [ -f "$TEST_REJECT" ]; then exit 1; fi
exec /usr/sbin/visudo "$@"
"#,
        ),
    ] {
        let path = bin.join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    // This integration-test binary contains one test; subprocess overrides
    // never affect the application's other test binaries or the host grants.
    std::env::set_var("PATH", format!("{}:/usr/bin:/usr/sbin:/bin", bin.display()));
    for (name, path) in [
        ("TEST_GRANT", temp.path().join("grant")),
        ("TEST_CACHED", temp.path().join("cached-password")),
        ("TEST_PKEXEC", temp.path().join("pkexec-called")),
        ("TEST_REJECT", temp.path().join("reject")),
        ("TEST_RULES", rules.clone()),
    ] {
        std::env::set_var(name, path);
    }

    fs::write(temp.path().join("cached-password"), "").unwrap();
    let response = app_lib::api::handlers::system::setup_privileged_access()
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(response.into_body(), 16384)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("No authentication agent found"));
    assert!(body["message"].as_str().unwrap().contains("authorize"));

    let target = rules.join("diskless-manager");
    fs::write(&target, "existing rule\n").unwrap();
    fs::write(temp.path().join("reject"), "").unwrap();
    let failed = Command::new(env!("CARGO_BIN_EXE_diskless-manager"))
        .arg("authorize")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert_eq!(fs::read_to_string(&target).unwrap(), "existing rule\n");
    fs::remove_file(temp.path().join("reject")).unwrap();

    let installed = Command::new(env!("CARGO_BIN_EXE_diskless-manager"))
        .arg("authorize")
        .env("USER", "attacker'; touch /tmp/authorization-injection; '")
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let user = Command::new("/usr/bin/id").arg("-un").output().unwrap();
    let user = String::from_utf8(user.stdout).unwrap();
    let rule = fs::read_to_string(&target).unwrap();
    assert!(rule.starts_with(&format!("{} ALL=(ALL) NOPASSWD:", user.trim())));
    assert!(rule.contains("/usr/bin/systemctl"));
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o440
    );
    assert_eq!(fs::read_dir(&rules).unwrap().count(), 1);

    let invalid_args = Command::new(env!("CARGO_BIN_EXE_diskless-manager"))
        .args(["authorize", "other-user"])
        .output()
        .unwrap();
    assert_eq!(invalid_args.status.code(), Some(2));

    // A desktop agent authorizes the same script; only elevation and its
    // permission probe are substituted. All filesystem operations remain real.
    fs::write(
        bin.join("pkexec"),
        r#"#!/usr/bin/python3
import os, sys
assert sys.argv[1:4] == ['--disable-internal-agent', 'sh', '-c'], sys.argv
open(os.environ['TEST_PKEXEC'], 'w').close()
args = sys.argv[3:]
args[1] = args[1].replace('/etc/sudoers.d', os.environ['TEST_RULES'])
args[1] += '\ntouch "$TEST_GRANT"\n'
os.execv('/bin/sh', ['sh'] + args)
"#,
    )
    .unwrap();
    let response = app_lib::api::handlers::system::setup_privileged_access()
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fs::read_to_string(&target).unwrap(), rule);

    fs::write(temp.path().join("grant"), "").unwrap();
    fs::remove_file(temp.path().join("pkexec-called")).unwrap();
    let response = app_lib::api::handlers::system::setup_privileged_access()
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!temp.path().join("pkexec-called").exists());
}
