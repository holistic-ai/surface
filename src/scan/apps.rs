//! Installed application names.
//!
//! One of five channels [`super::tooling`] matches against, and the only one
//! that is not pure filesystem work. It exists because some AI tools ship as a
//! desktop app and leave nothing on `PATH` — Claude Desktop and LM Studio, for
//! instance.
//!
//! Names only, no versions: the caller matches against a table of names, and
//! reading versions costs a `plutil` exec per bundle on macOS for nothing.
//!
//! Failure is not an error. If the platform probe cannot run, the tooling scan
//! loses one of its five channels and keeps the other four, which is why every
//! path here returns an empty `Vec` rather than a `Result`.

/// Application names installed on this machine, or empty if they cannot be read.
pub fn enumerate() -> Vec<String> {
    platform_enumerate()
}

/// A directory walk rather than `system_profiler`, which takes 10-30s on a
/// normal Mac. Nothing is executed.
#[cfg(target_os = "macos")]
fn platform_enumerate() -> Vec<String> {
    let mut roots = vec![
        std::path::PathBuf::from("/Applications"),
        std::path::PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(std::path::PathBuf::from(home).join("Applications"));
    }

    roots.iter().flat_map(|root| app_bundles_in(root)).collect()
}

#[cfg(target_os = "macos")]
fn app_bundles_in(root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_suffix(".app").map(str::to_string)
        })
        .collect()
}

/// One PowerShell call over both registry views plus per-user installs. We
/// format the output ourselves so the parser stays a one-field-per-line split.
#[cfg(target_os = "windows")]
fn platform_enumerate() -> Vec<String> {
    const SCRIPT: &str = r#"
$paths = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
Get-ItemProperty $paths -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName } |
  ForEach-Object { $_.DisplayName }
"#;

    run(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", SCRIPT],
    )
    .map(|out| parse_lines(&out))
    .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn platform_enumerate() -> Vec<String> {
    if let Some(out) = run("dpkg-query", &["-W", "-f=${Package}\n"]) {
        let names = parse_lines(&out);
        if !names.is_empty() {
            return names;
        }
    }

    run("rpm", &["-qa", "--qf", "%{NAME}\n"])
        .map(|out| parse_lines(&out))
        .unwrap_or_default()
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_enumerate() -> Vec<String> {
    Vec::new()
}

/// Blocking `std::process::Command`, because this is the only exec `surface`
/// makes and wiring an async process crate in for it would be absurd.
///
/// There is no timeout: both probes are local registry or package-database
/// reads. A hung one would hang the scan, which is the tradeoff taken for not
/// carrying a runtime.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// One name per line, trimmed, blanks dropped.
#[cfg(any(target_os = "windows", target_os = "linux", test))]
fn parse_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lines_trims_and_drops_blanks() {
        let out = "Cursor\n\n  Claude  \r\nLM Studio\n";
        assert_eq!(parse_lines(out), ["Cursor", "Claude", "LM Studio"]);
    }

    #[test]
    fn parse_lines_of_nothing_is_empty_not_a_blank_entry() {
        assert!(parse_lines("").is_empty());
        assert!(parse_lines("\n\n  \n").is_empty());
    }

    #[test]
    fn enumerate_never_panics_on_this_machine() {
        // The contract is "empty rather than an error", on every platform.
        let _ = enumerate();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_missing_applications_directory_is_empty() {
        assert!(app_bundles_in(std::path::Path::new("/nonexistent-apps")).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn only_dot_app_bundles_are_returned_and_the_suffix_is_stripped() {
        let dir = std::env::temp_dir().join("surface-apps-bundles");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Cursor.app")).unwrap();
        std::fs::create_dir_all(dir.join("Utilities")).unwrap();
        std::fs::write(dir.join("README.txt"), b"x").unwrap();

        assert_eq!(app_bundles_in(&dir), ["Cursor"]);
    }
}
