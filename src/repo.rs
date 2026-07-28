//! Turning a working directory into a repository identity.
//!
//! AI agents record where they ran as an absolute path. A path is the wrong
//! thing to send to a fleet backend: it carries the operator's username and
//! their private directory layout, and it does not join across devices —
//! two engineers with the same repository checked out to different places look
//! like two unrelated projects.
//!
//! A git remote solves both problems. `holistic-ai/hai-agents` is stable
//! across every device that has the repository, so the backend can correlate
//! one codebase across a fleet, and it names the *owner* — the fact governance
//! actually turns on, which is whether an agent is working in company code,
//! personal code, or code under no version control at all.
//!
//! # What leaves the device
//!
//! `host`, `owner` and repository `name`, read from `.git/config`.
//!
//! # Deliberately not emitted
//!
//! Absolute paths, the operator's home directory or username, branch names,
//! remote URL paths beyond `owner/name`, credentials embedded in a remote URL,
//! and any remote other than `origin`.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// How far up from a working directory to look for the enclosing repository.
/// Deep enough for a monorepo package, bounded so a pathological path cannot
/// walk the whole filesystem.
const MAX_WALK_UP: usize = 24;

/// A `.git/config` past this size is not one we are willing to read; the real
/// ones are a few hundred bytes.
const MAX_GIT_CONFIG_BYTES: u64 = 1024 * 1024;

/// A repository, identified the way a fleet backend can join on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Identity {
    /// Remote host, e.g. `github.com`. `None` when the repository has no
    /// `origin`.
    pub host: Option<String>,
    /// Owning user or organisation. `None` when there is no `origin`.
    pub owner: Option<String>,
    /// Repository name — the directory basename when there is no `origin`.
    pub name: String,
    /// `owner/name` when known, otherwise bare `name`. The join key.
    pub slug: String,
    /// Whether an `origin` remote was found. A false here means the agent was
    /// working in code that is not pushed anywhere, which is its own finding.
    pub versioned: bool,
}

impl Identity {
    /// A repository with no discoverable remote: named, but not locatable.
    fn unversioned(name: &str) -> Self {
        Self {
            host: None,
            owner: None,
            name: name.to_string(),
            slug: name.to_string(),
            versioned: false,
        }
    }
}

/// What a recorded working directory turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// A repository, or at least a directory distinguishable from home.
    Project(Identity),
    /// The session ran at or above the home directory. Worth counting — an
    /// agent with the whole home directory in scope is a real observation —
    /// but it is not a project and must not be named as one.
    Home,
}

/// Resolve a recorded working directory to a repository identity.
///
/// Walks up to the enclosing repository and reads its `origin`. Returns `None`
/// for a path that does not exist, which is the normal case for a project the
/// operator has since deleted.
pub fn resolve(cwd: &Path, home: &Path) -> Option<Scope> {
    if !cwd.is_dir() {
        return None;
    }
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());

    // At or above home is not a project. Checked before the walk, because a
    // dotfiles repository puts `.git` directly in the home directory and would
    // otherwise claim every session on the machine as one repository.
    if cwd == home || home.starts_with(&cwd) {
        return Some(Scope::Home);
    }

    let root = repo_root(&cwd, &home);
    let Some(root) = root else {
        // Outside any repository: still a distinct place, named by its
        // basename so the tree can show it without disclosing the path.
        return Some(Scope::Project(Identity::unversioned(&basename(&cwd))));
    };

    let name = basename(&root);
    let identity = read_origin(&root)
        .as_deref()
        .and_then(parse_remote)
        .map(|(host, owner, repo)| Identity {
            slug: format!("{owner}/{repo}"),
            host: Some(host),
            owner: Some(owner),
            name: repo,
            versioned: true,
        })
        .unwrap_or_else(|| Identity::unversioned(&name));

    Some(Scope::Project(identity))
}

/// The nearest ancestor holding a `.git`, stopping before home.
///
/// Stopping at home is what keeps a dotfiles repository from swallowing the
/// machine. A directory with its own `.git` still wins, so a real repository
/// inside home resolves normally.
fn repo_root(cwd: &Path, home: &Path) -> Option<PathBuf> {
    let mut cur = cwd;
    for _ in 0..MAX_WALK_UP {
        if cur == home {
            return None;
        }
        if cur.join(".git").exists() {
            return Some(cur.to_path_buf());
        }
        cur = cur.parent()?;
    }
    None
}

/// Read `origin` from a repository's config without spawning git.
///
/// `git remote get-url` would be authoritative but costs a process per
/// repository, and a scan may resolve dozens. Reading the file is a few
/// microseconds each and cannot hang.
fn read_origin(root: &Path) -> Option<String> {
    let git = root.join(".git");
    // A worktree or submodule has `.git` as a file pointing elsewhere. Those
    // are followed by the `gitdir:` indirection, which we do not chase — the
    // enclosing repository is reported instead.
    let config = if git.is_dir() {
        git.join("config")
    } else {
        return None;
    };
    let readable = std::fs::metadata(&config)
        .map(|m| m.is_file() && m.len() <= MAX_GIT_CONFIG_BYTES)
        .unwrap_or(false);
    if !readable {
        return None;
    }
    parse_git_config(&std::fs::read_to_string(&config).ok()?)
}

/// Extract `[remote "origin"] url` from git config text.
///
/// Deliberately a hand parse rather than an ini crate: git config allows
/// tabs, arbitrary indentation and subsection quoting, and this is the only
/// key we ever want.
pub fn parse_git_config(text: &str) -> Option<String> {
    let mut in_origin = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            // Section headers vary in spacing: `[remote "origin"]` and
            // `[remote"origin"]` are both accepted by git.
            let inner = line.trim_start_matches('[').trim_end_matches(']');
            let normalised: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
            in_origin = normalised == "remote\"origin\"";
            continue;
        }
        if !in_origin {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("url") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Split a remote URL into host, owner and repository name.
///
/// Handles the three forms in the wild: scp-like (`git@host:owner/repo.git`),
/// URL with a scheme (`https://host/owner/repo`), and a bare `host/owner/repo`.
/// Any embedded credentials are dropped rather than parsed.
pub fn parse_remote(url: &str) -> Option<(String, String, String)> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    // Strip a scheme, then userinfo. `https://user:token@host/...` must never
    // carry the token forward.
    let after_scheme = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };

    // scp-like syntax has no scheme and separates host from path with a colon.
    let (host_part, path) = if url.contains("://") {
        after_scheme.split_once('/')?
    } else if let Some((host, path)) = after_scheme.split_once(':') {
        // Guard against a bare `host:port/path` being read as scp syntax.
        if path.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            let (_, rest) = path.split_once('/')?;
            (host, rest)
        } else {
            (host, path)
        }
    } else {
        after_scheme.split_once('/')?
    };

    let host = host_part.rsplit('@').next()?; // drop userinfo
    let host = host.split(':').next()?; // drop port
    if host.is_empty() {
        return None;
    }

    // Owner is the second-to-last path segment and repo the last, so deeper
    // paths (self-hosted GitLab subgroups, Azure DevOps) still yield the
    // immediate owner rather than a top-level group.
    let segments: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty() && *s != "_git")
        .collect();
    if segments.len() < 2 {
        return None;
    }
    let repo = segments.last()?.trim_end_matches(".git");
    let owner = segments[segments.len() - 2];
    if repo.is_empty() || owner.is_empty() {
        return None;
    }

    // Lowercased, because the same repository reached two ways must be one
    // row. This machine has `git@github.com:Kleyt0n/graphnetz` alongside
    // `https://github.com/kleyt0n/jaxfolio` — one owner that would otherwise
    // split into two, and across a fleet the same repo would fragment by
    // however each engineer happened to clone it.
    Some((
        host.to_ascii_lowercase(),
        owner.to_ascii_lowercase(),
        repo.to_ascii_lowercase(),
    ))
}

/// Last path component, or `unknown` for a path that has none.
fn basename(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scp_syntax_is_the_common_github_form() {
        let (host, owner, repo) =
            parse_remote("git@github.com:holistic-ai/hai-agents.git").unwrap();
        assert_eq!(
            (host.as_str(), owner.as_str(), repo.as_str()),
            ("github.com", "holistic-ai", "hai-agents")
        );
    }

    #[test]
    fn https_urls_resolve_the_same_way() {
        let (host, owner, repo) =
            parse_remote("https://github.com/holistic-ai/hai-agents.git").unwrap();
        assert_eq!(host, "github.com");
        assert_eq!(owner, "holistic-ai");
        assert_eq!(repo, "hai-agents");
    }

    #[test]
    fn a_missing_dot_git_suffix_is_fine() {
        let (_, owner, repo) = parse_remote("https://github.com/holistic-ai/hai-neo").unwrap();
        assert_eq!(owner, "holistic-ai");
        assert_eq!(repo, "hai-neo");
    }

    #[test]
    fn credentials_in_the_url_never_survive_parsing() {
        // A token pasted into a remote URL is a credential. It must not reach
        // the host field, and nothing else in the tuple can carry it either.
        let (host, owner, repo) =
            parse_remote("https://kcosta:ghp_secrettoken@github.com/holistic-ai/hai-neo").unwrap();
        assert_eq!(host, "github.com");
        for part in [&host, &owner, &repo] {
            assert!(!part.contains("ghp_"), "credential leaked into {part}");
            assert!(!part.contains("kcosta"), "username leaked into {part}");
        }
    }

    #[test]
    fn ssh_scheme_with_a_port_is_not_mistaken_for_scp_syntax() {
        let (host, owner, repo) =
            parse_remote("ssh://git@git.internal:2222/team/service.git").unwrap();
        assert_eq!(host, "git.internal");
        assert_eq!(owner, "team");
        assert_eq!(repo, "service");
    }

    #[test]
    fn a_subgroup_path_reports_the_immediate_owner() {
        // Self-hosted GitLab nests groups. The owner a reader cares about is
        // the one directly holding the repository.
        let (host, owner, repo) =
            parse_remote("https://gitlab.internal/platform/data/warehouse.git").unwrap();
        assert_eq!(host, "gitlab.internal");
        assert_eq!(owner, "data");
        assert_eq!(repo, "warehouse");
    }

    #[test]
    fn hosts_are_lowercased_so_one_repo_is_one_row() {
        let (host, _, _) = parse_remote("git@GitHub.COM:holistic-ai/hai-neo.git").unwrap();
        assert_eq!(host, "github.com");
    }

    #[test]
    fn one_owner_cloned_two_ways_is_one_owner() {
        // Observed on a real machine: an ssh remote spelled `Kleyt0n` and an
        // https one spelled `kleyt0n` split a single person into two owners.
        let ssh = parse_remote("git@github.com:Kleyt0n/graphnetz.git").unwrap();
        let https = parse_remote("https://github.com/kleyt0n/jaxfolio.git").unwrap();
        assert_eq!(ssh.1, https.1, "same owner must produce the same key");
        assert_eq!(ssh.1, "kleyt0n");
    }

    #[test]
    fn a_url_with_no_owner_segment_is_rejected() {
        assert!(parse_remote("https://github.com/orphan.git").is_none());
        assert!(parse_remote("").is_none());
        assert!(parse_remote("not a url").is_none());
    }

    #[test]
    fn origin_is_read_out_of_a_real_git_config() {
        let text = r#"
[core]
	repositoryformatversion = 0
	bare = false
[remote "origin"]
	url = git@github.com:holistic-ai/hai-agents.git
	fetch = +refs/heads/*:refs/remotes/origin/*
[branch "main"]
	remote = origin
"#;
        assert_eq!(
            parse_git_config(text).as_deref(),
            Some("git@github.com:holistic-ai/hai-agents.git")
        );
    }

    #[test]
    fn a_non_origin_remote_is_ignored() {
        // Reporting `upstream` would attribute a fork's work to the project it
        // was forked from, which is the wrong owner.
        let text = r#"
[remote "upstream"]
	url = git@github.com:someone-else/original.git
[remote "origin"]
	url = git@github.com:holistic-ai/fork.git
"#;
        let url = parse_git_config(text).unwrap();
        assert!(
            url.contains("holistic-ai/fork"),
            "picked the wrong remote: {url}"
        );
    }

    #[test]
    fn a_config_with_only_other_remotes_yields_nothing() {
        let text = "[remote \"upstream\"]\n\turl = git@github.com:x/y.git\n";
        assert!(parse_git_config(text).is_none());
    }

    #[test]
    fn commented_out_urls_are_not_read() {
        let text = "[remote \"origin\"]\n\t# url = git@github.com:ghost/repo.git\n";
        assert!(parse_git_config(text).is_none());
    }

    #[test]
    fn unversioned_identity_still_has_a_usable_slug() {
        let id = Identity::unversioned("medusa");
        assert_eq!(id.slug, "medusa");
        assert!(!id.versioned);
        assert!(id.owner.is_none());
    }

    struct Tmp(PathBuf);
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmp(tag: &str) -> Tmp {
        let dir = std::env::temp_dir().join(format!("surface-repo-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }

    fn make_repo(at: &Path, origin: Option<&str>) {
        std::fs::create_dir_all(at.join(".git")).unwrap();
        let body = match origin {
            Some(url) => format!("[remote \"origin\"]\n\turl = {url}\n"),
            None => "[core]\n\tbare = false\n".to_string(),
        };
        std::fs::write(at.join(".git/config"), body).unwrap();
    }

    #[test]
    fn a_repo_resolves_to_its_remote_slug() {
        let t = tmp("slug");
        let home = t.0.join("home");
        let repo = home.join("repos/hai/hai-agents");
        std::fs::create_dir_all(&repo).unwrap();
        make_repo(&repo, Some("git@github.com:holistic-ai/hai-agents.git"));

        let Some(Scope::Project(id)) = resolve(&repo, &home) else {
            panic!("expected a project");
        };
        assert_eq!(id.slug, "holistic-ai/hai-agents");
        assert!(id.versioned);
    }

    #[test]
    fn a_subdirectory_resolves_to_the_enclosing_repo() {
        let t = tmp("subdir");
        let home = t.0.join("home");
        let repo = home.join("repos/mono");
        let pkg = repo.join("packages/api/src");
        std::fs::create_dir_all(&pkg).unwrap();
        make_repo(&repo, Some("https://github.com/acme/mono.git"));

        let Some(Scope::Project(id)) = resolve(&pkg, &home) else {
            panic!("expected a project");
        };
        assert_eq!(
            id.slug, "acme/mono",
            "a package inside a monorepo is still that repo"
        );
    }

    #[test]
    fn a_repo_without_a_remote_is_reported_as_unversioned() {
        let t = tmp("noremote");
        let home = t.0.join("home");
        let repo = home.join("repos/medusa");
        std::fs::create_dir_all(&repo).unwrap();
        make_repo(&repo, None);

        let Some(Scope::Project(id)) = resolve(&repo, &home) else {
            panic!("expected a project");
        };
        assert_eq!(id.slug, "medusa");
        assert!(!id.versioned);
        assert!(id.host.is_none());
    }

    #[test]
    fn a_dotfiles_repo_in_home_does_not_swallow_the_machine() {
        // The hazard this guard exists for: `.git` directly in home would
        // otherwise make every session on the device report one slug.
        let t = tmp("dotfiles");
        let home = t.0.join("home");
        let plain = home.join("Documents/notes");
        std::fs::create_dir_all(&plain).unwrap();
        make_repo(&home, Some("git@github.com:kcosta/dotfiles.git"));

        let Some(Scope::Project(id)) = resolve(&plain, &home) else {
            panic!("expected a project");
        };
        assert_eq!(
            id.slug, "notes",
            "resolved to the dotfiles repo instead of stopping at home"
        );
        assert!(!id.versioned);
    }

    #[test]
    fn home_itself_is_scope_home_not_a_project() {
        let t = tmp("home");
        let home = t.0.join("home");
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(resolve(&home, &home), Some(Scope::Home));
    }

    #[test]
    fn a_directory_above_home_is_also_scope_home() {
        let t = tmp("above");
        let home = t.0.join("home");
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(resolve(&t.0, &home), Some(Scope::Home));
    }

    #[test]
    fn a_deleted_project_directory_resolves_to_nothing() {
        let t = tmp("gone");
        let home = t.0.join("home");
        std::fs::create_dir_all(&home).unwrap();
        assert!(resolve(&home.join("repos/deleted"), &home).is_none());
    }

    #[test]
    fn no_resolution_ever_returns_an_absolute_path() {
        // The whole point of this module. Whatever comes out must be safe to
        // put on the wire.
        let t = tmp("nopaths");
        let home = t.0.join("home");
        let repo = home.join("repos/client-acme/secret-product");
        std::fs::create_dir_all(&repo).unwrap();
        make_repo(&repo, Some("git@github.com:acme/secret-product.git"));

        let Some(Scope::Project(id)) = resolve(&repo, &home) else {
            panic!("expected a project");
        };
        let rendered = serde_json::to_string(&id).unwrap();
        assert!(
            !rendered.contains("client-acme"),
            "leaked a path segment: {rendered}"
        );
        assert!(!rendered.contains(&home.to_string_lossy().to_string()));
        assert!(!rendered.contains("/repos/"), "leaked a path: {rendered}");
    }
}
