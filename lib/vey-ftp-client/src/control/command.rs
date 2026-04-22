/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;
use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

use vey_io_ext::LimitedWriteExt;

use super::FtpControlChannel;

#[derive(Debug, Clone, Copy)]
pub struct FtpCommand(&'static str);

impl fmt::Display for FtpCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

macro_rules! ftp_commands {
    (
        $(
            $(#[$docs:meta])*
            ($konst:ident, $phrase:expr);
        )+
    ) => {
        impl FtpCommand {
        $(
            $(#[$docs])*
            pub const $konst: FtpCommand = FtpCommand($phrase);
        )+
        }
    };
}

ftp_commands! {
    /// a fake command for greeting
    (GREETING, "-");
    (SPDT, "SPDT");
    (FEAT, "FEAT");
    (OPTS_UTF8_ON, "OPTS UTF8 ON");
    (USER, "USER");
    (PASS, "PASS");
    (QUIT, "QUIT");
    (DELE, "DELE");
    (RMD, "RMD");
    (TYPE_A, "TYPE A");
    (TYPE_I, "TYPE I");
    (PASV, "PASV");
    (EPSV, "EPSV");
    (SPSV, "SPSV");
    (MLST, "MLST");
    (SIZE, "SIZE");
    (MDTM, "MDTM");
    (ABOR, "ABOR");
    (PRET, "PRET");
    (LIST, "LIST");
    (REST, "REST");
    (RETR, "RETR");
    (STOR, "STOR");
}

impl<T> FtpControlChannel<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    async fn send_all(&mut self) -> io::Result<()> {
        crate::log_cmd!(&self.cmd_line);
        self.cmd_line.push_str("\r\n");
        self.stream
            .write_all_flush(self.cmd_line.as_bytes())
            .await?;
        self.cmd_line.clear();
        Ok(())
    }

    pub(super) async fn send_cmd(&mut self, cmd: FtpCommand) -> io::Result<()> {
        let len = cmd.0.len() + 2;
        self.cmd_line.reserve(len);
        self.cmd_line.push_str(cmd.0);

        self.send_all().await
    }

    pub(super) async fn send_cmd1(&mut self, cmd: FtpCommand, param1: &str) -> io::Result<()> {
        let len = cmd.0.len() + 1 + param1.len() + 2;
        self.cmd_line.reserve(len);
        self.cmd_line.push_str(cmd.0);
        self.cmd_line.push(' ');
        self.cmd_line.push_str(param1);

        self.send_all().await
    }

    pub(super) async fn send_pre_transfer_cmd1(
        &mut self,
        cmd: FtpCommand,
        param1: &str,
    ) -> io::Result<()> {
        let len = 5 + cmd.0.len() + 1 + param1.len() + 2;
        self.cmd_line.reserve(len);
        self.cmd_line.push_str("PRET ");
        self.cmd_line.push_str(cmd.0);
        self.cmd_line.push(' ');
        self.cmd_line.push_str(param1);

        self.send_all().await
    }
}
