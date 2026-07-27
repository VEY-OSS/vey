/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::str::FromStr;

#[cfg_attr(any(target_os = "linux", target_os = "android"), path = "linux.rs")]
#[cfg_attr(
    any(target_os = "freebsd", target_os = "dragonfly"),
    path = "freebsd.rs"
)]
#[cfg_attr(target_os = "netbsd", path = "netbsd.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;
use os::CpuAffinityImpl;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CpuId(usize);

impl FromStr for CpuId {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id = usize::from_str(s).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid CPU ID {s}: {e}"),
            )
        })?;
        Ok(CpuId(id))
    }
}

#[derive(Clone)]
pub struct CpuAffinity {
    os_impl: CpuAffinityImpl,
    cpu_id_list: Vec<usize>,
    max_cpu_id: usize,
}

impl Default for CpuAffinity {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuAffinity {
    pub fn new() -> Self {
        let os_impl = CpuAffinityImpl::default();
        let max_cpu_id = os_impl.max_cpu_id();
        CpuAffinity {
            os_impl,
            cpu_id_list: Vec::new(),
            max_cpu_id,
        }
    }

    pub fn cpu_id_list(&self) -> &[usize] {
        &self.cpu_id_list
    }

    pub fn add_id(&mut self, id: usize) -> io::Result<()> {
        if id > self.max_cpu_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid CPU ID, the max allowed is {}", self.max_cpu_id),
            ));
        }
        self.os_impl.add_id(id)?;
        self.cpu_id_list.push(id);
        Ok(())
    }

    pub fn parse_add(&mut self, s: &str) -> io::Result<()> {
        for p in s.split(',') {
            let part = p.trim();
            if part.is_empty() {
                continue;
            }

            match part.split_once('-') {
                Some((s1, s2)) => {
                    let start = CpuId::from_str(s1)?;
                    let end = CpuId::from_str(s2)?;
                    if start > end {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("invalid CPU ID range {part}"),
                        ));
                    }
                    for id in start.0..=end.0 {
                        self.add_id(id)?;
                    }
                }
                None => {
                    let id = CpuId::from_str(part)?;
                    self.add_id(id.0)?;
                }
            }
        }
        Ok(())
    }

    pub fn apply_to_local_thread(&self) -> io::Result<()> {
        self.os_impl.apply_to_local_thread()
    }
}

#[cfg(all(
    test,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        windows,
    )
))]
mod tests {
    use super::*;

    #[test]
    fn single() {
        let mut affinity = CpuAffinity::default();
        assert!(affinity.cpu_id_list().is_empty());
        affinity.add_id(1).unwrap();
        assert_eq!(affinity.cpu_id_list(), &[1]);
    }

    #[test]
    fn many() {
        let mut affinity = CpuAffinity::default();
        affinity.add_id(2).unwrap();
        affinity.parse_add("0").unwrap();
        assert_eq!(affinity.cpu_id_list(), &[2, 0]);
    }

    #[test]
    fn range() {
        let mut affinity = CpuAffinity::default();
        affinity.parse_add("0-1,4").unwrap();
        assert_eq!(affinity.cpu_id_list(), &[0, 1, 4]);
    }

    #[test]
    fn parse_add_skips_empty_parts() {
        let mut affinity = CpuAffinity::default();
        affinity.parse_add(" 1 , ,2 ").unwrap();
        assert_eq!(affinity.cpu_id_list(), &[1, 2]);
    }

    #[test]
    fn parse_add_accepts_single_id_range() {
        let mut affinity = CpuAffinity::default();
        affinity.parse_add("1-1,4-4").unwrap();
        assert_eq!(affinity.cpu_id_list(), &[1, 4]);
    }

    #[test]
    fn parse_add_rejects_inverted_range() {
        let mut affinity = CpuAffinity::default();
        assert!(affinity.parse_add("2-1").is_err());
    }

    #[test]
    fn parse_add_rejects_non_numeric() {
        let mut affinity = CpuAffinity::default();
        let err = affinity.parse_add("abc").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn add_id_rejects_out_of_range() {
        let mut affinity = CpuAffinity::default();
        let err = affinity.add_id(usize::MAX).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
