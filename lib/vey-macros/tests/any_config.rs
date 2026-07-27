/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use vey_macros::AnyConfig;

trait TestConfig {
    fn name(&self) -> &str;

    fn version(&self) -> usize;
    fn same_as(&self, other: &AnyTestConfig) -> bool;

    fn reload(&self);

    #[allow(unused)]
    async fn run(&self);
}

struct ConfigA {}

impl TestConfig for ConfigA {
    fn name(&self) -> &str {
        "A"
    }

    fn version(&self) -> usize {
        1
    }

    fn same_as(&self, _other: &AnyTestConfig) -> bool {
        false
    }

    fn reload(&self) {}

    async fn run(&self) {}
}

#[derive(AnyConfig)]
#[def_fn(name, &str)]
#[def_fn(version, usize)]
#[def_fn(same_as, &AnyTestConfig, bool)]
#[def_fn(reload)]
#[def_async_fn(run)]
pub(crate) enum AnyTestConfig {
    Variant1(ConfigA),
    // Variant 2
    Variant2(ConfigA),
}

#[test]
fn test_any() {
    let config = ConfigA {};
    let any_config = AnyTestConfig::Variant1(config);
    assert_eq!(any_config.name(), "A");
    assert_eq!(any_config.version(), 1);
    any_config.reload();

    let any_config2 = AnyTestConfig::Variant2(ConfigA {});
    assert!(!any_config.same_as(&any_config2));
}

#[test]
fn test_async_run() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let any_config = AnyTestConfig::Variant1(ConfigA {});
        any_config.run().await;
    });
}

struct ConfigB {
    label: String,
}

impl ConfigB {
    fn name(&self) -> &str {
        &self.label
    }

    fn version(&self) -> usize {
        2
    }

    fn same_as(&self, other: &AnyTestConfigWithParams) -> bool {
        match other {
            AnyTestConfigWithParams::Left(c) | AnyTestConfigWithParams::Right(c) => {
                c.label == self.label
            }
        }
    }

    fn reload(&self) {}

    async fn run(&self) {}
}

#[derive(AnyConfig)]
#[def_fn(name, &str)]
#[def_fn(version, usize)]
#[def_fn(same_as, &AnyTestConfigWithParams, bool)]
#[def_fn(reload)]
#[def_async_fn(run)]
pub(crate) enum AnyTestConfigWithParams {
    Left(ConfigB),
    Right(ConfigB),
}

#[test]
fn test_param_same_as() {
    let a = AnyTestConfigWithParams::Left(ConfigB {
        label: "B".to_string(),
    });
    let b = AnyTestConfigWithParams::Right(ConfigB {
        label: "B".to_string(),
    });
    let c = AnyTestConfigWithParams::Right(ConfigB {
        label: "C".to_string(),
    });
    assert_eq!(a.name(), "B");
    assert_eq!(a.version(), 2);
    a.reload();
    assert!(a.same_as(&b));
    assert!(!a.same_as(&c));

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        a.run().await;
    });
}
