//! Minimal Performance Data Helper wrapper: one query, English counter paths (so it
//! works on a Vietnamese Windows too), wildcard instances read as arrays.

use windows_sys::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW,
};

/// `PDH_FMT_NOCAP100`: keep values above 100 % (multi-engine sums).
const NOCAP100: u32 = 0x8000;

#[derive(Debug)]
pub struct Query {
    q: PDH_HQUERY,
}

#[derive(Debug, Clone, Copy)]
pub struct Counter(PDH_HCOUNTER);

// SAFETY: PDH handles are process-wide; the hub serializes every call.
unsafe impl Send for Query {}
unsafe impl Send for Counter {}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl Query {
    pub fn new() -> Option<Self> {
        let mut q = std::ptr::null_mut();
        // SAFETY: out-param.
        (unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut q) } == 0).then_some(Self { q })
    }

    /// Add `\Object(instance)\Counter` (English names).
    pub fn add(&mut self, path: &str) -> Option<Counter> {
        let p = wide(path);
        let mut c = std::ptr::null_mut();
        // SAFETY: valid query and NUL-terminated path.
        (unsafe { PdhAddEnglishCounterW(self.q, p.as_ptr(), 0, &mut c) } == 0).then_some(Counter(c))
    }

    pub fn collect(&mut self) -> bool {
        // SAFETY: valid query.
        unsafe { PdhCollectQueryData(self.q) == 0 }
    }

    /// Every instance of a (wildcard) counter with valid data: `(instance, value)`.
    pub fn values(&self, c: Counter) -> Vec<(String, f64)> {
        self.items(c, false)
    }

    /// Instance names, even before a rate counter has its second sample.
    pub fn instances(&self, c: Counter) -> Vec<String> {
        self.items(c, true).into_iter().map(|(n, _)| n).collect()
    }

    fn items(&self, c: Counter, any_status: bool) -> Vec<(String, f64)> {
        let mut size = 0u32;
        let mut count = 0u32;
        // SAFETY: size query.
        let rc = unsafe {
            PdhGetFormattedCounterArrayW(
                c.0,
                PDH_FMT_DOUBLE | NOCAP100,
                &mut size,
                &mut count,
                std::ptr::null_mut(),
            )
        };
        if rc != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }
        // items followed by their name strings; u64 storage keeps the alignment
        let mut buf = vec![0u64; (size as usize).div_ceil(8) + 1];
        // SAFETY: buffer of at least `size` bytes.
        let rc = unsafe {
            PdhGetFormattedCounterArrayW(
                c.0,
                PDH_FMT_DOUBLE | NOCAP100,
                &mut size,
                &mut count,
                buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
            )
        };
        if rc != 0 {
            return Vec::new();
        }
        // SAFETY: PDH wrote `count` items at the start of the buffer.
        let items = unsafe {
            std::slice::from_raw_parts(
                buf.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
                count as usize,
            )
        };
        items
            .iter()
            .filter(|it| any_status || it.FmtValue.CStatus <= 1) // VALID_DATA or NEW_DATA
            .map(|it| {
                // SAFETY: szName points into `buf`; doubleValue is the requested format.
                let name = unsafe { pwstr(it.szName) };
                (name, unsafe { it.FmtValue.Anonymous.doubleValue })
            })
            .collect()
    }

    /// A single-instance counter.
    pub fn value(&self, c: Counter) -> Option<f64> {
        self.values(c).into_iter().next().map(|(_, v)| v)
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: opened in new(); closes its counters too.
        unsafe { PdhCloseQuery(self.q) };
    }
}

/// # Safety
/// `p` must be null or a NUL-terminated UTF-16 string.
unsafe fn pwstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    // SAFETY: caller guarantees termination.
    unsafe {
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }
}
