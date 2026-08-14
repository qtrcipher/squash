//! OS "open with" entry points (docs/03 F6): turning the paths the OS hands
//! us — process argv on Windows/Linux, `RunEvent::Opened` URLs on macOS,
//! second-instance argv via the single-instance plugin — into plain file
//! paths for the frontend to route (archives → S3, everything else → S2,
//! via the existing `classify_paths` command).
//!
//! Both parsers are pure and unit-tested; the wiring lives in `lib.rs`.

use std::path::Path;
#[cfg(target_os = "macos")]
use tauri::Url;

/// Extract candidate file paths from process argv.
///
/// `args` is the raw argv (including argv\[0\], the binary itself, which is
/// skipped); `cwd` resolves relative paths — the single-instance callback
/// reports the *second* instance's working directory, which is where a
/// relative path argument makes sense. Anything starting with `-` is
/// dropped: the GUI takes no options, and this also filters the `-psn_…`
/// process-serial argument Finder adds on macOS cold starts.
pub fn paths_from_argv(args: &[String], cwd: &Path) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(|arg| {
            let path = Path::new(arg);
            if path.is_absolute() {
                arg.clone()
            } else {
                cwd.join(path).to_string_lossy().into_owned()
            }
        })
        .collect()
}

/// Extract file paths from `RunEvent::Opened` URLs (macOS: double-click on
/// an associated archive, or a drop onto the dock icon — F5/F6 treat them
/// identically). Non-file URLs are dropped. macOS-only: `RunEvent::Opened`
/// does not exist in Tauri on other platforms.
#[cfg(target_os = "macos")]
pub fn paths_from_urls(urls: &[Url]) -> Vec<String> {
    urls.iter()
        .filter_map(|url| url.to_file_path().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_skips_binary_name_and_flags() {
        let args = vec![
            "squash".to_string(),
            "-psn_0_123456".to_string(),
            "/tmp/photos.zip".to_string(),
            "--some-flag".to_string(),
            "/data/report.pdf".to_string(),
        ];
        assert_eq!(
            paths_from_argv(&args, Path::new("/unused")),
            vec!["/tmp/photos.zip", "/data/report.pdf"]
        );
    }

    #[test]
    fn argv_without_file_arguments_yields_nothing() {
        let args = vec!["squash".to_string()];
        assert!(paths_from_argv(&args, Path::new("/unused")).is_empty());
        let args = vec!["squash".to_string(), "-psn_0_1".to_string()];
        assert!(paths_from_argv(&args, Path::new("/unused")).is_empty());
    }

    #[test]
    fn argv_resolves_relative_paths_against_cwd() {
        let args = vec!["squash".to_string(), "downloads/a.zip".to_string()];
        let cwd = Path::new("/home/user");
        // Compute the expectation the same way the production code does, so
        // the assertion checks the join behavior, not the platform's path
        // separators (`\` on Windows).
        let expected = cwd.join("downloads/a.zip").to_string_lossy().into_owned();
        assert_eq!(paths_from_argv(&args, cwd), vec![expected]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn urls_keep_file_paths_and_drop_other_schemes() {
        let urls = vec![
            Url::parse("file:///tmp/photos.zip").unwrap(),
            Url::parse("https://example.com/nope.zip").unwrap(),
            Url::parse("file:///Users/amr/%D9%85%D9%84%D9%81.tar.gz").unwrap(),
        ];
        let paths = paths_from_urls(&urls);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "/tmp/photos.zip");
        // Percent-decoded (Arabic filename, docs/03 §6: paths are data).
        assert!(paths[1].ends_with("ملف.tar.gz"));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn urls_empty_input_yields_nothing() {
        assert!(paths_from_urls(&[]).is_empty());
    }
}
