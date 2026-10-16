//! `nanofreedisk-setup-pack <stub.exe> <payload dir> <out.exe>`: append the files of
//! `<payload dir>` (plus the stub itself as `uninstall.exe`) to the setup stub.
//! Used by `release.yml`; see `payload` for the format.

use std::path::Path;
use std::process::ExitCode;

use nanofreedisk_setup::payload::{self, Entry};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [stub, dir, out] = args.as_slice() else {
        eprintln!("usage: nanofreedisk-setup-pack <stub.exe> <payload dir> <out.exe>");
        return ExitCode::from(2);
    };
    match run(Path::new(stub), Path::new(dir), Path::new(out)) {
        Ok(n) => {
            println!("packed {n} files into {out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(stub: &Path, dir: &Path, out: &Path) -> std::io::Result<usize> {
    let stub_bytes = std::fs::read(stub)?;
    // a stub that already carries a payload is cut back to the bare stub
    let stub_bytes = payload::stub_of(&stub_bytes).to_vec();
    let mut entries = payload::entries_from_dir(dir)?;
    entries.retain(|e| e.name != payload::UNINSTALLER);
    entries.push(Entry {
        name: payload::UNINSTALLER.into(),
        data: stub_bytes.clone(),
    });
    let file = payload::pack(&stub_bytes, &entries)?;
    std::fs::write(out, file)?;
    Ok(entries.len())
}
