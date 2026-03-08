/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use vey_ctl::{CommandError, CommandResult};

use vey_gateway_proto::types_capnp::{fetch_result, operation_result};

pub(crate) fn parse_operation_result(r: operation_result::Reader<'_>) -> CommandResult<()> {
    match r.which().unwrap() {
        operation_result::Which::Ok(ok) => vey_ctl::print_ok_notice(ok?),
        operation_result::Which::Err(err) => {
            let e = err?;
            Err(CommandError::api_error(e.get_code(), e.get_reason()?))
        }
    }
}

pub(crate) fn parse_fetch_result<T>(
    r: fetch_result::Reader<'_, T>,
) -> CommandResult<<T as capnp::traits::Owned>::Reader<'_>>
where
    T: capnp::traits::Owned,
{
    match r.which().unwrap() {
        fetch_result::Which::Data(data) => Ok(data?),
        fetch_result::Which::Err(err) => {
            let e = err?;
            Err(CommandError::api_error(e.get_code(), e.get_reason()?))
        }
    }
}
