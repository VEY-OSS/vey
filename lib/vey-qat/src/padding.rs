/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use libc::c_int;
use openssl::foreign_types::ForeignTypeRef;
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::Private;
use openssl::rsa::{Padding, RsaRef};
use openssl_sys::RSA_padding_check_PKCS1_type_2;

use crate::openssl_ffi;

pub fn add_pkcs1_sign_padding(nid: Nid, digest: &[u8], em: &mut [u8]) -> bool {
    let tlen = em.len() as c_int;
    let rc = if nid == Nid::MD5_SHA1 {
        if digest.len() != 36 {
            return false;
        }
        unsafe {
            openssl_ffi::RSA_padding_add_PKCS1_type_1(
                em.as_mut_ptr(),
                tlen,
                digest.as_ptr(),
                digest.len() as c_int,
            )
        }
    } else {
        let Some(prefix) = digestinfo_prefix(nid) else {
            return false;
        };
        let mut encoded = [0u8; 96];
        let encoded_len = prefix.len() + digest.len();
        if encoded_len > encoded.len() {
            return false;
        }
        encoded[..prefix.len()].copy_from_slice(prefix);
        encoded[prefix.len()..encoded_len].copy_from_slice(digest);
        unsafe {
            openssl_ffi::RSA_padding_add_PKCS1_type_1(
                em.as_mut_ptr(),
                tlen,
                encoded.as_ptr(),
                encoded_len as c_int,
            )
        }
    };
    rc == 1
}

pub fn add_pss_sign_padding(rsa: &RsaRef<Private>, nid: Nid, digest: &[u8], em: &mut [u8]) -> bool {
    let Some(md) = MessageDigest::from_nid(nid) else {
        return false;
    };
    let rc = unsafe {
        openssl_ffi::RSA_padding_add_PKCS1_PSS_mgf1(
            rsa.as_ptr(),
            em.as_mut_ptr(),
            digest.as_ptr(),
            md.as_ptr(),
            md.as_ptr(),
            -1,
        )
    };
    rc == 1
}

pub fn check_decrypt_padding(
    padding: Padding,
    from: &[u8],
    to: &mut [u8],
    rsa_len: usize,
) -> Option<usize> {
    let flen = from.len() as c_int;
    let tlen = to.len() as c_int;
    let num = rsa_len as c_int;
    let len = match padding {
        Padding::NONE => unsafe {
            openssl_ffi::RSA_padding_check_none(to.as_mut_ptr(), tlen, from.as_ptr(), flen, num)
        },
        Padding::PKCS1 => unsafe {
            RSA_padding_check_PKCS1_type_2(to.as_mut_ptr(), tlen, from.as_ptr(), flen, num)
        },
        _ => return None,
    };
    if len < 0 { None } else { Some(len as usize) }
}

fn digestinfo_prefix(nid: Nid) -> Option<&'static [u8]> {
    match nid {
        Nid::SHA1 => Some(&[
            0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04,
            0x14,
        ]),
        Nid::SHA224 => Some(&[
            0x30, 0x2d, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x04, 0x05, 0x00, 0x04, 0x1c,
        ]),
        Nid::SHA256 => Some(&[
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x01, 0x05, 0x00, 0x04, 0x20,
        ]),
        Nid::SHA384 => Some(&[
            0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x02, 0x05, 0x00, 0x04, 0x30,
        ]),
        Nid::SHA512 => Some(&[
            0x30, 0x51, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x03, 0x05, 0x00, 0x04, 0x40,
        ]),
        _ => None,
    }
}
