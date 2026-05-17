/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_yaml::YamlDocPosition;

use super::{BasicUserGroupConfig, UserGroupConfig};
use crate::config::auth::UserConfig;

const USER_GROUP_TYPE: &str = "python-basic";

#[derive(Clone)]
pub(crate) struct PythonBasicUserGroupConfig {
    basic: BasicUserGroupConfig,
    pub(crate) script_file: PathBuf,
    pub(crate) unmanaged_user: Option<Arc<UserConfig>>,
    pub(crate) check_timeout: Duration,
    pub(crate) cache_user_count: NonZeroUsize,
    pub(crate) cache_expire_time: Duration,
}

impl PythonBasicUserGroupConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        PythonBasicUserGroupConfig {
            basic: BasicUserGroupConfig::new(position),
            script_file: PathBuf::new(),
            unmanaged_user: None,
            check_timeout: Duration::from_secs(4),
            cache_user_count: super::DEFAULT_CACHE_USER_COUNT,
            cache_expire_time: super::DEFAULT_CACHE_EXPIRE_TIME,
        }
    }

    pub(crate) fn parse(
        map: &yaml::Hash,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut config = Self::new(position);
        vey_yaml::foreach_kv(map, |k, v| config.set(k, v))?;
        config.check()?;
        Ok(config)
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.script_file.as_os_str().is_empty() {
            return Err(anyhow!("no script is set"));
        }

        self.basic.check()
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "script" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.basic.position.as_ref())?;
                self.script_file = vey_yaml::value::as_file_path(v, lookup_dir, false)
                    .context(format!("invalid file path value for key {k}"))?;
                Ok(())
            }
            "unmanaged_user" => {
                if let Yaml::Hash(map) = v {
                    let mut user = UserConfig::parse_yaml(map, self.basic.position.as_ref())?;
                    user.set_no_password();
                    self.unmanaged_user = Some(Arc::new(user));
                    Ok(())
                } else {
                    Err(anyhow!("invalid hash value for key {k}"))
                }
            }
            "check_timeout" => {
                self.check_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "cache_user_count" => {
                self.cache_user_count = vey_yaml::value::as_nonzero_usize(v)?;
                Ok(())
            }
            "cache_expire_time" => {
                self.cache_expire_time = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            _ => self.basic.set(k, v),
        }
    }
}

impl UserGroupConfig for PythonBasicUserGroupConfig {
    fn basic_config(&self) -> &BasicUserGroupConfig {
        &self.basic
    }

    fn r#type(&self) -> &'static str {
        USER_GROUP_TYPE
    }
}
