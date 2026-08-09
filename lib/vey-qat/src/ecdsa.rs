/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use openssl::bn::{BigNum, BigNumContext};
use openssl::ecdsa::EcdsaSig;
use openssl::foreign_types::ForeignType;
use openssl::nid::Nid;
use openssl::pkey::{PKeyRef, Private};
use tokio::sync::oneshot;

use crate::Error;
use crate::ffi::{
    self, CPA_CY_EC_FIELD_TYPE_PRIME, CPA_STATUS_SUCCESS, CPA_TRUE, CpaBoolean,
    CpaCyEcdsaSignRSOpData, CpaFlatBuffer, CpaStatus,
};
use crate::mem::DmaBuf;
use crate::openssl_ffi;
use crate::runtime::QatRuntime;

type EcdsaRs = (Vec<u8>, Vec<u8>);

struct EcdsaPending {
    tx: Option<oneshot::Sender<Result<EcdsaRs, Error>>>,
    xg: DmaBuf,
    yg: DmaBuf,
    n: DmaBuf,
    q: DmaBuf,
    a: DmaBuf,
    b: DmaBuf,
    k: DmaBuf,
    m: DmaBuf,
    d: DmaBuf,
    r: DmaBuf,
    s: DmaBuf,
    op_data: Box<CpaCyEcdsaSignRSOpData>,
    r_flat: Box<CpaFlatBuffer>,
    s_flat: Box<CpaFlatBuffer>,
    sign_status: Box<CpaBoolean>,
}

unsafe extern "C" fn ecdsa_cb(
    tag: *mut libc::c_void,
    status: CpaStatus,
    _op: *mut libc::c_void,
    multiply_status: CpaBoolean,
    _r: *mut CpaFlatBuffer,
    _s: *mut CpaFlatBuffer,
) {
    let pending = unsafe { Box::from_raw(tag as *mut EcdsaPending) };
    let result = if status == CPA_STATUS_SUCCESS && multiply_status == CPA_TRUE {
        Ok((pending.r.as_slice().to_vec(), pending.s.as_slice().to_vec()))
    } else {
        Err(Error::Cpa(status))
    };
    if let Some(tx) = pending.tx {
        let _ = tx.send(result);
    }
}

fn bn_to_dma(bn: &openssl::bn::BigNumRef, len: usize) -> Option<DmaBuf> {
    let mut buf = DmaBuf::alloc(len)?;
    if !buf.copy_from_be_padded(&bn.to_vec()) {
        return None;
    }
    Some(buf)
}

fn field_len_for_nid(nid: Nid) -> Option<usize> {
    match nid {
        Nid::X9_62_PRIME256V1 => Some(32),
        Nid::SECP384R1 => Some(48),
        Nid::SECP521R1 => Some(66),
        _ => None,
    }
}

/// ECDSA sign; returns DER-encoded signature.
pub async fn ecdsa_sign_der(
    runtime: Arc<QatRuntime>,
    key: &PKeyRef<Private>,
    digest: &[u8],
) -> Result<Vec<u8>, Error> {
    let ec = key.ec_key().map_err(|_| Error::Prepare)?;
    let group = ec.group();
    let nid = group.curve_name().ok_or(Error::Prepare)?;
    let flen = field_len_for_nid(nid).ok_or(Error::Prepare)?;

    let mut ctx = BigNumContext::new().map_err(|_| Error::Prepare)?;
    let mut p = BigNum::new().map_err(|_| Error::Prepare)?;
    let mut a = BigNum::new().map_err(|_| Error::Prepare)?;
    let mut b = BigNum::new().map_err(|_| Error::Prepare)?;
    group
        .components_gfp(&mut p, &mut a, &mut b, &mut ctx)
        .map_err(|_| Error::Prepare)?;
    let mut order = BigNum::new().map_err(|_| Error::Prepare)?;
    group
        .order(&mut order, &mut ctx)
        .map_err(|_| Error::Prepare)?;
    let mut gx = BigNum::new().map_err(|_| Error::Prepare)?;
    let mut gy = BigNum::new().map_err(|_| Error::Prepare)?;
    let generator = group.generator_opt().ok_or(Error::Prepare)?;
    generator
        .affine_coordinates_gfp(group, &mut gx, &mut gy, &mut ctx)
        .map_err(|_| Error::Prepare)?;

    let k = {
        let eph = BigNum::new().map_err(|_| Error::Prepare)?;
        let mut ok = false;
        for _ in 0..64 {
            let rc = unsafe { openssl_ffi::BN_priv_rand_range(eph.as_ptr(), order.as_ptr()) };
            let is_zero = unsafe { openssl_ffi::BN_is_zero(eph.as_ptr()) == 1 };
            if rc == 1 && !is_zero {
                ok = true;
                break;
            }
        }
        if !ok {
            return Err(Error::Prepare);
        }
        eph
    };

    let mut msg = vec![0u8; flen];
    if digest.len() >= flen {
        msg.copy_from_slice(&digest[..flen]);
    } else {
        msg[flen - digest.len()..].copy_from_slice(digest);
    }

    let mut xg = bn_to_dma(&gx, flen).ok_or(Error::Prepare)?;
    let mut yg = bn_to_dma(&gy, flen).ok_or(Error::Prepare)?;
    let mut n = bn_to_dma(&order, flen).ok_or(Error::Prepare)?;
    let mut q = bn_to_dma(&p, flen).ok_or(Error::Prepare)?;
    let mut aa = bn_to_dma(&a, flen).ok_or(Error::Prepare)?;
    let mut bb = bn_to_dma(&b, flen).ok_or(Error::Prepare)?;
    let mut kk = bn_to_dma(&k, flen).ok_or(Error::Prepare)?;
    let mut mm = DmaBuf::alloc(flen).ok_or(Error::Prepare)?;
    mm.as_mut_slice().copy_from_slice(&msg);
    let mut dd = bn_to_dma(ec.private_key(), flen).ok_or(Error::Prepare)?;
    let mut r_buf = DmaBuf::alloc(flen).ok_or(Error::Prepare)?;
    let mut s_buf = DmaBuf::alloc(flen).ok_or(Error::Prepare)?;

    let op_data = Box::new(CpaCyEcdsaSignRSOpData {
        xg: xg.flat(),
        yg: yg.flat(),
        n: n.flat(),
        q: q.flat(),
        a: aa.flat(),
        b: bb.flat(),
        k: kk.flat(),
        m: mm.flat(),
        d: dd.flat(),
        fieldType: CPA_CY_EC_FIELD_TYPE_PRIME,
    });
    let r_flat = Box::new(r_buf.flat());
    let s_flat = Box::new(s_buf.flat());

    let mut pending = Box::new(EcdsaPending {
        tx: None,
        xg,
        yg,
        n,
        q,
        a: aa,
        b: bb,
        k: kk,
        m: mm,
        d: dd,
        r: r_buf,
        s: s_buf,
        op_data,
        r_flat,
        s_flat,
        sign_status: Box::new(CPA_TRUE),
    });

    pending.op_data.xg = pending.xg.flat();
    pending.op_data.yg = pending.yg.flat();
    pending.op_data.n = pending.n.flat();
    pending.op_data.q = pending.q.flat();
    pending.op_data.a = pending.a.flat();
    pending.op_data.b = pending.b.flat();
    pending.op_data.k = pending.k.flat();
    pending.op_data.m = pending.m.flat();
    pending.op_data.d = pending.d.flat();
    pending.r_flat = Box::new(pending.r.flat());
    pending.s_flat = Box::new(pending.s.flat());

    let (tx, rx) = oneshot::channel();
    pending.tx = Some(tx);
    let tag = Box::into_raw(pending);

    let sts = unsafe {
        ffi::cpaCyEcdsaSignRS(
            runtime.instance(),
            Some(ecdsa_cb),
            tag as *mut _,
            (*tag).op_data.as_ref(),
            (*tag).sign_status.as_mut(),
            (*tag).r_flat.as_mut(),
            (*tag).s_flat.as_mut(),
        )
    };
    if sts != CPA_STATUS_SUCCESS {
        let _ = unsafe { Box::from_raw(tag) };
        return Err(Error::Cpa(sts));
    }

    let (r, s) = rx.await.map_err(|_| Error::Canceled)??;
    let r_bn = BigNum::from_slice(&r).map_err(|_| Error::Prepare)?;
    let s_bn = BigNum::from_slice(&s).map_err(|_| Error::Prepare)?;
    let sig = EcdsaSig::from_private_components(r_bn, s_bn).map_err(|_| Error::Prepare)?;
    sig.to_der().map_err(|_| Error::Prepare)
}
