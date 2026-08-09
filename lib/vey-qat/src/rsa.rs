/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use openssl::nid::Nid;
use openssl::pkey::{PKeyRef, Private};
use openssl::rsa::Padding;
use tokio::sync::oneshot;

use crate::Error;
use crate::ffi::{
    self, CPA_CY_RSA_PRIVATE_KEY_REP_TYPE_2, CPA_CY_RSA_VERSION_TWO_PRIME, CPA_STATUS_SUCCESS,
    CpaCyRsaDecryptOpData, CpaCyRsaPrivateKey, CpaFlatBuffer, CpaStatus,
};
use crate::mem::DmaBuf;
use crate::padding::{add_pkcs1_sign_padding, add_pss_sign_padding, check_decrypt_padding};
use crate::runtime::QatRuntime;

struct RsaPending {
    tx: Option<oneshot::Sender<Result<Vec<u8>, Error>>>,
    _p: DmaBuf,
    _q: DmaBuf,
    _dp: DmaBuf,
    _dq: DmaBuf,
    _qinv: DmaBuf,
    _input: DmaBuf,
    output: DmaBuf,
    key: Box<CpaCyRsaPrivateKey>,
    op_data: Box<CpaCyRsaDecryptOpData>,
    out_flat: Box<CpaFlatBuffer>,
}

unsafe extern "C" fn rsa_cb(
    tag: *mut libc::c_void,
    status: CpaStatus,
    _op: *mut libc::c_void,
    _out: *mut CpaFlatBuffer,
) {
    let pending = unsafe { Box::from_raw(tag as *mut RsaPending) };
    let result = if status == CPA_STATUS_SUCCESS {
        Ok(pending.output.as_slice().to_vec())
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

fn build_pending(rsa: &openssl::rsa::RsaRef<Private>, input: &[u8]) -> Option<Box<RsaPending>> {
    let key_len = rsa.size() as usize;
    if input.len() != key_len {
        return None;
    }
    let half = key_len.div_ceil(2);

    let mut p = bn_to_dma(rsa.p()?, half)?;
    let mut q = bn_to_dma(rsa.q()?, half)?;
    let mut dp = bn_to_dma(rsa.dmp1()?, half)?;
    let mut dq = bn_to_dma(rsa.dmq1()?, half)?;
    let mut qinv = bn_to_dma(rsa.iqmp()?, half)?;
    let mut in_buf = DmaBuf::alloc(key_len)?;
    in_buf.as_mut_slice().copy_from_slice(input);
    let mut out_buf = DmaBuf::alloc(key_len)?;

    let mut key = Box::new(unsafe { std::mem::zeroed::<CpaCyRsaPrivateKey>() });
    key.version = CPA_CY_RSA_VERSION_TWO_PRIME;
    key.privateKeyRepType = CPA_CY_RSA_PRIVATE_KEY_REP_TYPE_2;
    key.privateKeyRep2.prime1P = p.flat();
    key.privateKeyRep2.prime2Q = q.flat();
    key.privateKeyRep2.exponent1Dp = dp.flat();
    key.privateKeyRep2.exponent2Dq = dq.flat();
    key.privateKeyRep2.coefficientQInv = qinv.flat();

    let op_data = Box::new(CpaCyRsaDecryptOpData {
        pRecipientPrivateKey: key.as_mut() as *mut _,
        inputData: in_buf.flat(),
    });
    let out_flat = Box::new(out_buf.flat());

    Some(Box::new(RsaPending {
        tx: None,
        _p: p,
        _q: q,
        _dp: dp,
        _dq: dq,
        _qinv: qinv,
        _input: in_buf,
        output: out_buf,
        key,
        op_data,
        out_flat,
    }))
}

async fn rsa_private_crt(
    runtime: &QatRuntime,
    rsa: &openssl::rsa::RsaRef<Private>,
    input: &[u8],
) -> Result<Vec<u8>, Error> {
    let mut pending = build_pending(rsa, input).ok_or(Error::Prepare)?;
    // Refresh pointers after move into Box (DmaBuf ptrs are stable; key/op self-refs are not).
    pending.key.privateKeyRep2.prime1P = pending._p.flat();
    pending.key.privateKeyRep2.prime2Q = pending._q.flat();
    pending.key.privateKeyRep2.exponent1Dp = pending._dp.flat();
    pending.key.privateKeyRep2.exponent2Dq = pending._dq.flat();
    pending.key.privateKeyRep2.coefficientQInv = pending._qinv.flat();
    pending.op_data.pRecipientPrivateKey = pending.key.as_mut();
    pending.op_data.inputData = pending._input.flat();
    pending.out_flat = Box::new(pending.output.flat());

    let (tx, rx) = oneshot::channel();
    pending.tx = Some(tx);

    let tag = Box::into_raw(pending);
    let sts = unsafe {
        ffi::cpaCyRsaDecrypt(
            runtime.instance(),
            Some(rsa_cb),
            tag as *mut _,
            (*tag).op_data.as_ref(),
            (*tag).out_flat.as_mut(),
        )
    };
    if sts != CPA_STATUS_SUCCESS {
        let _ = unsafe { Box::from_raw(tag) };
        return Err(Error::Cpa(sts));
    }
    rx.await.map_err(|_| Error::Canceled)?
}

/// PKCS#1 sign via software padding + QAT RSA private primitive.
pub async fn rsa_sign_pkcs1(
    runtime: Arc<QatRuntime>,
    key: &PKeyRef<Private>,
    nid: Nid,
    digest: &[u8],
) -> Result<Vec<u8>, Error> {
    let rsa = key.rsa().map_err(|_| Error::Prepare)?;
    let key_len = rsa.size() as usize;
    let mut em = vec![0u8; key_len];
    if !add_pkcs1_sign_padding(nid, digest, &mut em) {
        return Err(Error::Prepare);
    }
    rsa_private_crt(&runtime, &rsa, &em).await
}

pub async fn rsa_sign_pss(
    runtime: Arc<QatRuntime>,
    key: &PKeyRef<Private>,
    nid: Nid,
    digest: &[u8],
) -> Result<Vec<u8>, Error> {
    let rsa = key.rsa().map_err(|_| Error::Prepare)?;
    let key_len = rsa.size() as usize;
    let mut em = vec![0u8; key_len];
    if !add_pss_sign_padding(&rsa, nid, digest, &mut em) {
        return Err(Error::Prepare);
    }
    rsa_private_crt(&runtime, &rsa, &em).await
}

/// RSA private decrypt + software unpad.
pub async fn rsa_decrypt(
    runtime: Arc<QatRuntime>,
    key: &PKeyRef<Private>,
    ciphertext: &[u8],
    padding: Padding,
) -> Result<Vec<u8>, Error> {
    let rsa = key.rsa().map_err(|_| Error::Prepare)?;
    let key_len = rsa.size() as usize;
    let out = rsa_private_crt(&runtime, &rsa, ciphertext).await?;
    let mut plain = vec![0u8; key_len];
    let len = check_decrypt_padding(padding, &out, &mut plain, key_len).ok_or(Error::Prepare)?;
    plain.truncate(len);
    Ok(plain)
}
