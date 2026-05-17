/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::hash_map::Entry;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ahash::AHashMap;
use anyhow::anyhow;
use arc_swap::ArcSwapOption;
use arcstr::ArcStr;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use vey_types::auth::{Password, UserAuthError};
use vey_types::metrics::{MetricTagMap, NodeName};

use super::BaseUserGroup;
use crate::auth::{User, UserContext, UserType};
use crate::config::auth::{PythonBasicUserGroupConfig, UserGroupConfig};

const FN_NAME_CHECK_PASSWORD: &str = "check_password";

const VAR_NAME_FILE: &str = "__file__";

pub(crate) struct PythonBasicUserGroup {
    base: BaseUserGroup<PythonBasicUserGroupConfig>,
    unmanaged_users: Mutex<AHashMap<ArcStr, Arc<User>>>,
}

impl PythonBasicUserGroup {
    pub(super) fn base(&self) -> &BaseUserGroup<PythonBasicUserGroupConfig> {
        &self.base
    }

    pub(super) fn clone_config(&self) -> PythonBasicUserGroupConfig {
        (*self.base.config).clone()
    }

    pub(super) async fn new_with_config(
        config: PythonBasicUserGroupConfig,
    ) -> anyhow::Result<Arc<Self>> {
        let base = BaseUserGroup::new_with_config(config).await?;
        Ok(Arc::new(PythonBasicUserGroup {
            base,
            unmanaged_users: Default::default(),
        }))
    }

    pub(super) fn reload(&self, config: PythonBasicUserGroupConfig) -> anyhow::Result<Arc<Self>> {
        let base = self.base.reload(config)?;
        Ok(Arc::new(PythonBasicUserGroup {
            base,
            unmanaged_users: Default::default(),
        }))
    }

    pub(super) async fn check_user_with_password(
        &self,
        username: &str,
        password: &Password,
        server_name: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Result<UserContext, UserAuthError> {
        match &self.base.config.unmanaged_user {
            Some(unmanaged_user_config) => {
                self.do_python_check(username, password).await?;

                if let Some((user, user_type)) = self.base.get_user(username) {
                    return Ok(UserContext::new(
                        Some(username.into()),
                        user,
                        user_type,
                        server_name,
                        server_extra_tags,
                    ));
                }

                let mut ht = self.unmanaged_users.lock().unwrap();
                match ht.entry(username.into()) {
                    Entry::Occupied(o) => {
                        let user = o.get().clone();
                        Ok(UserContext::new(
                            Some(username.into()),
                            user.clone(),
                            UserType::Unmanaged,
                            server_name,
                            server_extra_tags,
                        ))
                    }
                    Entry::Vacant(v) => {
                        let username = ArcStr::from(username);

                        let user = User::new_unmanaged(
                            &username,
                            self.base.config.basic_config().name(),
                            unmanaged_user_config,
                        )
                        .map_err(|_| UserAuthError::NoSuchUser)?;
                        let user = Arc::new(user);

                        v.insert(user.clone());

                        Ok(UserContext::new(
                            Some(username),
                            user.clone(),
                            UserType::Unmanaged,
                            server_name,
                            server_extra_tags,
                        ))
                    }
                }
            }
            None => {
                if let Some((user, user_type)) = self.base.get_user(username) {
                    self.do_python_check(username, password).await?;
                    Ok(UserContext::new(
                        Some(username.into()),
                        user,
                        user_type,
                        server_name,
                        server_extra_tags,
                    ))
                } else {
                    Err(UserAuthError::NoSuchUser)
                }
            }
        }
    }

    async fn do_python_check(
        &self,
        username: &str,
        password: &Password,
    ) -> Result<(), UserAuthError> {
        if crate::auth::cache::has_valid_password(
            self.base.config.basic_config().name(),
            username,
            password,
        ) {
            return Ok(());
        }

        let script = self.base.config.script_file.clone();

        let result = tokio::time::timeout(
            self.base.config.check_timeout,
            call_python_check_password(script, username.to_owned(), password.clone()),
        )
        .await;

        match result {
            Ok(Ok(true)) => {
                crate::auth::cache::save_user_password(
                    self.base.config.basic_config().name(),
                    self.base.config.cache_user_count,
                    username.to_owned(),
                    password.clone(),
                    self.base.config.cache_expire_time,
                );
                Ok(())
            }
            Ok(Ok(false)) => Err(UserAuthError::TokenNotMatch),
            Ok(Err(_)) => Err(UserAuthError::RemoteError),
            Err(_) => Err(UserAuthError::RemoteTimeout),
        }
    }
}

async fn call_python_check_password(
    script: PathBuf,
    username: String,
    password: Password,
) -> anyhow::Result<bool> {
    let code = load_code_as_cstring(&script).await?;
    let password_str = password.as_original().to_string();

    let handle = vey_daemon::runtime::main_handle()
        .ok_or_else(|| anyhow::anyhow!("no main runtime handle set"))?;
    handle
        .spawn_blocking(move || {
            Python::attach(|py| {
                let code = load_py_code(py, code, &script)?;

                let check_password = code.getattr(FN_NAME_CHECK_PASSWORD).map_err(|e| {
                    anyhow!(
                        "no {FN_NAME_CHECK_PASSWORD} function found in script {}: {e:?}",
                        script.display(),
                    )
                })?;

                let args =
                PyTuple::new(py, [username.as_str(), password_str.as_str()]).map_err(|e| {
                    anyhow!(
                        "failed to construct param tuple for {}::{FN_NAME_CHECK_PASSWORD}(): {e:?}",
                        script.display()
                    )
                })?;

                let result: bool = check_password
                .call1(args)
                .map_err(|e| {
                    anyhow!(
                        "failed to call {}::{FN_NAME_CHECK_PASSWORD}(): {e:?}",
                        script.display()
                    )
                })?
                .extract()
                .map_err(|e| {
                    anyhow!(
                        "failed to extract bool value from {}::{FN_NAME_CHECK_PASSWORD}(): {e:?}",
                        script.display(),
                    )
                })?;

                Ok(result)
            })
        })
        .await
        .map_err(|e| anyhow!("join blocking task error: {e}"))?
}

async fn load_code_as_cstring(path: &Path) -> anyhow::Result<CString> {
    let code = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| anyhow!("failed to read in content of file {}: {e}", path.display()))?;
    CString::new(code.into_bytes()).map_err(|e| anyhow!("failed to convert code to CString: {e}"))
}

fn load_py_code<'py>(
    py: Python<'py>,
    code: CString,
    path: &Path,
) -> anyhow::Result<Bound<'py, PyModule>> {
    let code = PyModule::from_code(py, &code, c"", c"").map_err(|e| {
        anyhow!(
            "failed to load code from script file {}: {e}",
            path.display(),
        )
    })?;
    code.setattr(VAR_NAME_FILE, path.display().to_string())
        .map_err(|e| anyhow!("failed to set {VAR_NAME_FILE} to {}: {e}", path.display()))?;
    Ok(code)
}
