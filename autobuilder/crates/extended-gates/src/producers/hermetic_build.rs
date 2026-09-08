//! `hermetic-build`: detect outbound network sockets during `cargo build --offline`.
//!
//! Linux-only. Spawns `cargo build --offline` as a tracked child, then polls
//! `/proc` at a bounded interval (≤1s) for the duration of the build,
//! attributing sockets to the build's own process tree — never to whatever
//! else is running on the machine.
//!
//! ## Why process-tree attribution (not machine-wide before/after)
//!
//! The original implementation diffed `/proc/net/tcp{,6}` machine-wide
//! before and after the build: any socket that appeared anywhere on the
//! host between the two snapshots was blamed on the build. On a workstation
//! that always has cohabiting network activity (agorabus's NATS link,
//! tailscale, a running Claude session hitting the Anthropic API, snap
//! refreshes...) that produces false blocks routinely, and — symmetrically
//! — would miss a build-tree connection that opened and closed between the
//! two snapshots. This version instead:
//!
//! 1. Spawns cargo as a tracked child and walks `/proc` iteratively (BFS
//!    from the child's pid over the live `pid -> ppid` map, never
//!    recursing) to find its full descendant set on every sample tick.
//! 2. For each descendant, reads `/proc/<pid>/fd/*` to collect the socket
//!    inodes it holds open.
//! 3. Joins those inodes against `/proc/net/{tcp,tcp6,udp,udp6}` to get
//!    connection details, and records only rows whose inode belongs to the
//!    tree.
//! 4. Samples on an interval short enough (150ms, well under the 1s bound)
//!    to catch a connection that opens and closes entirely within the
//!    build, at the residual risk of missing something shorter than one
//!    tick — `sample_interval_ms` in the receipt states that bound.
//!
//! A cohabiting process's sockets never appear (its pid is never in the
//! tree). A build-tree child's socket is caught live even if it closes
//! before the build ends, as long as it outlives one sample tick.
//!
//! The spawned cargo runs with `RUSTC_WRAPPER` / `CARGO_BUILD_RUSTC_WRAPPER`
//! forced empty: a hermetic build has no wrapper by definition, and a
//! developer machine with `rustc-wrapper = sccache` set in
//! `~/.cargo/config.toml` would otherwise have sccache's localhost client
//! connections misreported as hermeticity violations. Loopback remote
//! addresses (`127.0.0.0/8`, `::1`) are excluded from attribution by
//! default for the same reason — a local build cache or daemon on
//! localhost is not "outbound." Unix-domain sockets are never examined at
//! all (only the four `/proc/net` TCP/UDP tables are read), so sccache's
//! local protocol never has to be filtered explicitly there either. Both
//! ignore rules are named in the receipt's `ignore_rules` field. `--strict`
//! (wired via [`set_strict`], called by the bin before `run_producer`) also
//! attributes loopback connections, for crates that must not even talk
//! locally.
//!
//! On non-Linux hosts the producer emits `verdict=skipped`.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::prelude::{ProducerSpec, write_receipt};

/// Set by `hermetic-build --strict` (via [`set_strict`]) before
/// `run_producer` is called. A process-wide flag rather than a CLI
/// parameter threaded through [`crate::prelude::run_producer`] because
/// that entry point's signature is shared by all 17 producers; a plain
/// safe atomic avoids reaching for `unsafe { std::env::set_var }` (denied
/// workspace-wide) just to pass one producer's one flag.
static STRICT: AtomicBool = AtomicBool::new(false);

/// Called by the `hermetic-build` bin when `--strict` is passed. Must be
/// called (if at all) before `run_producer("hermetic-build", ..)`.
pub fn set_strict(strict: bool) {
    STRICT.store(strict, Ordering::SeqCst);
}

/// Sampling interval for the /proc poll loop. Well under the ≤1s bound the
/// PRD requires; short enough to reliably catch a socket that opens and
/// closes entirely within a build step in practice. This is the bound
/// stated in the receipt's `sample_interval_ms` field — an ultra-short
/// socket living for less than one tick is a documented residual risk.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(150);

/// One socket attributed to the build's own process tree.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct AttributedSocket {
    /// pid (within the build's process tree) that held the socket fd.
    pid: i32,
    /// `/proc/<pid>/comm` at attribution time (best-effort; `"?"` if the
    /// process had already exited by the time we tried to read it).
    comm: String,
    /// Which `/proc/net/*` table the row came from: `tcp`, `tcp6`, `udp`,
    /// or `udp6`.
    proto: &'static str,
    /// Local `ip:port`.
    local: String,
    /// Remote `ip:port` — the destination.
    remote: String,
    /// Raw `/proc/net` state field (e.g. `"01"` = ESTABLISHED, `"02"` =
    /// `SYN_SENT`).
    state: String,
}

#[derive(Debug, Serialize)]
struct Payload {
    platform: String,
    new_sockets: Vec<AttributedSocket>,
    cargo_exit_code: Option<i32>,
    rustc_wrapper_disabled: bool,
    /// The sampling interval bound, in milliseconds — a socket that opens
    /// and fully closes within less than this window between two polls
    /// could be missed. States the residual-risk bound the PRD calls for.
    sample_interval_ms: u64,
    /// Which categories of build-tree sockets are excluded from
    /// attribution by default: `"loopback"` (unless `--strict`) and
    /// `"unix-domain"` (always — never examined).
    ignore_rules: Vec<&'static str>,
    /// True if `--strict` was requested (loopback also attributed).
    strict: bool,
}

/// Decode a `/proc/net/tcp{,6}` hex address field (before the `:port`) into
/// its real address bytes. The kernel stores the address as one or more
/// 32-bit words in host byte order; on every hermetic-build host (`x86_64`/
/// `aarch64` Linux, both little-endian) that means each 4-byte word must be
/// reversed to get network-order bytes.
fn decode_hex_addr(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 8 != 0 || hex.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for word in hex.as_bytes().chunks(8) {
        let word_str = std::str::from_utf8(word).ok()?;
        let mut word_bytes = [0u8; 4];
        for (i, wb) in word_bytes.iter_mut().enumerate() {
            let pair = word_str.get(i * 2..i * 2 + 2)?;
            *wb = u8::from_str_radix(pair, 16).ok()?;
        }
        word_bytes.reverse();
        out.extend_from_slice(&word_bytes);
    }
    Some(out)
}

/// True if the `/proc/net/tcp{,6}` hex remote address (without the port) is
/// a loopback address: `127.0.0.0/8` for IPv4, `::1` for IPv6.
fn is_loopback_hex_addr(hex: &str) -> bool {
    let Some(bytes) = decode_hex_addr(hex) else {
        return false;
    };
    match bytes.len() {
        4 => bytes.first() == Some(&127),
        16 => bytes.iter().take(15).all(|b| *b == 0) && bytes.get(15) == Some(&1),
        _ => false,
    }
}

/// Format a `/proc/net` `"<addrhex>:<porthex>"` field as human-readable
/// `ip:port`. Falls back to the raw field if it can't be decoded so a
/// receipt never silently drops evidence.
fn format_hex_sockaddr(field: &str) -> String {
    let mut parts = field.split(':');
    let (Some(addr_hex), Some(port_hex)) = (parts.next(), parts.next()) else {
        return field.to_owned();
    };
    let port = u16::from_str_radix(port_hex, 16).unwrap_or(0);
    match decode_hex_addr(addr_hex) {
        Some(bytes) if bytes.len() == 4 => match bytes.as_slice() {
            [a, b, c, d] => format!("{a}.{b}.{c}.{d}:{port}"),
            _ => field.to_owned(),
        },
        Some(bytes) if bytes.len() == 16 => {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&bytes);
            format!("{}:{port}", std::net::Ipv6Addr::from(arr))
        }
        _ => field.to_owned(),
    }
}

/// Read `ppid` for a single pid from `/proc/<pid>/stat`. `comm` (the 2nd
/// field) may itself contain spaces or parens, so we locate the *last*
/// `)` and parse fields after it, where `state` is always first and `ppid`
/// second per the documented format. Returns `None` on any read/parse
/// failure — including the pid having already exited (ESRCH-equivalent
/// ENOENT), which callers must tolerate silently rather than treat as an
/// error.
fn read_ppid(pid: i32) -> Option<i32> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rparen = text.rfind(')')?;
    let rest = text.get(rparen + 1..)?;
    let mut fields = rest.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse::<i32>().ok()
}

/// Best-effort `/proc/<pid>/comm`. Returns `"?"` rather than erroring if the
/// process has already exited by the time we read it — a race that is
/// expected and tolerated, not a bug.
fn read_comm(pid: i32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map_or_else(|_| "?".to_owned(), |s| s.trim().to_owned())
}

/// Every socket inode a pid currently holds open, via `/proc/<pid>/fd/*`
/// symlinks of the form `socket:[12345]`. Returns an empty vec (never an
/// error) if the pid has already exited or a race drops an individual fd
/// entry mid-read — both are expected under concurrent sampling.
fn socket_inodes(pid: i32) -> Vec<u64> {
    let dir = format!("/proc/{pid}/fd");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(target) = std::fs::read_link(entry.path()) else {
            continue;
        };
        let Some(s) = target.to_str() else { continue };
        if let Some(inode_str) = s.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')) {
            if let Ok(inode) = inode_str.parse::<u64>() {
                out.push(inode);
            }
        }
    }
    out
}

/// The full descendant set of `root` (root included), found iteratively —
/// a single `/proc` scan to build the live `pid -> ppid` map, then a BFS
/// queue walk. Never recurses, so a build tree with hundreds of children
/// doesn't risk stack depth; bounded by the number of live pids on the
/// host, walked once per sample tick.
fn descendants(root: i32) -> BTreeSet<i32> {
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
                continue;
            };
            if let Some(ppid) = read_ppid(pid) {
                children.entry(ppid).or_default().push(pid);
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    queue.push_back(root);
    visited.insert(root);
    while let Some(p) = queue.pop_front() {
        if let Some(kids) = children.get(&p) {
            for &k in kids {
                if visited.insert(k) {
                    queue.push_back(k);
                }
            }
        }
    }
    visited
}

/// One parsed row from a `/proc/net/{tcp,tcp6,udp,udp6}` table.
struct ConnRow {
    inode: u64,
    local: String,
    remote: String,
    state: String,
    loopback: bool,
}

/// Parse a `/proc/net/<name>` table, keeping only rows with a real remote
/// endpoint (excludes listening sockets and unconnected UDP sockets, both
/// of which report `remote = 00000000:0000` / the IPv6 equivalent).
fn read_conn_table(name: &str) -> Vec<ConnRow> {
    let path = format!("/proc/net/{name}");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let _sl = f.next()?;
            let local = f.next()?;
            let remote = f.next()?;
            let state = f.next()?;
            let _tx_rx_queue = f.next()?;
            let _tr_tm_when = f.next()?;
            let _retrnsmt = f.next()?;
            let _uid = f.next()?;
            let _timeout = f.next()?;
            let inode = f.next()?.parse::<u64>().ok()?;
            if remote.ends_with(":0000") {
                return None;
            }
            let remote_addr_hex = remote.split(':').next().unwrap_or("");
            let loopback = is_loopback_hex_addr(remote_addr_hex);
            Some(ConnRow {
                inode,
                local: format_hex_sockaddr(local),
                remote: format_hex_sockaddr(remote),
                state: state.to_owned(),
                loopback,
            })
        })
        .collect()
}

/// One sampling tick: find the build tree's current descendants, the
/// socket inodes they hold, join against the four connection tables, and
/// merge newly-observed attributed sockets into `out` (deduped by
/// `(pid, proto, local, remote, state)` across ticks, so `out` accumulates
/// the union over the whole build window rather than just the last tick —
/// this is what catches a connection that closes between ticks).
fn sample_once(root_pid: i32, strict: bool, out: &mut BTreeSet<AttributedSocket>) {
    let tree = descendants(root_pid);
    if tree.is_empty() {
        return;
    }
    let mut inode_to_pid: HashMap<u64, i32> = HashMap::new();
    for &pid in &tree {
        for inode in socket_inodes(pid) {
            inode_to_pid.entry(inode).or_insert(pid);
        }
    }
    if inode_to_pid.is_empty() {
        return;
    }
    for proto in ["tcp", "tcp6", "udp", "udp6"] {
        for row in read_conn_table(proto) {
            let Some(&pid) = inode_to_pid.get(&row.inode) else {
                continue;
            };
            if row.loopback && !strict {
                continue;
            }
            let comm = read_comm(pid);
            out.insert(AttributedSocket {
                pid,
                comm,
                proto,
                local: row.local,
                remote: row.remote,
                state: row.state,
            });
        }
    }
}

/// Run the hermetic-build audit.
///
/// # Errors
///
/// Returns an error if cargo can't be spawned or the receipt write fails.
pub fn run(spec: &ProducerSpec, project: &Path) -> Result<String> {
    if !cfg!(target_os = "linux") {
        write_receipt(
            project,
            spec,
            "skipped",
            Payload {
                platform: std::env::consts::OS.to_owned(),
                new_sockets: Vec::new(),
                cargo_exit_code: None,
                rustc_wrapper_disabled: false,
                sample_interval_ms: u64::try_from(SAMPLE_INTERVAL.as_millis()).unwrap_or(u64::MAX),
                ignore_rules: Vec::new(),
                strict: false,
            },
        )?;
        return Ok(format!(
            "hermetic-build: skipped (platform={})",
            std::env::consts::OS
        ));
    }

    // Wired by the `hermetic-build --strict` CLI flag (see src/bin/hermetic_build.rs
    // and set_strict above).
    let strict = STRICT.load(Ordering::SeqCst);
    let ignore_rules: Vec<&'static str> = if strict {
        vec!["unix-domain"]
    } else {
        vec!["loopback", "unix-domain"]
    };

    // A hermetic build has no rustc wrapper by definition. Force both env
    // vars empty so a locally configured `rustc-wrapper = sccache` in
    // ~/.cargo/config.toml doesn't get invoked and misattribute its
    // localhost client sockets to this build.
    let mut child = Command::new("cargo")
        .args(["build", "--offline", "--release"])
        .current_dir(project)
        .env("RUSTC_WRAPPER", "")
        .env("CARGO_BUILD_RUSTC_WRAPPER", "")
        .spawn()
        .context("spawn cargo build --offline")?;
    // Safe on the platforms this producer runs on (Linux only, gated above):
    // Child::id() always fits i32-range pids there.
    let root_pid = i32::try_from(child.id()).unwrap_or(i32::MAX);

    let mut attributed: BTreeSet<AttributedSocket> = BTreeSet::new();
    let exit = loop {
        sample_once(root_pid, strict, &mut attributed);
        match child.try_wait().context("poll cargo build child")? {
            Some(status) => break status.code(),
            None => std::thread::sleep(SAMPLE_INTERVAL),
        }
    };
    // One more sample right after exit is observed: catches anything alive
    // in the gap between the last poll and the child reaping. Any pid
    // that's already gone by now is tolerated silently (socket_inodes /
    // read_ppid / read_comm all degrade to empty/"?" rather than erroring).
    sample_once(root_pid, strict, &mut attributed);

    let new_sockets: Vec<AttributedSocket> = attributed.into_iter().collect();
    let verdict = if new_sockets.is_empty() && exit == Some(0) {
        "pass"
    } else {
        "block"
    };
    let summary = format!(
        "hermetic-build: cargo exit={exit:?}, {} attributed socket(s) (tree-scoped{})",
        new_sockets.len(),
        if strict { ", strict" } else { "" }
    );
    write_receipt(
        project,
        spec,
        verdict,
        Payload {
            platform: std::env::consts::OS.to_owned(),
            new_sockets,
            cargo_exit_code: exit,
            rustc_wrapper_disabled: true,
            sample_interval_ms: u64::try_from(SAMPLE_INTERVAL.as_millis()).unwrap_or(u64::MAX),
            ignore_rules,
            strict,
        },
    )?;
    Ok(summary)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn decodes_ipv4_loopback() {
        // 127.0.0.1 stored little-endian-per-word as "0100007F".
        assert!(is_loopback_hex_addr("0100007F"));
    }

    #[test]
    fn decodes_ipv4_non_loopback() {
        // 10.0.0.5 stored as "0500000A".
        assert!(!is_loopback_hex_addr("0500000A"));
    }

    #[test]
    fn decodes_ipv6_loopback() {
        // ::1 as 16 bytes is 15 zero bytes then a trailing 1. Stored as 4
        // little-endian 32-bit words, the last word (bytes[12..16], value
        // [0,0,0,1]) is byte-reversed to "01000000"; the rest are all-zero.
        let hex = "00000000000000000000000001000000";
        assert_eq!(hex.len(), 32);
        assert!(is_loopback_hex_addr(hex));
    }

    #[test]
    fn non_loopback_ipv6_is_not_flagged() {
        assert!(!is_loopback_hex_addr(
            "00000000000000000000000000000000"
        ));
    }

    #[test]
    fn formats_ipv4_sockaddr() {
        assert_eq!(format_hex_sockaddr("0100007F:0050"), "127.0.0.1:80");
    }

    #[test]
    fn descendants_includes_self_for_current_process() {
        // The current test process is at minimum its own descendant.
        let pid = i32::try_from(std::process::id()).unwrap();
        let set = descendants(pid);
        assert!(set.contains(&pid));
    }

    #[test]
    fn socket_inodes_for_exited_pid_is_empty_not_panicking() {
        // A pid that (almost certainly) never existed / has long since
        // exited: ESRCH-equivalent races must degrade to empty, not panic
        // or error.
        let bogus_pid = i32::MAX - 1;
        assert!(socket_inodes(bogus_pid).is_empty());
        assert_eq!(read_comm(bogus_pid), "?");
        assert_eq!(read_ppid(bogus_pid), None);
    }
}
