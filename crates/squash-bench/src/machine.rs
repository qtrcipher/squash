//! Best-effort machine identification, recorded with every run so numbers
//! stay attributable (docs/05 §6: "machine info recorded"). No dependencies:
//! OS/arch/cores come from `std`; CPU model and RAM are probed via
//! `sysctl` (macOS/BSD) or `/proc` (Linux) and degrade to `None`.

use crate::model::MachineInfo;
use std::process::Command;

pub fn collect() -> MachineInfo {
    MachineInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cores: std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(0),
        cpu: cpu_model(),
        ram_bytes: ram_bytes(),
    }
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn cpu_model() -> Option<String> {
    if cfg!(target_os = "macos") {
        return command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"]);
    }
    if cfg!(target_os = "linux") {
        let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        return text
            .lines()
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == "model name"))
            .map(|(_, v)| v.trim().to_string());
    }
    // Windows and anything else: the environment variable is better than nothing.
    std::env::var("PROCESSOR_IDENTIFIER").ok()
}

fn ram_bytes() -> Option<u64> {
    if cfg!(target_os = "macos") {
        return command_stdout("sysctl", &["-n", "hw.memsize"])?
            .parse()
            .ok();
    }
    if cfg!(target_os = "linux") {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kib: u64 = text
            .lines()
            .find_map(|l| {
                let (k, v) = l.split_once(':')?;
                (k.trim() == "MemTotal").then(|| v.trim().trim_end_matches(" kB").to_string())
            })?
            .parse()
            .ok()?;
        return Some(kib * 1024);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_os_arch_and_cores() {
        let info = collect();
        assert_eq!(info.os, std::env::consts::OS);
        assert_eq!(info.arch, std::env::consts::ARCH);
        assert!(info.cores > 0);
    }
}
