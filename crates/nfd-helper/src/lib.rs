//! Elevated helper (PLAN §3.2 principle 6, docs/HARDWARE_MONITOR.md §4).
//!
//! The app stays unelevated. When the user turns on Advanced sensors it starts its
//! own executable with `--elevated-helper <pipe> <app pid>` through UAC; the helper
//! connects back to a named pipe the app created and then answers exactly four
//! requests — [`Request::Discover`], [`Request::Sample`], [`Request::Shutdown`] and
//! [`Request::Health`] (Phase 7: SMART thresholds, SMART through USB bridges and the
//! volume dirty bit — the fixed read commands of `nfd_health::cmd`, nothing else).
//! There is no request that runs code, opens files or writes hardware.
//!
//! Both ends check each other: the app only accepts the PID UAC started, the helper
//! only talks to the PID it was given, and both must be the same executable. The
//! pipe is local-only and its ACL admits the owner and administrators.

use serde::{Deserialize, Serialize};

pub use nfd_sensors::{Device, Kind, Note};

/// Command-line switch of the helper mode.
pub const HELPER_FLAG: &str = "--elevated-helper";

/// Wire version; both ends refuse a mismatch (stale helper after an update).
pub const PROTOCOL: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    Discover {
        protocol: u32,
    },
    Sample,
    Shutdown,
    /// Administrator-only drive reads for these drives / volumes (indices and roots
    /// only — never command bytes).
    Health {
        protocol: u32,
        drives: Vec<HealthDrive>,
        volumes: Vec<String>,
    },
}

/// A drive the app wants the helper to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthDrive {
    pub index: u32,
    /// Read through SCSI / ATA Translation (USB bridge) instead of `SMART_RCV_DRIVE_DATA`.
    pub usb: bool,
}

/// A sensor as sent over the pipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WireSensor {
    pub id: String,
    pub device: String,
    pub kind: Kind,
    pub label: String,
    pub index: Option<u32>,
    pub detail: Option<String>,
    pub primary: bool,
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Response {
    Discovery {
        protocol: u32,
        devices: Vec<Device>,
        sensors: Vec<WireSensor>,
        notes: Vec<Note>,
    },
    Sample {
        values: Vec<(usize, f64)>,
    },
    Health {
        privileged: nfd_health::Privileged,
    },
    /// Advanced sensors are not available in the helper (PawnIO missing…).
    Unavailable {
        reason: String,
    },
    Error {
        message: String,
    },
}

/// Largest frame accepted (a discovery of a 64-core CPU is ~40 KB).
pub const MAX_FRAME: usize = 1 << 20;

/// `u32` little-endian length + JSON.
pub fn encode<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = serde_json::to_vec(msg).unwrap_or_default();
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend(body);
    out
}

pub fn decode<T: for<'de> Deserialize<'de>>(frame: &[u8]) -> Option<T> {
    serde_json::from_slice(frame).ok()
}

/// Pipe name for this app instance.
pub fn pipe_name(app_pid: u32, nonce: u64) -> String {
    format!(r"\\.\pipe\nanofreedisk-helper-{app_pid}-{nonce:016x}")
}

/// Parse `--elevated-helper <pipe> <pid>` (only our own pipe names are accepted).
pub fn helper_args(args: &[String]) -> Option<(String, u32)> {
    let i = args.iter().position(|a| a == HELPER_FLAG)?;
    let pipe = args.get(i + 1)?.clone();
    let pid: u32 = args.get(i + 2)?.parse().ok()?;
    let ok = pipe.starts_with(r"\\.\pipe\nanofreedisk-helper-")
        && pipe.len() < 96
        && pipe[r"\\.\pipe\".len()..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    ok.then_some((pipe, pid))
}

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::{RemoteProvider, StartError, health_once, run_helper, start};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let r = Request::Discover { protocol: PROTOCOL };
        let f = encode(&r);
        assert_eq!(
            u32::from_le_bytes([f[0], f[1], f[2], f[3]]) as usize,
            f.len() - 4
        );
        assert_eq!(decode::<Request>(&f[4..]), Some(r));
        let s = Response::Sample {
            values: vec![(0, 62.5), (3, 51.0)],
        };
        assert_eq!(decode::<Response>(&encode(&s)[4..]), Some(s));
        assert_eq!(
            decode::<Request>(b"{\"op\":\"run\",\"cmd\":\"calc\"}"),
            None,
            "only the four requests exist"
        );
    }

    #[test]
    fn health_round_trip() {
        let r = Request::Health {
            protocol: PROTOCOL,
            drives: vec![HealthDrive {
                index: 2,
                usb: true,
            }],
            volumes: vec![r"C:\".into()],
        };
        assert_eq!(decode::<Request>(&encode(&r)[4..]), Some(r));
        let p = nfd_health::Privileged {
            smart: vec![nfd_health::PrivilegedSmart {
                disk: 2,
                data: vec![1; 512],
                thresholds: None,
                via: nfd_health::Source::Sat,
            }],
            volumes: vec![nfd_health::PrivilegedVolume {
                root: r"C:\".into(),
                dirty: false,
            }],
            sat_failed: vec![3],
        };
        let resp = Response::Health { privileged: p };
        let frame = encode(&resp);
        assert!(frame.len() < MAX_FRAME);
        assert_eq!(decode::<Response>(&frame[4..]), Some(resp));
        // a request carrying command bytes does not parse
        assert_eq!(
            decode::<Request>(
                br#"{"op":"health","protocol":2,"drives":[],"volumes":[],"cdb":[133]}"#
            )
            .map(|r| matches!(r, Request::Health { .. })),
            Some(true),
            "unknown fields are ignored, never executed"
        );
    }

    #[test]
    fn helper_args_are_strict() {
        let a = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let name = pipe_name(1234, 0xabcdef);
        assert_eq!(
            helper_args(&a(&["app.exe", HELPER_FLAG, &name, "1234"])),
            Some((name.clone(), 1234))
        );
        assert_eq!(
            helper_args(&a(&["app.exe", HELPER_FLAG, r"\\.\pipe\other", "1"])),
            None
        );
        assert_eq!(
            helper_args(&a(&[
                "app.exe",
                HELPER_FLAG,
                r"\\.\pipe\nanofreedisk-helper-1\..\x",
                "1"
            ])),
            None
        );
        assert_eq!(helper_args(&a(&["app.exe", HELPER_FLAG, &name])), None);
        assert_eq!(helper_args(&a(&["app.exe"])), None);
    }
}
