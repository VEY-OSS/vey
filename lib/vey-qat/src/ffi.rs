/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use libc::{c_char, c_int, c_void};

pub type Cpa8U = u8;
pub type Cpa16U = u16;
pub type Cpa32U = u32;
pub type Cpa32S = i32;
pub type Cpa64U = u64;
pub type CpaStatus = Cpa32S;
pub type CpaBoolean = c_int;
pub type CpaInstanceHandle = *mut c_void;
pub type CpaPhysicalAddr = Cpa64U;
pub type CpaVirtualToPhysical = Option<unsafe extern "C" fn(*mut c_void) -> CpaPhysicalAddr>;

pub const CPA_STATUS_SUCCESS: CpaStatus = 0;
pub const CPA_STATUS_FAIL: CpaStatus = -1;
pub const CPA_STATUS_RETRY: CpaStatus = -2;
pub const CPA_TRUE: CpaBoolean = 1;
pub const CPA_FALSE: CpaBoolean = 0;

pub const CPA_CY_RSA_VERSION_TWO_PRIME: c_int = 1;
pub const CPA_CY_RSA_PRIVATE_KEY_REP_TYPE_2: c_int = 2;
pub const CPA_CY_EC_FIELD_TYPE_PRIME: c_int = 1;

/// `CpaAccelerationServiceType::CPA_ACC_SVC_TYPE_CRYPTO`
pub const CPA_ACC_SVC_TYPE_CRYPTO: c_int = 0;
/// `CpaInstanceResponseMode::CPA_INST_RX_NOTIFY_BY_EVENT`
pub const CPA_INST_RX_NOTIFY_BY_EVENT: c_int = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CpaFlatBuffer {
    pub dataLenInBytes: Cpa32U,
    pub pData: *mut Cpa8U,
}

#[repr(C)]
pub struct CpaCyRsaPublicKey {
    pub modulusN: CpaFlatBuffer,
    pub publicExponentE: CpaFlatBuffer,
}

#[repr(C)]
pub struct CpaCyRsaPrivateKeyRep1 {
    pub modulusN: CpaFlatBuffer,
    pub privateExponentD: CpaFlatBuffer,
}

#[repr(C)]
pub struct CpaCyRsaPrivateKeyRep2 {
    pub prime1P: CpaFlatBuffer,
    pub prime2Q: CpaFlatBuffer,
    pub exponent1Dp: CpaFlatBuffer,
    pub exponent2Dq: CpaFlatBuffer,
    pub coefficientQInv: CpaFlatBuffer,
}

#[repr(C)]
pub struct CpaCyRsaPrivateKey {
    pub version: c_int,
    pub privateKeyRepType: c_int,
    pub privateKeyRep1: CpaCyRsaPrivateKeyRep1,
    pub privateKeyRep2: CpaCyRsaPrivateKeyRep2,
}

#[repr(C)]
pub struct CpaCyRsaDecryptOpData {
    pub pRecipientPrivateKey: *mut CpaCyRsaPrivateKey,
    pub inputData: CpaFlatBuffer,
}

#[repr(C)]
pub struct CpaCyEcdsaSignRSOpData {
    pub xg: CpaFlatBuffer,
    pub yg: CpaFlatBuffer,
    pub n: CpaFlatBuffer,
    pub q: CpaFlatBuffer,
    pub a: CpaFlatBuffer,
    pub b: CpaFlatBuffer,
    pub k: CpaFlatBuffer,
    pub m: CpaFlatBuffer,
    pub d: CpaFlatBuffer,
    pub fieldType: c_int,
}

pub type CpaCyGenFlatBufCbFunc = Option<
    unsafe extern "C" fn(
        pCallbackTag: *mut c_void,
        status: CpaStatus,
        pOpdata: *mut c_void,
        pOut: *mut CpaFlatBuffer,
    ),
>;

pub type CpaCyEcdsaSignRSCbFunc = Option<
    unsafe extern "C" fn(
        pCallbackTag: *mut c_void,
        status: CpaStatus,
        pOpData: *mut c_void,
        multiplyStatus: CpaBoolean,
        pR: *mut CpaFlatBuffer,
        pS: *mut CpaFlatBuffer,
    ),
>;

unsafe extern "C" {
    pub fn icp_sal_userStart(pProcessName: *const c_char) -> CpaStatus;
    pub fn icp_sal_userStop() -> CpaStatus;
    pub fn icp_sal_CyPollInstance(
        instanceHandle: CpaInstanceHandle,
        response_quota: Cpa32U,
    ) -> CpaStatus;
    pub fn icp_sal_CyGetFileDescriptor(
        instanceHandle: CpaInstanceHandle,
        fd: *mut c_int,
    ) -> CpaStatus;
    pub fn icp_sal_CyPutFileDescriptor(instanceHandle: CpaInstanceHandle) -> CpaStatus;

    pub fn cpaInstanceSetResponseMode(
        instanceHandle: CpaInstanceHandle,
        accelerationServiceType: c_int,
        responseMode: c_int,
    ) -> CpaStatus;

    pub fn cpaCyGetNumInstances(pNumInstances: *mut Cpa16U) -> CpaStatus;
    pub fn cpaCyGetInstances(
        numInstances: Cpa16U,
        cyInstances: *mut CpaInstanceHandle,
    ) -> CpaStatus;
    pub fn cpaCyStartInstance(instanceHandle: CpaInstanceHandle) -> CpaStatus;
    pub fn cpaCyStopInstance(instanceHandle: CpaInstanceHandle) -> CpaStatus;
    pub fn cpaCySetAddressTranslation(
        instanceHandle: CpaInstanceHandle,
        virtual2Physical: CpaVirtualToPhysical,
    ) -> CpaStatus;

    pub fn cpaCyRsaDecrypt(
        instanceHandle: CpaInstanceHandle,
        pRsaDecryptCb: CpaCyGenFlatBufCbFunc,
        pCallbackTag: *mut c_void,
        pDecryptOpData: *const CpaCyRsaDecryptOpData,
        pOutputData: *mut CpaFlatBuffer,
    ) -> CpaStatus;

    pub fn cpaCyEcdsaSignRS(
        instanceHandle: CpaInstanceHandle,
        pCb: CpaCyEcdsaSignRSCbFunc,
        pCallbackTag: *mut c_void,
        pOpData: *const CpaCyEcdsaSignRSOpData,
        pSignStatus: *mut CpaBoolean,
        pR: *mut CpaFlatBuffer,
        pS: *mut CpaFlatBuffer,
    ) -> CpaStatus;

    pub fn qaeMemAllocNUMA(size: usize, node: c_int, phys_alignment_byte: usize) -> *mut c_void;
    pub fn qaeMemFreeNUMA(ptr: *mut *mut c_void);
    pub fn qaeVirtToPhysNUMA(pVirtAddr: *mut c_void) -> u64;
}
