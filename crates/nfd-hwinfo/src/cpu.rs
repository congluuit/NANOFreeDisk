//! CPU identification through the `CPUID` instruction (x86 / x86-64 only; other
//! architectures report what the OS knows). No driver, no privilege.

/// What `CPUID` tells about the processor this thread runs on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CpuId {
    pub vendor: String,
    pub brand: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    /// Leaf 0x16 (Intel): base / max turbo / bus clock in MHz.
    pub base_mhz: Option<u32>,
    pub max_mhz: Option<u32>,
    pub bus_mhz: Option<u32>,
    /// Intel hybrid (Alder Lake and newer).
    pub hybrid: bool,
    /// Instruction-set extensions worth listing ("AVX2", "AVX-512F", "VT-x"…).
    pub features: Vec<&'static str>,
}

/// Decode the leaves `cpuid(leaf, subleaf) -> [eax, ebx, ecx, edx]` returns. Kept
/// separate from the instruction so it can be tested with recorded values.
pub fn decode(cpuid: &dyn Fn(u32, u32) -> [u32; 4]) -> CpuId {
    let l0 = cpuid(0, 0);
    let max_leaf = l0[0];
    let vendor = regs_str(&[l0[1], l0[3], l0[2]]);
    let mut out = CpuId {
        vendor: vendor_name(&vendor),
        ..CpuId::default()
    };
    if max_leaf >= 1 {
        let [eax, _, ecx, edx] = cpuid(1, 0);
        let base_family = (eax >> 8) & 0xF;
        let base_model = (eax >> 4) & 0xF;
        out.stepping = eax & 0xF;
        out.family = if base_family == 0xF {
            base_family + ((eax >> 20) & 0xFF)
        } else {
            base_family
        };
        out.model = if base_family == 0x6 || base_family == 0xF {
            base_model | (((eax >> 16) & 0xF) << 4)
        } else {
            base_model
        };
        let mut f = |on: bool, name: &'static str| {
            if on {
                out.features.push(name);
            }
        };
        f(edx & (1 << 26) != 0, "SSE2");
        f(ecx & (1 << 0) != 0, "SSE3");
        f(ecx & (1 << 19) != 0, "SSE4.1");
        f(ecx & (1 << 20) != 0, "SSE4.2");
        f(ecx & (1 << 25) != 0, "AES");
        f(ecx & (1 << 28) != 0, "AVX");
        f(ecx & (1 << 12) != 0, "FMA3");
        f(ecx & (1 << 5) != 0, "VT-x");
    }
    if max_leaf >= 7 {
        let [_, ebx, ecx, edx] = cpuid(7, 0);
        let mut f = |on: bool, name: &'static str| {
            if on {
                out.features.push(name);
            }
        };
        f(ebx & (1 << 5) != 0, "AVX2");
        f(ebx & (1 << 16) != 0, "AVX-512F");
        f(ebx & (1 << 29) != 0, "SHA");
        f(ecx & (1 << 11) != 0, "AVX-VNNI512");
        out.hybrid = edx & (1 << 15) != 0;
    }
    if max_leaf >= 0x16 && out.vendor == "Intel" {
        let [a, b, c, _] = cpuid(0x16, 0);
        out.base_mhz = (a & 0xFFFF != 0).then_some(a & 0xFFFF);
        out.max_mhz = (b & 0xFFFF != 0).then_some(b & 0xFFFF);
        out.bus_mhz = (c & 0xFFFF != 0).then_some(c & 0xFFFF);
    }
    let ext_max = cpuid(0x8000_0000, 0)[0];
    if ext_max >= 0x8000_0001 {
        let ecx = cpuid(0x8000_0001, 0)[2];
        if ecx & (1 << 2) != 0 {
            out.features.push("AMD-V");
        }
    }
    if ext_max >= 0x8000_0004 {
        let mut regs = Vec::with_capacity(12);
        for leaf in 0x8000_0002..=0x8000_0004u32 {
            regs.extend_from_slice(&cpuid(leaf, 0));
        }
        out.brand = clean_brand(&regs_str(&regs));
    }
    out
}

fn regs_str(regs: &[u32]) -> String {
    let bytes: Vec<u8> = regs.iter().flat_map(|r| r.to_le_bytes()).collect();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn vendor_name(id: &str) -> String {
    match id {
        "GenuineIntel" => "Intel".into(),
        "AuthenticAMD" => "AMD".into(),
        other => other.trim().to_owned(),
    }
}

/// "13th Gen Intel(R) Core(TM) i9-13900HX" — collapse runs of spaces, drop "(R)"/"(TM)"
/// is left to the UI; only whitespace is normalized here.
fn clean_brand(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The real instruction.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn native() -> CpuId {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::__cpuid_count;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::__cpuid_count;
    // SAFETY: CPUID is available on every x86-64 CPU and on all x86 CPUs Windows 10
    // supports; it has no side effects.
    #[allow(unused_unsafe)]
    decode(&|leaf, sub| {
        let r = unsafe { __cpuid_count(leaf, sub) };
        [r.eax, r.ebx, r.ecx, r.edx]
    })
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn native() -> CpuId {
    CpuId::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Leaves recorded on an i9-13900HX (Raptor Lake, family 6 model 0xB7).
    fn raptor(leaf: u32, _sub: u32) -> [u32; 4] {
        match leaf {
            0 => [0x20, 0x756e_6547, 0x6c65_746e, 0x4965_6e69],
            1 => [0x000B_0671, 0, 0x7FFA_FBFF, 0xBFEB_FBFF],
            7 => [0, 0x239C_A7EB, 0, 0xFC18_C410],
            0x16 => [2200, 5400, 100, 0],
            0x8000_0000 => [0x8000_0008, 0, 0, 0],
            0x8000_0001 => [0, 0, 0x121, 0x2C10_0800],
            0x8000_0002 => [0x6874_3331, 0x6e65_4720, 0x746e_4920, 0x5228_6c65],
            0x8000_0003 => [0x6f43_2029, 0x5428_6572, 0x6920_294d, 0x3331_2d39],
            0x8000_0004 => [0x4830_3039, 0x58, 0, 0],
            _ => [0; 4],
        }
    }

    #[test]
    fn decodes_raptor_lake() {
        let c = decode(&raptor);
        assert_eq!(c.vendor, "Intel");
        assert_eq!(c.brand, "13th Gen Intel(R) Core(TM) i9-13900HX");
        assert_eq!((c.family, c.model, c.stepping), (6, 0xB7, 1));
        assert_eq!(
            (c.base_mhz, c.max_mhz, c.bus_mhz),
            (Some(2200), Some(5400), Some(100))
        );
        assert!(c.hybrid);
        assert!(c.features.contains(&"AVX2"));
        assert!(!c.features.contains(&"AVX-512F"));
    }

    #[test]
    fn amd_family_uses_extended_bits() {
        let zen4 = |leaf: u32, _sub: u32| match leaf {
            0 => [0x10, 0x6874_7541, 0x444d_4163, 0x6974_6e65],
            1 => [0x00A6_0F12, 0, 0, 0],
            _ => [0; 4],
        };
        let c = decode(&zen4);
        assert_eq!(c.vendor, "AMD");
        assert_eq!((c.family, c.model), (0x19, 0x61));
        assert_eq!(c.base_mhz, None);
    }

    #[test]
    fn native_does_not_panic() {
        let _ = native();
    }
}
