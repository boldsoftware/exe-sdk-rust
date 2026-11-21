//! Tests against exe.dev itself, which create and delete VMs.
//!
//! The token comes from EXE_TOKEN, or EXE_TOKEN_FILE (a bare token or token
//! JSON; default ~/.config/exedev/sdk-test-token). Without one, or with
//! EXE_SKIP_INTEGRATION set, these tests pass without doing anything.
//! EXE_SKIP_SSH_INTEGRATION skips what needs ssh. EXE_BASE_URL selects
//! another server, and EXE_TEST_TAG the tag the token scopes VMs to (default
//! sdk-test).
//!
//! It is one test, so that it makes few VMs, and deletes all of them.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use exedev::{Client, Error, WaitUntil};

fn token() -> Option<String> {
    if std::env::var_os("EXE_SKIP_INTEGRATION").is_some() {
        return None;
    }
    if let Ok(t) = std::env::var("EXE_TOKEN") {
        return Some(t.trim().to_string());
    }
    let path = std::env::var_os("EXE_TOKEN_FILE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(&std::env::var("HOME").unwrap_or_default()).join(".config/exedev/sdk-test-token")
        });
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    if text.starts_with('{') {
        let v: serde_json::Value = serde_json::from_str(text).expect("token JSON");
        return v["token"].as_str().map(str::to_string);
    }
    Some(text.to_string())
}

#[tokio::test]
async fn lifecycle() {
    let Some(token) = token() else {
        eprintln!("skipping: no exe.dev token (see tests/integration.rs)");
        return;
    };
    let mut b = Client::builder().token(token);
    if let Ok(url) = std::env::var("EXE_BASE_URL") {
        b = b.base_url(url);
    }
    let exe = b.build().unwrap();
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let name = format!("sdk-rs-{ms:x}");
    let names = [name.clone(), format!("{name}-r"), format!("{name}-c")];

    let result = run(&exe, &name).await;
    // Whatever happened, delete every VM this test may have made.
    let cleanup = exe.delete(names.iter().map(String::as_str)).await;
    result.unwrap();
    let cleanup = cleanup.expect("cleanup");
    let leaked: Vec<_> = cleanup
        .failed
        .iter()
        .filter(|n| !cleanup.not_found.as_ref().is_some_and(|nf| nf.contains(n)))
        .collect();
    assert!(leaked.is_empty(), "cleanup failed: {cleanup:?}");
}

async fn run(exe: &Client, name: &str) -> exedev::Result<()> {
    let ssh = std::env::var_os("EXE_SKIP_SSH_INTEGRATION").is_none();
    let tag = std::env::var("EXE_TEST_TAG").unwrap_or_else(|_| "sdk-test".into());

    let me = exe.whoami().await?;
    assert!(me.email.contains('@'), "{me:?}");

    let mut vm = exe
        .new_vm()
        .name(name)
        .comment("sdk integration test")
        .wait_until(if ssh { WaitUntil::Ssh } else { WaitUntil::Created })
        .await?;
    assert_eq!(vm.name(), name);
    assert!(vm.tags().contains(&tag), "tags {:?}", vm.tags());
    assert_eq!(vm.comment(), "sdk integration test");

    let listed = exe.vms().await?;
    assert!(listed.iter().any(|v| v.name() == name));
    let found = exe.vm(name).await?;
    assert_eq!(found.https_url(), vm.https_url());
    match exe.vm(&format!("{name}-x")).await {
        Err(e) if e.is_not_found() => {}
        other => panic!("missing VM: {other:?}"),
    }

    vm.set_comment("renamed soon").await?;
    assert_eq!(vm.comment(), "renamed soon");
    vm.rename(format!("{name}-r")).await?;
    assert_eq!(vm.name(), format!("{name}-r"));

    if ssh {
        let r = vm.run("echo $((6 * 7)); echo oops >&2; exit 3").await?;
        assert_eq!(
            (r.exit_code, r.stdout_lossy().as_str(), r.stderr_lossy().as_str()),
            (3, "42\n", "oops\n")
        );
        let mut lines = Vec::new();
        vm.run(["printf", "%s\\n", "a b", "it's"])
            .on_stdout(|l| lines.push(l.to_string()))
            .check(true)
            .await?;
        assert_eq!(lines, ["a b", "it's"]);
        match vm.run("sleep 30").timeout(Duration::from_secs(1)).await {
            Err(e) if e.is_timeout() => {}
            other => panic!("timeout: {other:?}"),
        }
        // A command longer than a request takes goes up as a file.
        let long = format!("echo {}", "x".repeat(100_000));
        assert_eq!(vm.run(long).check(true).await?.stdout.len(), 100_001);

        // Files.
        let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
        vm.write_file("t/data.bin", data.clone()).mode(0o600).await?;
        assert_eq!(vm.read_file("t/data.bin").await?, data);
        match vm.read_file("t/none").await {
            Err(Error::FileNotFound { .. }) => {}
            other => panic!("missing file: {other:?}"),
        }
        let dir = std::env::temp_dir().join(format!("{name}-up"));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.txt"), "a\n").unwrap();
        std::fs::write(dir.join("skip.log"), "no\n").unwrap();
        vm.upload(&dir, "up").exclude(["*.log"]).await?;
        assert_eq!(
            vm.run("cat up/sub/a.txt; ls up").check(true).await?.stdout_lossy(),
            "a\nsub\n"
        );
        let back = std::env::temp_dir().join(format!("{name}-down"));
        vm.download("up", &back).await?;
        assert_eq!(std::fs::read_to_string(back.join("sub/a.txt")).unwrap(), "a\n");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&back);

        // Processes outlive the connection.
        let mut p = vm
            .spawn("echo started; sleep 1; echo done >&2; exit 5")
            .name("job")
            .await?;
        let done = p.wait().timeout(Duration::from_secs(30)).await?;
        assert_eq!(done.exit_code, 5);
        assert_eq!(done.stdout_lossy(), "started\n");
        assert!(vm.processes().await?.iter().any(|q| q.name() == "job"));
        let mut sleeper = vm.spawn("sleep 600").await?;
        sleeper.kill().await?;
        let killed = sleeper.wait().timeout(Duration::from_secs(30)).await?;
        assert_ne!(killed.exit_code, 0);
    }

    // A copy, waited for only until it exists.
    let copy = vm
        .copy()
        .name(format!("{name}-c"))
        .wait_until(WaitUntil::Created)
        .await?;
    assert_eq!(copy.name(), format!("{name}-c"));
    copy.delete().await?;
    match exe.vm(&format!("{name}-c")).await {
        Err(e) if e.is_not_found() => {}
        other => panic!("deleted copy: {other:?}"),
    }
    vm.delete().await?;
    Ok(())
}
