// CDDL HEADER START
//
// The contents of this file are subject to the terms of the
// Common Development and Distribution License (the "License").
// You may not use this file except in compliance with the License.
//
// You can obtain a copy of the license in the file LICENSE
// or at https://opensource.org/licenses/CDDL-1.0.
// See the License for the specific language governing permissions
// and limitations under the License.
//
// When distributing Covered Code, include this CDDL HEADER in each
// file and include the License file. If applicable, add the following
// below this CDDL HEADER, with the fields enclosed by brackets "[]"
// replaced with your own identifying information:
// Portions Copyright [yyyy] [name of copyright owner]
//
// CDDL HEADER END
//
// Copyright 2026 millie.moe. All rights reserved.
// Use is subject to license terms.
//! Runtime configuration: which Plane instance to talk to, which workspace, and
//! the token to talk with.
//!
//! Each setting is resolved independently, first hit wins:
//!
//! 1. An environment variable
//! 2. The TOML config file
//! 3. A built-in default (`base_url` only)
//!
//! A `.env` file is loaded into the environment before step 1, so anything you
//! write there behaves exactly like a real environment variable. `.env` is
//! looked for in the working directory and every parent, so `cargo run` from
//! anywhere in the repo finds the one at the root.
//!
//! ```text
//! # .env  — gitignored, this is where the token belongs
//! PLANE_API_KEY=plane_api_...
//! PLANE_WORKSPACE=eleboog-com
//! ```
//!
//! The config file is optional and lives at `$XDG_CONFIG_HOME/sailboat/config.toml`
//! (falling back to `~/.config/sailboat/config.toml`), or wherever `SAILBOAT_CONFIG`
//! points:
//!
//! ```toml
//! [api]
//! base_url = "https://api.plane.so"
//!
//! [workspace]
//! slug = "eleboog-com"
//! ```

use std::{env, fmt, fs, io, path::PathBuf};

use color_eyre::eyre::{WrapErr, eyre};
use serde::Deserialize;

/// Plane Cloud. Self-hosted instances use their own domain.
const DEFAULT_BASE_URL: &str = "https://api.plane.so";

const ENV_API_KEY: &str = "PLANE_API_KEY";
const ENV_WORKSPACE: &str = "PLANE_WORKSPACE";
const ENV_BASE_URL: &str = "PLANE_BASE_URL";
const ENV_CONFIG_PATH: &str = "SAILBOAT_CONFIG";

/// Fully resolved settings. Build one with [`Config::load`].
#[derive(Debug, Clone)]
pub struct Config {
    pub api: Api,
    pub workspace: Workspace,
}

#[derive(Clone)]
pub struct Api {
    /// Origin of the Plane instance, without a trailing slash.
    pub base_url: String,
    /// Personal access token, sent as the `X-API-Key` header.
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    /// Workspace slug as it appears in Plane's URLs, e.g. `eleboog-com` in
    /// `https://app.plane.so/eleboog-com/projects/`.
    pub slug: String,
}

impl Config {
    /// Load `.env`, then the config file, then the environment.
    pub fn load() -> color_eyre::Result<Self> {
        load_dotenv()?;

        let file = load_file()?;

        let base_url = from_env(ENV_BASE_URL)
            .or(file.api.base_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let api_key = from_env(ENV_API_KEY).or(file.api.api_key).ok_or_else(|| {
            eyre!(
                "no Plane API token found.\n\n\
                 Create one in Plane under Settings > Personal Access Tokens, then put it in a\n\
                 `.env` file at the root of the project:\n\n    \
                 {ENV_API_KEY}=plane_api_...\n"
            )
        })?;

        let slug = from_env(ENV_WORKSPACE).or(file.workspace.slug).ok_or_else(|| {
            eyre!(
                "no Plane workspace set.\n\n\
                 Add the workspace slug — the first path segment of your Plane URL —\n\
                 to your `.env` file:\n\n    \
                 {ENV_WORKSPACE}=my-workspace\n"
            )
        })?;

        Ok(Config {
            api: Api {
                base_url: base_url.trim_end().trim_end_matches('/').to_string(),
                api_key,
            },
            workspace: Workspace { slug },
        })
    }

    /// Prefix shared by every workspace-scoped endpoint, e.g.
    /// `https://api.plane.so/api/v1/workspaces/eleboog-com`.
    pub fn workspace_url(&self) -> String {
        format!(
            "{base}/api/v1/workspaces/{slug}",
            base = self.api.base_url,
            slug = self.workspace.slug,
        )
    }
}

/// Hand-written so a stray `{config:?}` — or a color-eyre panic report — can't
/// spill the token into a log.
impl fmt::Debug for Api {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Api")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

/// Pull `.env` into the process environment. A missing file is fine; a malformed
/// one is not, since silently ignoring it looks identical to a typo'd key.
fn load_dotenv() -> color_eyre::Result<()> {
    match dotenvy::dotenv() {
        Ok(_) => Ok(()),
        Err(error) if error.not_found() => Ok(()),
        Err(error) => Err(error).wrap_err("could not read .env"),
    }
}

/// Reads an environment variable, treating blank as unset so an empty line in
/// `.env` falls through to the next source instead of yielding "".
fn from_env(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}

fn load_file() -> color_eyre::Result<FileConfig> {
    // An explicit SAILBOAT_CONFIG must exist; the default path need not.
    let (path, required) = match from_env(ENV_CONFIG_PATH) {
        Some(path) => (PathBuf::from(path), true),
        None => (default_config_path()?, false),
    };

    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)
            .wrap_err_with(|| format!("{} is not a valid sailboat config", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound && required == false => {
            Ok(FileConfig::default())
        }
        Err(error) => Err(error).wrap_err_with(|| format!("could not read {}", path.display())),
    }
}

fn default_config_path() -> color_eyre::Result<PathBuf> {
    let base = match env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(
            env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .ok_or_else(|| {
                    eyre!("neither XDG_CONFIG_HOME nor HOME is set; point {ENV_CONFIG_PATH} at a config file")
                })?,
        )
        .join(".config"),
    };

    Ok(base.join("sailboat").join("config.toml"))
}

/// The config file's shape. Every field is optional — the file only overrides
/// defaults, and the environment overrides the file.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    api: FileApi,
    #[serde(default)]
    workspace: FileWorkspace,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileApi {
    base_url: Option<String>,
    /// Supported, but prefer `PLANE_API_KEY` in `.env`. If you do put the token
    /// here, `chmod 600` the file.
    api_key: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileWorkspace {
    slug: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_file_parses_with_sections_omitted() {
        let file: FileConfig = toml::from_str("").unwrap();
        assert!(file.api.base_url.is_none());
        assert!(file.workspace.slug.is_none());

        let file: FileConfig =
            toml::from_str("[workspace]\nslug = \"eleboog-com\"\n").unwrap();
        assert_eq!(file.workspace.slug.as_deref(), Some("eleboog-com"));
    }

    #[test]
    fn a_misspelled_key_is_an_error_rather_than_a_silent_no_op() {
        assert!(toml::from_str::<FileConfig>("[api]\nbaseurl = \"x\"\n").is_err());
    }

    #[test]
    fn debug_output_does_not_contain_the_token() {
        let config = Config {
            api: Api {
                base_url: DEFAULT_BASE_URL.to_string(),
                api_key: "plane_api_supersecret".to_string(),
            },
            workspace: Workspace { slug: "eleboog-com".to_string() },
        };
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("supersecret"), "{rendered}");
    }

    #[test]
    fn workspace_url_is_the_prefix_every_endpoint_hangs_off() {
        let config = Config {
            api: Api {
                base_url: "https://api.plane.so".to_string(),
                api_key: String::new(),
            },
            workspace: Workspace { slug: "eleboog-com".to_string() },
        };
        assert_eq!(
            config.workspace_url(),
            "https://api.plane.so/api/v1/workspaces/eleboog-com"
        );
    }
}
