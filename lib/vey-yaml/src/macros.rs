/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[macro_export]
macro_rules! yaml_str {
    ($s:expr) => {
        ::saphyr::Yaml::Value(::saphyr::Scalar::String(::std::borrow::Cow::Borrowed($s)))
    };
}

#[macro_export]
macro_rules! yaml_doc {
    ($s:expr) => {{
        use ::saphyr::LoadableYamlNode as _;
        ::saphyr::Yaml::load_from_str($s).unwrap().remove(0)
    }};
}

#[cfg(test)]
mod tests {
    use saphyr::{Scalar, Yaml};

    #[test]
    fn yaml_str() {
        let v = yaml_str!("1234");
        assert!(matches!(v, Yaml::Value(Scalar::String(_))));

        let path = "absolute_path";
        let v = yaml_str!(path);
        assert_eq!(v.as_str(), Some(path));
    }

    #[test]
    fn yaml_doc() {
        let v = yaml_doc!(
            r#"
                key1: v1
            "#
        );
        assert!(matches!(v, Yaml::Mapping(_)));

        let v = yaml_doc!(
            r#"
                - v1
                - v2
            "#
        );
        assert!(matches!(v, Yaml::Sequence(_)));

        let v = yaml_doc!("1234");
        assert!(matches!(v, Yaml::Value(Scalar::Integer(1234))));

        let v = yaml_doc!("1.0");
        assert!(matches!(v, Yaml::Value(Scalar::FloatingPoint(_))));

        let v = yaml_doc!(r#""1234""#);
        assert!(matches!(v, Yaml::Value(Scalar::String(_))));
    }
}
