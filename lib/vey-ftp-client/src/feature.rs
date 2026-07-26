/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[derive(Default)]
pub(crate) struct FtpServerFeature {
    utf8_path: bool,
    file_size: bool,
    file_mtime: bool,
    rest_stream: bool,
    pre_transfer: bool,
    machine_list: bool,
    extended_passive: bool,
    single_port_passive: bool,
}

impl FtpServerFeature {
    pub(crate) fn parse_and_set(&mut self, s: &str) {
        let (f, v) = s.split_once(' ').unwrap_or((s, ""));
        match f.to_lowercase().as_str() {
            "utf8" => self.utf8_path = true,
            "size" => self.file_size = true,
            "mdtm" => self.file_mtime = true,
            "rest" if v.to_lowercase().eq("stream") => {
                self.rest_stream = true;
            }
            "pret" => self.pre_transfer = true,
            "mlst" => self.machine_list = true,
            "epsv" => self.extended_passive = true,
            "spsv" => self.single_port_passive = true,
            _ => {}
        }
    }

    #[inline]
    pub(crate) fn support_utf8_path(&self) -> bool {
        self.utf8_path
    }

    #[inline]
    pub(crate) fn support_file_size(&self) -> bool {
        self.file_size
    }

    #[inline]
    pub(crate) fn support_file_mtime(&self) -> bool {
        self.file_mtime
    }

    #[inline]
    pub(crate) fn support_rest_stream(&self) -> bool {
        self.rest_stream
    }

    #[inline]
    pub(crate) fn support_pre_transfer(&self) -> bool {
        self.pre_transfer
    }

    #[inline]
    pub(crate) fn support_machine_list(&self) -> bool {
        self.machine_list
    }

    #[inline]
    pub(crate) fn support_epsv(&self) -> bool {
        self.extended_passive
    }

    #[inline]
    pub(crate) fn support_spsv(&self) -> bool {
        self.single_port_passive
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_set_features() {
        let mut feat = FtpServerFeature::default();
        for line in [
            "UTF8",
            "SIZE",
            "MDTM",
            "REST STREAM",
            "PRET",
            "MLST",
            "EPSV",
            "SPSV",
            "UNKNOWN",
        ] {
            feat.parse_and_set(line);
        }
        assert!(feat.support_utf8_path());
        assert!(feat.support_file_size());
        assert!(feat.support_file_mtime());
        assert!(feat.support_rest_stream());
        assert!(feat.support_pre_transfer());
        assert!(feat.support_machine_list());
        assert!(feat.support_epsv());
        assert!(feat.support_spsv());
    }

    #[test]
    fn rest_requires_stream_suffix() {
        let mut feat = FtpServerFeature::default();
        feat.parse_and_set("REST");
        assert!(!feat.support_rest_stream());
        feat.parse_and_set("REST stream");
        assert!(feat.support_rest_stream());
    }
}
