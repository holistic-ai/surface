//! Browser and profile discovery.
//!
//! Shared by the two web collectors. Everything here is directory probing —
//! no browser is launched, no database is opened, nothing is written.
//!
//! The counterpart to "what we found" is [`blind_spots`]: browsers that are
//! installed but whose history we cannot read. A dashboard that shows two
//! browsers scanned and says nothing about the third is quietly wrong, and on
//! macOS the third is usually Safari.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// Chrome, Edge, Brave, Arc, Vivaldi, Opera, Chromium — all share the
    /// `History` SQLite schema.
    Chromium,
    /// Firefox and forks — `places.sqlite`.
    Firefox,
    /// Present but unreadable without extra privileges.
    Restricted,
}

#[derive(Debug, Clone, Copy)]
pub struct Browser {
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    /// Path of the profile root, relative to the user's home directory.
    macos_root: &'static str,
    windows_root: &'static str,
    linux_root: &'static str,
}

/// Browsers we know how to find. `""` means the browser does not exist on
/// that platform.
pub const BROWSERS: &[Browser] = &[
    Browser {
        id: "chrome",
        name: "Google Chrome",
        family: Family::Chromium,
        macos_root: "Library/Application Support/Google/Chrome",
        windows_root: "AppData/Local/Google/Chrome/User Data",
        linux_root: ".config/google-chrome",
    },
    Browser {
        id: "edge",
        name: "Microsoft Edge",
        family: Family::Chromium,
        macos_root: "Library/Application Support/Microsoft Edge",
        windows_root: "AppData/Local/Microsoft/Edge/User Data",
        linux_root: ".config/microsoft-edge",
    },
    Browser {
        id: "brave",
        name: "Brave",
        family: Family::Chromium,
        macos_root: "Library/Application Support/BraveSoftware/Brave-Browser",
        windows_root: "AppData/Local/BraveSoftware/Brave-Browser/User Data",
        linux_root: ".config/BraveSoftware/Brave-Browser",
    },
    Browser {
        id: "arc",
        name: "Arc",
        family: Family::Chromium,
        macos_root: "Library/Application Support/Arc/User Data",
        windows_root: "AppData/Local/Packages/TheBrowserCompany.Arc/LocalCache/Local/Arc/User Data",
        linux_root: "",
    },
    Browser {
        id: "vivaldi",
        name: "Vivaldi",
        family: Family::Chromium,
        macos_root: "Library/Application Support/Vivaldi",
        windows_root: "AppData/Local/Vivaldi/User Data",
        linux_root: ".config/vivaldi",
    },
    Browser {
        id: "opera",
        name: "Opera",
        family: Family::Chromium,
        macos_root: "Library/Application Support/com.operasoftware.Opera",
        windows_root: "AppData/Roaming/Opera Software/Opera Stable",
        linux_root: ".config/opera",
    },
    Browser {
        id: "chromium",
        name: "Chromium",
        family: Family::Chromium,
        macos_root: "Library/Application Support/Chromium",
        windows_root: "AppData/Local/Chromium/User Data",
        linux_root: ".config/chromium",
    },
    Browser {
        id: "firefox",
        name: "Firefox",
        family: Family::Firefox,
        macos_root: "Library/Application Support/Firefox/Profiles",
        windows_root: "AppData/Roaming/Mozilla/Firefox/Profiles",
        linux_root: ".mozilla/firefox",
    },
    Browser {
        id: "zen",
        name: "Zen Browser",
        family: Family::Firefox,
        macos_root: "Library/Application Support/zen/Profiles",
        windows_root: "AppData/Roaming/zen/Profiles",
        linux_root: ".zen",
    },
    Browser {
        id: "safari",
        name: "Safari",
        // TCC-protected: reading History.db needs Full Disk Access, granted
        // by an MDM PPPC profile. Reported as a blind spot instead.
        family: Family::Restricted,
        macos_root: "Library/Safari",
        windows_root: "",
        linux_root: "",
    },
];

impl Browser {
    /// Profile root for the current platform, or `None` if not applicable.
    pub fn root(&self, home: &Path) -> Option<PathBuf> {
        let relative = if cfg!(target_os = "macos") {
            self.macos_root
        } else if cfg!(target_os = "windows") {
            self.windows_root
        } else {
            self.linux_root
        };

        (!relative.is_empty()).then(|| home.join(relative))
    }

    /// Is this browser actually installed, as opposed to leaving a stub
    /// directory behind?
    ///
    /// The profile root existing is not enough: unrelated software drops
    /// `NativeMessagingHosts` into `Application Support/<Browser>/` for
    /// browsers that were never installed, and treating those as installs
    /// fills the report with phantom coverage gaps. A Chromium browser that
    /// has actually run writes `Local State`.
    pub fn is_installed(&self, home: &Path) -> bool {
        let Some(root) = self.root(home) else {
            return false;
        };
        if !root.is_dir() {
            return false;
        }

        match self.family {
            Family::Chromium => {
                root.join("Local State").is_file()
                    || root.join("Default").is_dir()
                    || has_numbered_profile(&root)
            }
            // A profiles directory with anything in it.
            Family::Firefox => std::fs::read_dir(&root)
                .map(|mut e| e.any(|entry| entry.is_ok_and(|e| e.path().is_dir())))
                .unwrap_or(false),
            // Safari's support directory only exists once it has run.
            Family::Restricted => true,
        }
    }
}

fn has_numbered_profile(root: &Path) -> bool {
    std::fs::read_dir(root)
        .map(|mut entries| {
            entries.any(|entry| {
                entry.is_ok_and(|e| {
                    e.path().is_dir() && e.file_name().to_string_lossy().starts_with("Profile ")
                })
            })
        })
        .unwrap_or(false)
}

/// One profile directory with a history database in it.
#[derive(Debug, Clone)]
pub struct Profile {
    pub browser_id: &'static str,
    pub family: Family,
    /// Profile directory name, e.g. `Default`, `Profile 1`, `xy12ab.default`.
    pub profile: String,
    pub history_db: PathBuf,
}

/// Every readable history database on this machine.
pub fn discover_profiles() -> Vec<Profile> {
    let Some(home) = crate::paths::home() else {
        return Vec::new();
    };
    discover_profiles_in(&home)
}

/// Testable form of [`discover_profiles`], rooted at an arbitrary directory.
pub fn discover_profiles_in(home: &Path) -> Vec<Profile> {
    let mut profiles = Vec::new();

    for browser in BROWSERS {
        let Some(root) = browser.root(home) else {
            continue;
        };
        match browser.family {
            Family::Chromium => collect_chromium(browser, &root, &mut profiles),
            Family::Firefox => collect_firefox(browser, &root, &mut profiles),
            Family::Restricted => {}
        }
    }

    profiles.sort_by(|a, b| (a.browser_id, &a.profile).cmp(&(b.browser_id, &b.profile)));
    profiles
}

/// Chromium keeps `Default` plus `Profile 1`, `Profile 2`, … side by side.
fn collect_chromium(browser: &Browser, root: &Path, out: &mut Vec<Profile>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_profile = name == "Default" || name.starts_with("Profile ");
        if !is_profile {
            continue;
        }
        let db = entry.path().join("History");
        if db.is_file() {
            out.push(Profile {
                browser_id: browser.id,
                family: browser.family,
                profile: name,
                history_db: db,
            });
        }
    }
}

/// Firefox profile directories are named `<random>.<name>`.
fn collect_firefox(browser: &Browser, root: &Path, out: &mut Vec<Profile>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let db = entry.path().join("places.sqlite");
        if db.is_file() {
            out.push(Profile {
                browser_id: browser.id,
                family: browser.family,
                profile: entry.file_name().to_string_lossy().to_string(),
                history_db: db,
            });
        }
    }
}

/// Browsers present on the device whose history we did not read, and why.
///
/// Reported alongside the results so a gap in coverage is visible rather than
/// being mistaken for an absence of AI use.
pub fn blind_spots() -> Vec<BlindSpot> {
    let Some(home) = crate::paths::home() else {
        return Vec::new();
    };
    blind_spots_in(&home)
}

#[derive(Debug, Clone, Serialize)]
pub struct BlindSpot {
    pub browser: &'static str,
    /// Display name, so a coverage gap reads as "Safari" and not "safari".
    pub name: &'static str,
    pub reason: &'static str,
}

pub fn blind_spots_in(home: &Path) -> Vec<BlindSpot> {
    let mut spots = Vec::new();

    for browser in BROWSERS {
        if !browser.is_installed(home) {
            continue;
        }

        match browser.family {
            // Installed, but reading it needs Full Disk Access.
            Family::Restricted => spots.push(BlindSpot {
                browser: browser.id,
                name: browser.name,
                reason: crate::reason::INSUFFICIENT_PRIVILEGES,
            }),
            // Profile root exists but held no readable database — a fresh
            // install, or a profile layout we do not recognise.
            _ => {
                let found = discover_profiles_in(home)
                    .iter()
                    .any(|p| p.browser_id == browser.id);
                if !found {
                    spots.push(BlindSpot {
                        browser: browser.id,
                        name: browser.name,
                        reason: "no_history_database",
                    });
                }
            }
        }
    }

    spots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("surface-browser-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_profile(home: &Path, browser: &str, profile: &str, file: &str) {
        let root = BROWSERS
            .iter()
            .find(|b| b.id == browser)
            .unwrap()
            .root(home)
            .unwrap();
        let dir = root.join(profile);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file), b"fake db").unwrap();
    }

    #[test]
    fn browser_ids_are_unique() {
        let mut ids: Vec<_> = BROWSERS.iter().map(|b| b.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    #[test]
    fn every_browser_has_a_root_on_some_platform() {
        for b in BROWSERS {
            assert!(
                !(b.macos_root.is_empty() && b.windows_root.is_empty() && b.linux_root.is_empty()),
                "{} is unreachable on every platform",
                b.id
            );
        }
    }

    #[test]
    fn finds_chromium_default_and_numbered_profiles() {
        let home = temp_home("chromium");
        make_profile(&home, "chrome", "Default", "History");
        make_profile(&home, "chrome", "Profile 1", "History");
        make_profile(&home, "chrome", "Profile 2", "History");

        let profiles = discover_profiles_in(&home);

        assert_eq!(profiles.len(), 3);
        assert!(profiles.iter().all(|p| p.browser_id == "chrome"));
        assert!(profiles.iter().all(|p| p.family == Family::Chromium));
        let names: Vec<_> = profiles.iter().map(|p| p.profile.as_str()).collect();
        assert_eq!(names, ["Default", "Profile 1", "Profile 2"]);
    }

    #[test]
    fn ignores_chromium_directories_that_are_not_profiles() {
        let home = temp_home("notprofiles");
        make_profile(&home, "chrome", "Default", "History");
        make_profile(&home, "chrome", "ShaderCache", "History");
        make_profile(&home, "chrome", "GrShaderCache", "History");

        let profiles = discover_profiles_in(&home);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].profile, "Default");
    }

    #[test]
    fn skips_profile_directories_without_a_database() {
        let home = temp_home("nodb");
        let root = BROWSERS
            .iter()
            .find(|b| b.id == "chrome")
            .unwrap()
            .root(&home)
            .unwrap();
        std::fs::create_dir_all(root.join("Default")).unwrap();

        assert!(discover_profiles_in(&home).is_empty());
    }

    #[test]
    fn finds_firefox_profiles_by_places_database() {
        let home = temp_home("firefox");
        make_profile(&home, "firefox", "xy12ab.default-release", "places.sqlite");

        let profiles = discover_profiles_in(&home);

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].browser_id, "firefox");
        assert_eq!(profiles[0].family, Family::Firefox);
        assert_eq!(profiles[0].profile, "xy12ab.default-release");
    }

    #[test]
    fn finds_several_browsers_at_once() {
        let home = temp_home("multi");
        make_profile(&home, "chrome", "Default", "History");
        make_profile(&home, "brave", "Default", "History");
        make_profile(&home, "firefox", "abc.default", "places.sqlite");

        let ids: Vec<_> = discover_profiles_in(&home)
            .iter()
            .map(|p| p.browser_id)
            .collect();

        assert_eq!(ids, ["brave", "chrome", "firefox"]);
    }

    #[test]
    fn no_browsers_installed_is_empty_not_an_error() {
        let home = temp_home("empty");
        assert!(discover_profiles_in(&home).is_empty());
        assert!(blind_spots_in(&home).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn safari_is_reported_as_a_blind_spot_when_present() {
        let home = temp_home("safari");
        std::fs::create_dir_all(home.join("Library/Safari")).unwrap();

        let spots = blind_spots_in(&home);

        assert_eq!(spots.len(), 1);
        assert_eq!(spots[0].browser, "safari");
        assert_eq!(spots[0].reason, crate::reason::INSUFFICIENT_PRIVILEGES);
    }

    #[test]
    fn a_stub_directory_is_not_an_installed_browser() {
        // Password managers and container tools drop NativeMessagingHosts into
        // Application Support for browsers that were never installed. Counting
        // those as installs fills the report with phantom coverage gaps —
        // observed on a real Mac with six such stubs.
        let home = temp_home("stub");
        let chrome = BROWSERS.iter().find(|b| b.id == "chrome").unwrap();
        std::fs::create_dir_all(chrome.root(&home).unwrap().join("NativeMessagingHosts")).unwrap();

        assert!(!chrome.is_installed(&home));
        assert!(blind_spots_in(&home).is_empty());
    }

    #[test]
    fn a_chromium_browser_that_has_run_is_installed() {
        let home = temp_home("realinstall");
        let chrome = BROWSERS.iter().find(|b| b.id == "chrome").unwrap();
        let root = chrome.root(&home).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Local State"), b"{}").unwrap();

        assert!(chrome.is_installed(&home));
    }

    #[test]
    fn a_profile_directory_alone_counts_as_installed() {
        let home = temp_home("profileonly");
        make_profile(&home, "chrome", "Default", "History");
        let chrome = BROWSERS.iter().find(|b| b.id == "chrome").unwrap();

        assert!(chrome.is_installed(&home));
    }

    #[test]
    fn a_real_install_with_no_database_is_a_blind_spot() {
        // Genuinely installed — it has run and written `Local State` — but no
        // profile has a history database yet. That is a coverage gap worth
        // reporting, unlike a bare stub directory.
        let home = temp_home("blind");
        let root = BROWSERS
            .iter()
            .find(|b| b.id == "chrome")
            .unwrap()
            .root(&home)
            .unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Local State"), b"{}").unwrap();

        let spots = blind_spots_in(&home);

        assert!(spots
            .iter()
            .any(|s| s.browser == "chrome" && s.reason == "no_history_database"));
    }

    #[test]
    fn a_browser_with_a_database_is_not_a_blind_spot() {
        let home = temp_home("notblind");
        make_profile(&home, "chrome", "Default", "History");

        assert!(!blind_spots_in(&home).iter().any(|s| s.browser == "chrome"));
    }
}
