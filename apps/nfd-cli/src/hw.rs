//! `nfd hw` — hardware inventory (docs/HARDWARE_MONITOR.md).

use std::process::ExitCode;

use clap::Args;

#[derive(Args, Debug)]
pub struct HwArgs {
    /// Print the inventory as JSON (schema nanofreedisk.hwinfo.v1).
    #[arg(long)]
    json: bool,
}

pub fn hw(args: HwArgs) -> anyhow::Result<ExitCode> {
    let inv = nfd_hwinfo::collect();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&inv)?);
    } else {
        print!("{}", nfd_hwinfo::to_text(&inv));
    }
    Ok(ExitCode::SUCCESS)
}
