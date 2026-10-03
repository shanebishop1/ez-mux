//! Real CLI boundary tests: no prompts and no user's `OpenCode` service.
use std::{fs, os::unix::fs::PermissionsExt, process::Command};
use tempfile::TempDir;

fn invoke(root: &TempDir, extra: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("XDG_STATE_HOME", root.path().join("state"))
        .env("EZM_OPENCODE_BIN", root.path().join("opencode"))
        .args([
            "__internal",
            "opencode",
            "--server",
            "http://localhost:4096",
            "--directory",
            "/remote/work tree",
            "--key",
            "project/review/1",
        ])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn v2_reopen_reuses_conversation_and_new_is_explicit() {
    let root = TempDir::new().unwrap();
    let script = root.path().join("opencode");
    fs::write(&script, r"#!/usr/bin/env python3
import sys,os,json
from pathlib import Path
p=Path(os.environ['HOME'])/'calls'
a=sys.argv[1:]
with p.open('a') as f:f.write(json.dumps(a)+'\n')
if a[0]=='api':
 if 'POST' in a:
  print(json.dumps({'data':{'id':'ses_test'+str(len(p.read_text().splitlines()))}}))
 else:print(json.dumps({'data':{'id':a[-1].split('/')[-1], 'location':{'directory':'/remote/work tree'}}}))
").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    for extra in [&[][..], &[][..], &["--new"][..]] {
        let result = invoke(&root, extra);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let calls: Vec<Vec<String>> = fs::read_to_string(root.path().join("calls"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        calls.iter().filter(|a| a.contains(&"POST".into())).count(),
        2
    );
    let clients: Vec<_> = calls
        .iter()
        .filter(|a| a.first().is_some_and(|s| s == "--server"))
        .collect();
    assert_eq!(clients.len(), 3);
    assert_eq!(clients[0], clients[1]);
    assert_ne!(clients[1], clients[2]);
    assert!(!calls.iter().any(|a| a.contains(&"attach".into())));
}

#[test]
fn v2_failure_does_not_replace_saved_conversation() {
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("opencode"), "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(
        root.path().join("opencode"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let result = invoke(&root, &[]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("OpenCode API"));
}

#[test]
fn concurrent_open_creates_only_one_conversation() {
    let root = TempDir::new().unwrap();
    let script = root.path().join("opencode");
    fs::write(
        &script,
        r"#!/usr/bin/env python3
import sys,os,json,time
from pathlib import Path
if sys.argv[1]=='api':
 if 'POST' in sys.argv:
  with (Path(os.environ['HOME'])/'created').open('a') as f:f.write('created\n')
  time.sleep(0.1)
 print(json.dumps({'data':{'id':'ses_concurrent','location':{'directory':'/remote/work tree'}}}))
",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    std::thread::scope(|scope| {
        let a = scope.spawn(|| invoke(&root, &[]));
        let b = scope.spawn(|| invoke(&root, &[]));
        assert!(a.join().unwrap().status.success());
        assert!(b.join().unwrap().status.success());
    });
    assert_eq!(
        fs::read_to_string(root.path().join("created"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    // A later failure cannot erase or replace the persisted identity.
    fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
    assert!(!invoke(&root, &[]).status.success());
    let records: Vec<_> = fs::read_dir(root.path().join("state/ez-mux/conversations"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(records.len(), 1);
    assert!(
        fs::read_to_string(records[0].path())
            .unwrap()
            .contains("ses_concurrent")
    );
}

#[test]
fn imported_identity_requires_explicit_adoption_and_checks_directory() {
    let root = TempDir::new().unwrap();
    let script = root.path().join("opencode");
    fs::write(
        &script,
        r"#!/usr/bin/env python3
import sys,json
assert 'POST' not in sys.argv
print(json.dumps({'data':{'id':'ses_adopted','location':{'directory':'/remote/work tree'}}}))
",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    let missing = invoke(&root, &["--require-existing", "--prepare-only"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("ezm adopt"));
    assert!(!invoke(&root, &["--adopt", "ses_wrong"]).status.success());
    let adopted = invoke(&root, &["--adopt", "ses_adopted"]);
    assert!(
        adopted.status.success(),
        "{}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    assert!(
        invoke(&root, &["--require-existing", "--prepare-only"])
            .status
            .success()
    );
}
