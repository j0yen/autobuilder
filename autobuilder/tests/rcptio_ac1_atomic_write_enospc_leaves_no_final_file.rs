//! AC1 (P0, PRD-autobuilder-receipt-write-verify): given a receipts
//! directory on a filesystem that fails writes with `ENOSPC`, a receipt
//! writer leaves no final file, no `.tmp` remains, and the error names the
//! path and `ENOSPC`.
//!
//! Real `ENOSPC` needs a genuinely full filesystem, not a mock — so this
//! test mounts a 4 KiB `tmpfs` inside an unprivileged user+mount namespace
//! (`unshare --mount --map-root-user`, no root or CI config required) and
//! re-execs this SAME test binary inside it via `--exact ... --nocapture`,
//! with an env flag (`RCPTIO_ENOSPC_CHILD`) telling the re-exec to run the
//! actual assertions instead of doing the unshare dance again. The outer
//! invocation just checks the child's exit status — a real libtest failure
//! (panic) inside the child surfaces as a non-zero exit here too.
//!
//! If the host can't create user/mount namespaces at all (`unshare` missing
//! or blocked), the preflight probe below fails and the test is skipped
//! with a clear reason rather than reported as a code failure — the
//! mechanism being unavailable is an environment fact, not a regression in
//! `autobuilder_receipt::write`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::doc_markdown)]

use std::env;
use std::path::PathBuf;
use std::process::Command;

const CHILD_ENV: &str = "RCPTIO_ENOSPC_CHILD";
const MNT_ENV: &str = "RCPTIO_ENOSPC_MNT";
const TEST_NAME: &str = "rcptio_ac1_atomic_write_enospc_leaves_no_final_file";

fn unshare_tmpfs_available() -> bool {
    // Same probe sequence the real test performs, run once with a throwaway
    // mount point so a genuinely-unavailable mechanism skips cleanly.
    let probe_dir = env::temp_dir().join(format!("rcptio-probe-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&probe_dir);
    let script = format!(
        "mount -t tmpfs -o size=4k tmpfs {p} && umount {p}",
        p = probe_dir.display()
    );
    let ok = Command::new("unshare")
        .args(["--mount", "--map-root-user", "--", "sh", "-c", &script])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = std::fs::remove_dir_all(&probe_dir);
    ok
}

#[test]
fn rcptio_ac1_atomic_write_enospc_leaves_no_final_file() {
    if env::var(CHILD_ENV).is_ok() {
        run_child_assertions();
        return;
    }

    if !unshare_tmpfs_available() {
        eprintln!(
            "skipping rcptio_ac1: this host cannot mount tmpfs inside an \
             unprivileged user+mount namespace (unshare unavailable/blocked) \
             — environment limitation, not a code failure"
        );
        return;
    }

    let mnt = env::temp_dir().join(format!("rcptio-ac1-mnt-{}", std::process::id()));
    std::fs::create_dir_all(&mnt).unwrap();
    let exe = env::current_exe().unwrap();
    let script = format!(
        "mount -t tmpfs -o size=4k tmpfs {mnt} && exec {exe} --exact {test} --nocapture",
        mnt = mnt.display(),
        exe = exe.display(),
        test = TEST_NAME,
    );
    let output = Command::new("unshare")
        .args(["--mount", "--map-root-user", "--", "sh", "-c", &script])
        .env(CHILD_ENV, "1")
        .env(MNT_ENV, &mnt)
        .output()
        .expect("failed to spawn unshare");
    let _ = std::fs::remove_dir_all(&mnt);

    assert!(
        output.status.success(),
        "child (inside tmpfs ENOSPC namespace) failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs inside the unshared mount namespace, on the freshly-mounted 4 KiB
/// tmpfs — real `ENOSPC` from the kernel, not a simulation.
fn run_child_assertions() {
    let mnt = PathBuf::from(env::var(MNT_ENV).expect("RCPTIO_ENOSPC_MNT must be set"));
    let receipts_dir = mnt.join("receipts");
    std::fs::create_dir_all(&receipts_dir).expect("mkdir inside tmpfs");

    let path = receipts_dir.join("flake-audit-receipt.json");
    // Padded well past the 4 KiB tmpfs budget so the write genuinely
    // exhausts the filesystem instead of merely almost filling it.
    let padding = "x".repeat(64 * 1024);
    let value = serde_json::json!({
        "schema": "autobuilder.flake_audit_receipt.v1",
        "verdict": "pass",
        "padding": padding,
    });

    let err = autobuilder_receipt::write(&path, value)
        .expect_err("write must fail on a full filesystem");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("28") || msg.to_lowercase().contains("no space"),
        "error must name the OS error (ENOSPC / os error 28): {msg}"
    );
    assert!(
        msg.contains(&path.display().to_string())
            || msg.contains(&format!("{}.tmp", path.display())),
        "error must name the path: {msg}"
    );

    assert!(!path.exists(), "no final receipt file may exist after a failed write");
    let tmp_path = {
        let mut s = path.clone().into_os_string();
        s.push(".tmp");
        PathBuf::from(s)
    };
    assert!(!tmp_path.exists(), "no .tmp file may remain after a failed write");
}
