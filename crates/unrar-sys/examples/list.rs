//! Dev/debug helper: `cargo run -p unrar-sys --example list <archive.rar>`
//! lists entries and exercises the extraction pass, printing shim codes.

use unrar_sys::RarArchive;

fn main() {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: list <rar>"));
    let mode = std::env::args().nth(2).unwrap_or_default();

    let mut arc = match RarArchive::open_list(&path) {
        Ok(a) => a,
        Err(e) => {
            println!("open_list failed: {e} (code {})", e.0);
            return;
        }
    };
    loop {
        match arc.next_entry() {
            Ok(Some(e)) => {
                println!(
                    "entry: {:?} size={} dir={} enc={}",
                    e.name, e.size, e.is_dir, e.is_encrypted
                );
                // DLL contract: RARProcessFile(RAR_SKIP) between headers.
                if let Err(err) = arc.skip_current() {
                    println!("skip failed: {err} (code {})", err.0);
                    break;
                }
            }
            Ok(None) => {
                println!("end of archive");
                break;
            }
            Err(e) => {
                println!("next failed: {e} (code {})", e.0);
                break;
            }
        }
    }
    drop(arc);

    if mode == "x" {
        let mut arc = RarArchive::open_extract(&path).unwrap();
        while let Ok(Some(e)) = arc.next_entry() {
            let mut total = 0usize;
            let rc = arc.extract_current(&mut |chunk| {
                total += chunk.len();
                true
            });
            println!("extract {:?} -> {:?} ({} bytes)", e.name, rc, total);
        }
    }
}
