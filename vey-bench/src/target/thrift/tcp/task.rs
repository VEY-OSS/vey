/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use tokio::time::Instant;

use super::{
    MultiplexTransfer, SimplexTransfer, ThriftConnectionPool, ThriftHistogramRecorder,
    ThriftRuntimeStats, ThriftTcpArgs,
};
use crate::ProcArgs;
use crate::target::thrift::tcp::connection::ThriftTcpResponse;
use crate::target::{BenchError, BenchTaskContext};

pub(super) struct ThriftTcpTaskContext {
    args: Arc<ThriftTcpArgs>,
    proc_args: Arc<ProcArgs>,

    pool: Option<Arc<ThriftConnectionPool>>,
    multiplex: Option<Arc<MultiplexTransfer>>,
    simplex: Option<SimplexTransfer>,

    conn_used_times: u64,

    runtime_stats: Arc<ThriftRuntimeStats>,
    histogram_recorder: ThriftHistogramRecorder,
}

impl Drop for ThriftTcpTaskContext {
    fn drop(&mut self) {
        if self.conn_used_times > 0 {
            self.histogram_recorder
                .record_conn_used_times(self.conn_used_times);
            self.conn_used_times = 0;
        }
    }
}

impl ThriftTcpTaskContext {
    pub(super) fn new(
        args: &Arc<ThriftTcpArgs>,
        proc_args: &Arc<ProcArgs>,
        runtime_stats: &Arc<ThriftRuntimeStats>,
        histogram_recorder: ThriftHistogramRecorder,
        pool: Option<Arc<ThriftConnectionPool>>,
    ) -> anyhow::Result<Self> {
        Ok(ThriftTcpTaskContext {
            args: args.clone(),
            proc_args: proc_args.clone(),
            pool,
            multiplex: None,
            simplex: None,
            conn_used_times: 0,
            runtime_stats: runtime_stats.clone(),
            histogram_recorder,
        })
    }

    async fn fetch_multiplex_handle(&mut self) -> anyhow::Result<Arc<MultiplexTransfer>> {
        if let Some(pool) = &self.pool {
            return pool.fetch_handle().await;
        }

        if let Some(handle) = &self.multiplex {
            if !handle.is_closed() {
                self.conn_used_times += 1;
                return Ok(handle.clone());
            }
            self.multiplex = None;
        }

        if self.conn_used_times > 0 {
            self.histogram_recorder
                .record_conn_used_times(self.conn_used_times);
            self.conn_used_times = 0;
        }

        self.runtime_stats.add_conn_attempt();
        let handle = match tokio::time::timeout(
            self.args.connect_timeout,
            self.args.new_multiplex_connection(&self.proc_args),
        )
        .await
        {
            Ok(Ok(h)) => Arc::new(h),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(anyhow!("timeout to get new connection")),
        };
        self.runtime_stats.add_conn_success();

        self.multiplex = Some(handle.clone());
        self.conn_used_times += 1;
        Ok(handle)
    }

    async fn fetch_simplex_connection(&mut self) -> anyhow::Result<SimplexTransfer> {
        if let Some(mut c) = self.simplex.take()
            && !c.is_closed()
        {
            self.conn_used_times += 1;
            return Ok(c);
        }

        if self.conn_used_times > 0 {
            self.histogram_recorder
                .record_conn_used_times(self.conn_used_times);
            self.conn_used_times = 0;
        }

        self.runtime_stats.add_conn_attempt();
        match tokio::time::timeout(
            self.args.connect_timeout,
            self.args.new_simplex_connection(&self.proc_args),
        )
        .await
        {
            Ok(Ok(c)) => {
                self.runtime_stats.add_conn_success();
                self.conn_used_times += 1;
                Ok(c)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!("timeout to get new connection")),
        }
    }

    async fn do_run_multiplex(
        &self,
        handle: &MultiplexTransfer,
    ) -> anyhow::Result<ThriftTcpResponse> {
        match tokio::time::timeout(
            self.args.timeout,
            handle.send_request(self.args.global.payload.clone()),
        )
        .await
        {
            Ok(Ok(rsp)) => Ok(rsp),
            Ok(Err(id)) => match handle.fetch_error() {
                Some(e) => Err(anyhow!("{}/{id} error: {e}", handle.local_addr())),
                None => Err(anyhow!(
                    "{}/{id}: we get no response but no error reported",
                    handle.local_addr()
                )),
            },
            Err(_) => Err(anyhow!("{}: request timed out", handle.local_addr())),
        }
    }

    async fn do_run_simplex(
        &mut self,
        connection: &mut SimplexTransfer,
    ) -> anyhow::Result<ThriftTcpResponse> {
        match tokio::time::timeout(
            self.args.timeout,
            connection.send_request(&self.args.global.payload),
        )
        .await
        {
            Ok(Ok(rsp)) => Ok(rsp),
            Ok(Err(e)) => Err(anyhow!("{} error: {e}", connection.local_addr())),
            Err(_) => Err(anyhow!("{}: request timed out", connection.local_addr())),
        }
    }

    fn check_response(&self, rsp: ThriftTcpResponse) -> anyhow::Result<()> {
        if rsp.seq_id != rsp.message.seq_id {
            return Err(anyhow!(
                "sequence number in header not match the value in message"
            ));
        }

        if rsp.message.method != self.args.global.method {
            return Err(anyhow!(
                "unexpected method name {} in response",
                rsp.message.method
            ));
        }

        if let Some(check_message_length) = self.args.global.check_message_length
            && check_message_length != rsp.message.encoded_length
        {
            return Err(anyhow!(
                "unexpected received message length {}",
                rsp.message.encoded_length
            ));
        }

        Ok(())
    }
}

impl BenchTaskContext for ThriftTcpTaskContext {
    fn mark_task_start(&self) {
        self.runtime_stats.add_task_total();
        self.runtime_stats.inc_task_alive();
    }

    fn mark_task_passed(&self) {
        self.runtime_stats.add_task_passed();
        self.runtime_stats.dec_task_alive();
    }

    fn mark_task_failed(&self) {
        self.runtime_stats.add_task_failed();
        self.runtime_stats.dec_task_alive();
    }

    async fn run(&mut self, _task_id: usize, time_started: Instant) -> Result<(), BenchError> {
        if self.args.multiplex {
            let handle = self
                .fetch_multiplex_handle()
                .await
                .map_err(BenchError::Fatal)?;

            match self.do_run_multiplex(&handle).await {
                Ok(rsp) => {
                    let total_time = time_started.elapsed();
                    self.histogram_recorder.record_total_time(total_time);

                    self.check_response(rsp).map_err(BenchError::Task)
                }
                Err(e) => {
                    self.multiplex = None;
                    Err(BenchError::Task(e))
                }
            }
        } else {
            let mut handle = self
                .fetch_simplex_connection()
                .await
                .map_err(BenchError::Fatal)?;

            match self.do_run_simplex(&mut handle).await {
                Ok(rsp) => {
                    let total_time = time_started.elapsed();
                    if !self.args.no_keepalive {
                        self.simplex = Some(handle);
                    }
                    self.histogram_recorder.record_total_time(total_time);

                    self.check_response(rsp).map_err(BenchError::Task)
                }
                Err(e) => Err(BenchError::Task(e)),
            }
        }
    }
}
