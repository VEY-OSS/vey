/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{CommandError, CommandResult};

pub fn print_ok_notice(notice_reader: capnp::text::Reader<'_>) -> CommandResult<()> {
    match notice_reader.to_str() {
        Ok(notice) => {
            println!("notice: {notice}");
            Ok(())
        }
        Err(e) => Err(CommandError::Utf8 {
            field: "ok",
            reason: e,
        }),
    }
}

pub fn print_text(field: &'static str, text_reader: capnp::text::Reader<'_>) -> CommandResult<()> {
    match text_reader.to_str() {
        Ok(text) => {
            println!("{text}");
            Ok(())
        }
        Err(e) => Err(CommandError::Utf8 { field, reason: e }),
    }
}

#[inline]
pub fn print_version(version_reader: capnp::text::Reader<'_>) -> CommandResult<()> {
    print_text("version", version_reader)
}

pub fn print_text_list(
    field: &'static str,
    list: capnp::text_list::Reader<'_>,
) -> CommandResult<()> {
    for text in list.iter() {
        print_text(field, text?)?;
    }
    Ok(())
}

#[inline]
pub fn print_result_list(result_list_reader: capnp::text_list::Reader<'_>) -> CommandResult<()> {
    print_text_list("result", result_list_reader)
}

pub fn print_data(data_reader: capnp::data::Reader<'_>) {
    println!("{}", hex::encode(data_reader));
}

pub fn print_data_list(list: capnp::data_list::Reader<'_>) -> CommandResult<()> {
    for data in list.iter() {
        print_data(data?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use capnp::message;

    use super::*;

    #[test]
    fn print_ok_notice_ok() {
        let reader = capnp::text::Reader(b"reloaded");
        assert!(print_ok_notice(reader).is_ok());
    }

    #[test]
    fn print_text_ok() {
        let reader = capnp::text::Reader(b"1.2.3");
        assert!(print_text("version", reader).is_ok());
    }

    #[test]
    fn print_text_invalid_utf8() {
        let reader = capnp::text::Reader(&[0xff]);
        let err = print_text("version", reader).unwrap_err();
        match err {
            CommandError::Utf8 { field, .. } => assert_eq!(field, "version"),
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn print_data_encodes_hex() {
        print_data(&[0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn print_text_list_ok() {
        let mut message = message::Builder::new_default();
        message
            .set_root(&["alpha", "beta"] as &[&str])
            .unwrap();
        let reader = message
            .get_root_as_reader::<capnp::text_list::Reader>()
            .unwrap();
        assert!(print_text_list("result", reader).is_ok());
    }

    #[test]
    fn print_data_list_empty() {
        let mut message = message::Builder::new_default();
        message.init_root::<capnp::data_list::Builder>();
        let reader = message
            .get_root_as_reader::<capnp::data_list::Reader>()
            .unwrap();
        assert!(print_data_list(reader).is_ok());
    }
}
