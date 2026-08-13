//! CLI grammar: `squash c` (compress) / `squash x` (extract) — docs/01 §3.5.
//!
//! Parsing and validation only; dispatch to `squash_core::Engine` lives in
//! `run.rs`.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Squash — one command, one config, works identically everywhere.
#[derive(Debug, Parser)]
#[command(name = "squash", version, about, long_about = None)]
pub struct Cli {
    /// Machine-readable output (mirrors the GUI job model; docs/03 §4).
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Compress files/folders into an archive.
    #[command(visible_alias = "c")]
    Compress {
        /// Files and folders to compress.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// Output archive path (default: next to the first input).
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Archive format: zip | 7z | tar.gz | tar.zst | gz | xz | zst.
        /// Single-file codecs (gz/xz/zst) take files only, one output each.
        #[arg(short, long)]
        format: Option<String>,

        /// Compression preset.
        #[arg(short, long, value_enum, default_value_t = PresetArg::Balanced)]
        preset: PresetArg,
    },

    /// Extract an archive.
    #[command(visible_alias = "x")]
    Extract {
        /// Archive(s) to extract.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// Destination directory (default: same folder as the archive).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// CLI mirror of `squash_core::presets::Preset` — exactly three, no flag jungle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PresetArg {
    Fast,
    Balanced,
    Max,
}

impl From<PresetArg> for squash_core::presets::Preset {
    fn from(arg: PresetArg) -> Self {
        match arg {
            PresetArg::Fast => Self::Fast,
            PresetArg::Balanced => Self::Balanced,
            PresetArg::Max => Self::Max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_parses_inputs_output_preset() {
        let cli = Cli::try_parse_from([
            "squash",
            "c",
            "src/",
            "docs/",
            "-o",
            "out.tar.zst",
            "--preset",
            "max",
            "--json",
        ])
        .unwrap();
        assert!(cli.json);
        match cli.command {
            Commands::Compress {
                inputs,
                output,
                preset,
                ..
            } => {
                assert_eq!(inputs.len(), 2);
                assert_eq!(output.unwrap(), PathBuf::from("out.tar.zst"));
                assert_eq!(preset, PresetArg::Max);
            }
            _ => panic!("expected compress"),
        }
    }

    #[test]
    fn extract_alias_and_defaults() {
        let cli = Cli::try_parse_from(["squash", "x", "backup.zip"]).unwrap();
        assert!(!cli.json);
        match cli.command {
            Commands::Extract { inputs, output } => {
                assert_eq!(inputs, vec![PathBuf::from("backup.zip")]);
                assert!(output.is_none());
            }
            _ => panic!("expected extract"),
        }
    }

    #[test]
    fn missing_inputs_is_usage_error() {
        assert!(Cli::try_parse_from(["squash", "compress"]).is_err());
        assert!(Cli::try_parse_from(["squash"]).is_err());
    }

    #[test]
    fn preset_maps_to_core() {
        let core: squash_core::presets::Preset = PresetArg::Fast.into();
        assert_eq!(core, squash_core::presets::Preset::Fast);
    }
}
