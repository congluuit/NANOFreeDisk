//! Physical network adapters (`GetAdaptersAddresses`). MAC and IP addresses are
//! deliberately left out of the inventory.

use windows_sys::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    GAA_FLAG_SKIP_UNICAST, GetAdaptersAddresses, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
    IP_ADAPTER_ADDRESSES_LH,
};

use crate::NetAdapter;

fn wstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: NUL-terminated string owned by the adapter list.
    unsafe {
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }
}

pub fn native() -> Vec<NetAdapter> {
    const AF_UNSPEC: u32 = 0;
    let flags = GAA_FLAG_SKIP_UNICAST
        | GAA_FLAG_SKIP_ANYCAST
        | GAA_FLAG_SKIP_MULTICAST
        | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = 16 * 1024u32;
    let mut buf: Vec<u64> = Vec::new();
    for _ in 0..3 {
        buf = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: buffer of `size` bytes, 8-byte aligned.
        let rc = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC,
                flags,
                std::ptr::null(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if rc == 0 {
            break;
        }
        if rc != 111 {
            // not ERROR_BUFFER_OVERFLOW
            return Vec::new();
        }
    }
    let mut out = Vec::new();
    let mut p = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !p.is_null() {
        // SAFETY: linked list inside `buf`.
        let a = unsafe { &*p };
        let kind = match a.IfType {
            IF_TYPE_ETHERNET_CSMACD => Some("ethernet"),
            IF_TYPE_IEEE80211 => Some("wifi"),
            _ => None,
        };
        let desc = wstr(a.Description);
        // skip virtual switches, VPN and filter drivers that pose as Ethernet
        let virt = [
            "virtual",
            "hyper-v",
            "vpn",
            "tap-",
            "wan miniport",
            "loopback",
            "bluetooth",
        ]
        .iter()
        .any(|w| desc.to_ascii_lowercase().contains(w));
        if let Some(kind) = kind {
            if !virt && a.PhysicalAddressLength > 0 {
                let up = a.OperStatus == 1;
                out.push(NetAdapter {
                    name: desc,
                    alias: Some(wstr(a.FriendlyName)).filter(|s| !s.is_empty()),
                    kind: kind.to_owned(),
                    connected: up,
                    link_bps: (up && a.TransmitLinkSpeed != u64::MAX && a.TransmitLinkSpeed > 0)
                        .then_some(a.TransmitLinkSpeed),
                });
            }
        }
        p = a.Next;
    }
    out
}
