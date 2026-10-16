//! Active monitors through `QueryDisplayConfig`: EDID friendly name, resolution,
//! precise refresh rate, connector, primary.

use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
    QueryDisplayConfig,
};

use super::from_wide;
use crate::Display;

fn connector(tech: i32) -> Option<&'static str> {
    Some(match tech {
        0 => "VGA",
        4 => "DVI",
        5 => "HDMI",
        6 => "LVDS",
        10 => "DisplayPort",
        11 => "eDP",
        15 => "Miracast",
        16 | 17 => "Indirect",
        i32::MIN => "Internal",
        _ => return None,
    })
}

pub fn native() -> Vec<Display> {
    let (mut np, mut nm) = (0u32, 0u32);
    // SAFETY: out-params only.
    if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) } != 0 {
        return Vec::new();
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
    // SAFETY: arrays sized by GetDisplayConfigBufferSizes.
    let rc = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut np,
            paths.as_mut_ptr(),
            &mut nm,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Vec::new();
    }
    paths.truncate(np as usize);
    modes.truncate(nm as usize);
    let mut out = Vec::new();
    for p in &paths {
        let t = &p.targetInfo;
        let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
        name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
        name.header.size = size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
        name.header.adapterId = t.adapterId;
        name.header.id = t.id;
        // SAFETY: header filled for this request type.
        let named = unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } == 0;
        // SAFETY: modeInfoIdx is the active member without virtual-mode support.
        let idx = unsafe { p.sourceInfo.Anonymous.modeInfoIdx } as usize;
        let (mut width, mut height, mut primary) = (0, 0, false);
        if let Some(m) = modes.get(idx) {
            if m.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
                // SAFETY: infoType says sourceMode is the active member.
                let s = unsafe { m.Anonymous.sourceMode };
                (width, height) = (s.width, s.height);
                primary = s.position.x == 0 && s.position.y == 0;
            }
        }
        let r = t.refreshRate;
        let refresh =
            (r.Denominator > 0).then(|| f64::from(r.Numerator) / f64::from(r.Denominator));
        let friendly = if named {
            from_wide(&name.monitorFriendlyDeviceName)
        } else {
            String::new()
        };
        out.push(Display {
            name: (!friendly.is_empty()).then_some(friendly),
            width,
            height,
            refresh_hz: refresh.map(|v| (v * 100.0).round() / 100.0),
            connector: connector(t.outputTechnology).map(str::to_owned),
            primary,
        });
    }
    out
}
