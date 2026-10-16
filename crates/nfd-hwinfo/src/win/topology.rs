//! Cores, threads, P/E split and caches from `GetLogicalProcessorInformationEx`.

use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationAll,
};

/// Processor topology.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Topology {
    pub packages: u32,
    pub cores: u32,
    pub threads: u32,
    /// Hybrid CPUs: cores of the highest efficiency class (P) and the rest (E).
    pub p_cores: Option<u32>,
    pub e_cores: Option<u32>,
    /// Totals in bytes (sum over all instances).
    pub l1: u64,
    pub l2: u64,
    pub l3: u64,
    /// Efficiency class of each logical processor, indexed by its number in group 0
    /// (used to label per-thread sensors "P" / "E").
    pub thread_class: Vec<u8>,
}

/// Parse the variable-length records the API returns (x86-64 layout).
pub fn parse(buf: &[u8]) -> Topology {
    let mut t = Topology::default();
    let mut classes: Vec<(u8, u32)> = Vec::new(); // (efficiency class, threads of the core)
    let rd32 = |b: &[u8], o: usize| -> u32 {
        b.get(o..o + 4)
            .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
            .unwrap_or(0)
    };
    let mut pos = 0;
    while pos + 8 <= buf.len() {
        let rel = rd32(buf, pos);
        let size = rd32(buf, pos + 4) as usize;
        if size < 8 || pos + size > buf.len() {
            break;
        }
        let r = &buf[pos..pos + size];
        match rel {
            0 => {
                // RelationProcessorCore: Flags, EfficiencyClass, Reserved[20], GroupCount, GroupMask[]
                t.cores += 1;
                let class = r.get(9).copied().unwrap_or(0);
                let groups = r
                    .get(30..32)
                    .map(|x| u16::from_le_bytes([x[0], x[1]]))
                    .unwrap_or(0) as usize;
                let mut threads = 0;
                for g in 0..groups {
                    let off = 32 + g * 16;
                    let Some(m) = r.get(off..off + 8) else { break };
                    let mask = u64::from_le_bytes(m.try_into().unwrap_or([0; 8]));
                    let group = r
                        .get(off + 8..off + 10)
                        .map(|x| u16::from_le_bytes([x[0], x[1]]))
                        .unwrap_or(0);
                    threads += mask.count_ones();
                    if group == 0 {
                        for bit in 0..64 {
                            if mask & (1 << bit) != 0 {
                                if t.thread_class.len() <= bit {
                                    t.thread_class.resize(bit + 1, 0);
                                }
                                t.thread_class[bit] = class;
                            }
                        }
                    }
                }
                t.threads += threads;
                classes.push((class, threads));
            }
            2 => {
                // RelationCache: Level, Associativity, LineSize, CacheSize, Type
                let level = r.get(8).copied().unwrap_or(0);
                let size = u64::from(rd32(r, 12));
                match level {
                    1 => t.l1 += size,
                    2 => t.l2 += size,
                    3 => t.l3 += size,
                    _ => {}
                }
            }
            3 => t.packages += 1,
            _ => {}
        }
        pos += size;
    }
    let max = classes.iter().map(|c| c.0).max();
    let min = classes.iter().map(|c| c.0).min();
    if let (Some(max), Some(min)) = (max, min) {
        if max != min {
            let p = classes.iter().filter(|c| c.0 == max).count() as u32;
            t.p_cores = Some(p);
            t.e_cores = Some(t.cores - p);
        }
    }
    t
}

pub fn native() -> Topology {
    let mut len = 0u32;
    // SAFETY: size query with a null buffer.
    unsafe { GetLogicalProcessorInformationEx(RelationAll, std::ptr::null_mut(), &mut len) };
    if len == 0 {
        return Topology::default();
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: buffer of `len` bytes.
    let ok =
        unsafe { GetLogicalProcessorInformationEx(RelationAll, buf.as_mut_ptr().cast(), &mut len) };
    if ok == 0 {
        return Topology::default();
    }
    buf.truncate(len as usize);
    parse(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(class: u8, mask: u64) -> Vec<u8> {
        let mut r = vec![0u8; 48];
        r[0..4].copy_from_slice(&0u32.to_le_bytes());
        r[4..8].copy_from_slice(&48u32.to_le_bytes());
        r[9] = class;
        r[30..32].copy_from_slice(&1u16.to_le_bytes());
        r[32..40].copy_from_slice(&mask.to_le_bytes());
        r
    }

    fn cache(level: u8, size: u32) -> Vec<u8> {
        let mut r = vec![0u8; 48];
        r[0..4].copy_from_slice(&2u32.to_le_bytes());
        r[4..8].copy_from_slice(&48u32.to_le_bytes());
        r[8] = level;
        r[12..16].copy_from_slice(&size.to_le_bytes());
        r
    }

    #[test]
    fn hybrid_split() {
        // 2 P-cores with HT (class 1) + 4 E-cores (class 0)
        let mut buf = Vec::new();
        buf.extend(core(1, 0b11));
        buf.extend(core(1, 0b1100));
        for i in 4..8 {
            buf.extend(core(0, 1 << i));
        }
        buf.extend(cache(2, 2 << 20));
        buf.extend(cache(3, 36 << 20));
        let t = parse(&buf);
        assert_eq!((t.cores, t.threads), (6, 8));
        assert_eq!((t.p_cores, t.e_cores), (Some(2), Some(4)));
        assert_eq!((t.l2, t.l3), (2 << 20, 36 << 20));
        assert_eq!(t.thread_class, vec![1, 1, 1, 1, 0, 0, 0, 0]);
    }

    #[test]
    fn uniform_cpu_has_no_split() {
        let mut buf = core(0, 0b11);
        buf.extend(core(0, 0b1100));
        let t = parse(&buf);
        assert_eq!(t.p_cores, None);
        assert_eq!(t.threads, 4);
        assert_eq!(parse(&[1, 2, 3]), Topology::default());
    }
}
