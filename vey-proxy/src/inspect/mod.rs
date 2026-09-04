/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arcstr::ArcStr;
use slog::Logger;
use tokio::io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use vey_daemon::server::ServerQuitPolicy;
use vey_dpi::{
    H1InterceptionConfig, H2InterceptionConfig, ImapInterceptionConfig, MaybeProtocol,
    ProtocolInspectAction, ProtocolInspector, SmtpInterceptionConfig,
};
use vey_io_ext::IdleWheel;
use vey_types::net::{Host, OpensslClientConfig};

use crate::audit::AuditHandle;
use crate::auth::{User, UserContext, UserForbiddenStats, UserSite};
use crate::config::server::ServerConfig;
use crate::escape::EgressNotes;
use crate::module::http_forward::HttpProxyClientResponse;
use crate::serve::{ArcServerStats, ServerIdleChecker, ServerTaskNotes};
use crate::site::SiteContext;

mod error;
pub(crate) use error::InterceptionError;

pub(crate) mod stream;
pub(crate) use stream::StreamTransitTask;

pub(crate) mod tls;
use tls::TlsInterceptionContext;

pub(crate) mod start_tls;
use start_tls::StartTlsProtocol;

pub(crate) mod http;
mod websocket;

pub(crate) mod imap;
pub(crate) mod smtp;

/// SWG / expose visitor snapshot. `user_site` is the client's `explicit_sites` overlay.
#[derive(Clone)]
pub(super) struct StreamInspectUserContext {
    raw_user_name: Option<ArcStr>,
    user: Arc<User>,
    user_site: Option<Arc<UserSite>>,
    forbidden_stats: Arc<UserForbiddenStats>,
}

impl StreamInspectUserContext {
    fn from_user_context(ctx: &UserContext) -> Self {
        StreamInspectUserContext {
            raw_user_name: ctx.raw_user_name().cloned(),
            user: ctx.user().clone(),
            user_site: ctx.user_site().cloned(),
            forbidden_stats: ctx.forbidden_stats().clone(),
        }
    }

    fn http_rsp_hdr_recv_timeout(&self) -> Option<Duration> {
        self.user_site
            .as_ref()
            .and_then(|site| site.http_rsp_hdr_recv_timeout())
            .or(self.user.http_rsp_hdr_recv_timeout())
    }
}

/// Reverse-proxy origin snapshot. Tenant is the site owner; there is no `user_site`.
#[derive(Clone)]
pub(super) struct StreamInspectSiteContext {
    tenant: Option<StreamInspectTenant>,
    rsp_hdr_recv_timeout: Option<Duration>,
}

#[derive(Clone)]
struct StreamInspectTenant {
    raw_user_name: Option<ArcStr>,
    user: Arc<User>,
    forbidden_stats: Arc<UserForbiddenStats>,
}

impl StreamInspectSiteContext {
    fn from_site_context(ctx: &SiteContext) -> Self {
        StreamInspectSiteContext {
            tenant: ctx.tenant().map(|t| StreamInspectTenant {
                raw_user_name: t.raw_user_name().cloned(),
                user: t.user().clone(),
                forbidden_stats: t.forbidden_stats().clone(),
            }),
            rsp_hdr_recv_timeout: ctx.rsp_hdr_recv_timeout(),
        }
    }

    fn tenant(&self) -> Option<&Arc<User>> {
        self.tenant.as_ref().map(|t| &t.user)
    }

    fn raw_username(&self) -> Option<&ArcStr> {
        self.tenant.as_ref().and_then(|t| t.raw_user_name.as_ref())
    }

    fn is_blocked(&self) -> bool {
        self.tenant.as_ref().is_some_and(|t| t.user.is_blocked())
    }

    fn rsp_hdr_recv_timeout(&self) -> Option<Duration> {
        self.rsp_hdr_recv_timeout
    }

    fn log_uri_max_chars(&self) -> Option<usize> {
        self.tenant
            .as_ref()
            .and_then(|t| t.user.log_uri_max_chars())
    }

    fn add_proto_banned(&self) {
        if let Some(t) = &self.tenant {
            t.forbidden_stats.add_proto_banned();
        }
    }
}

#[derive(Clone)]
pub(crate) struct StreamInspectTaskNotes {
    task_id: Uuid,
    pub(crate) client_addr: SocketAddr,
    pub(crate) server_addr: SocketAddr,
    worker_id: Option<usize>,
    user_ctx: Option<StreamInspectUserContext>,
    site_ctx: Option<StreamInspectSiteContext>,
    max_idle_count: usize,
}

impl StreamInspectTaskNotes {
    pub(crate) fn user(&self) -> Option<&Arc<User>> {
        self.user_ctx.as_ref().map(|ctx| &ctx.user)
    }

    pub(crate) fn tenant(&self) -> Option<&Arc<User>> {
        self.site_ctx.as_ref().and_then(|s| s.tenant())
    }

    pub(crate) fn is_blocked(&self) -> bool {
        self.user().is_some_and(|u| u.is_blocked())
            || self.site_ctx.as_ref().is_some_and(|s| s.is_blocked())
    }

    /// ICAP / detour client identity: visitor first, owner if no visitor.
    pub(crate) fn raw_username(&self) -> Option<&ArcStr> {
        self.user_ctx
            .as_ref()
            .and_then(|ctx| ctx.raw_user_name.as_ref())
            .or_else(|| self.site_ctx.as_ref().and_then(|s| s.raw_username()))
    }

    #[inline]
    pub(crate) fn task_id(&self) -> &Uuid {
        &self.task_id
    }

    fn from_task_notes(task_notes: &ServerTaskNotes, server_config: &impl ServerConfig) -> Self {
        StreamInspectTaskNotes {
            task_id: task_notes.id,
            client_addr: task_notes.client_addr(),
            server_addr: task_notes.server_addr(),
            worker_id: task_notes.worker_id(),
            user_ctx: task_notes
                .user_ctx()
                .map(StreamInspectUserContext::from_user_context),
            site_ctx: task_notes
                .site_ctx()
                .map(StreamInspectSiteContext::from_site_context),
            max_idle_count: task_notes.task_max_idle_count(server_config.task_max_idle_count()),
        }
    }

    #[inline]
    pub(crate) fn max_idle_count(&self) -> usize {
        self.max_idle_count
    }
}

#[derive(Clone, Copy)]
pub(crate) struct StreamInspectConnectNotes {
    pub(crate) client_addr: SocketAddr,
    pub(crate) server_addr: SocketAddr,
}

impl From<&EgressNotes> for StreamInspectConnectNotes {
    fn from(egress_notes: &EgressNotes) -> Self {
        StreamInspectConnectNotes {
            client_addr: egress_notes.tcp_connect_local_addr().unwrap(),
            server_addr: egress_notes.tcp_connect_peer_addr().unwrap(),
        }
    }
}

pub(crate) struct StreamInspectContext<SC: ServerConfig> {
    audit_handle: Arc<AuditHandle>,
    server_config: Arc<SC>,
    server_stats: ArcServerStats,
    server_quit_policy: Arc<ServerQuitPolicy>,
    idle_wheel: Arc<IdleWheel>,
    task_notes: StreamInspectTaskNotes,
    connect_notes: StreamInspectConnectNotes,
    inspection_depth: usize,
}

impl<SC: ServerConfig> Clone for StreamInspectContext<SC> {
    fn clone(&self) -> Self {
        StreamInspectContext {
            audit_handle: self.audit_handle.clone(),
            server_config: self.server_config.clone(),
            server_stats: self.server_stats.clone(),
            server_quit_policy: self.server_quit_policy.clone(),
            idle_wheel: self.idle_wheel.clone(),
            task_notes: self.task_notes.clone(),
            connect_notes: self.connect_notes,
            inspection_depth: self.inspection_depth,
        }
    }
}

impl<SC: ServerConfig> StreamInspectContext<SC> {
    pub(crate) fn new(
        audit_handle: Arc<AuditHandle>,
        server_config: Arc<SC>,
        server_stats: ArcServerStats,
        server_quit_policy: Arc<ServerQuitPolicy>,
        idle_wheel: Arc<IdleWheel>,
        task_notes: &ServerTaskNotes,
        egress_notes: &EgressNotes,
    ) -> Self {
        let task_notes =
            StreamInspectTaskNotes::from_task_notes(task_notes, server_config.as_ref());

        StreamInspectContext {
            audit_handle,
            server_config,
            server_stats,
            server_quit_policy,
            idle_wheel,
            task_notes,
            connect_notes: StreamInspectConnectNotes::from(egress_notes),
            inspection_depth: 0,
        }
    }

    #[inline]
    fn user(&self) -> Option<&User> {
        self.task_notes.user().map(|u| u.as_ref())
    }

    #[inline]
    fn tenant(&self) -> Option<&User> {
        self.task_notes.tenant().map(|t| t.as_ref())
    }

    fn user_cloned(&self) -> Option<Arc<User>> {
        self.task_notes.user().cloned()
    }

    fn tenant_cloned(&self) -> Option<Arc<User>> {
        self.task_notes.tenant().cloned()
    }

    #[inline]
    fn raw_user_name(&self) -> Option<&ArcStr> {
        self.task_notes.raw_username()
    }

    /// DPI flags: site context uses tenant only; otherwise visitor user.
    fn audit_proto_banned_if(&self, prohibit: impl Fn(&User) -> bool) -> bool {
        if let Some(site) = &self.task_notes.site_ctx {
            if let Some(tenant) = site.tenant()
                && prohibit(tenant)
            {
                site.add_proto_banned();
                return true;
            }
            return false;
        }
        if let Some(ctx) = &self.task_notes.user_ctx
            && prohibit(&ctx.user)
        {
            ctx.forbidden_stats.add_proto_banned();
            return true;
        }
        false
    }

    #[inline]
    pub(crate) fn server_task_id(&self) -> &Uuid {
        self.task_notes.task_id()
    }

    #[inline]
    fn server_force_quit(&self) -> bool {
        self.server_quit_policy.force_quit()
    }

    #[inline]
    fn server_offline(&self) -> bool {
        !self.server_stats.is_online()
    }

    #[inline]
    pub(crate) fn inspect_logger(&self) -> Option<&Logger> {
        self.audit_handle.inspect_logger()
    }

    #[inline]
    pub(crate) fn intercept_logger(&self) -> Option<&Logger> {
        self.audit_handle.intercept_logger()
    }

    pub(crate) fn apply_proxy_status_ident(&self, rsp: &mut HttpProxyClientResponse) {
        rsp.apply_proxy_status(
            self.audit_handle.no_proxy_status(),
            self.audit_handle.server_id(),
        );
    }

    pub(crate) fn idle_checker(&self) -> ServerIdleChecker {
        ServerIdleChecker::new(
            self.idle_wheel.clone(),
            self.user_cloned(),
            self.tenant_cloned(),
            self.max_idle_count(),
            self.server_quit_policy.clone(),
        )
    }

    pub(crate) fn protocol_inspector(
        &self,
        explicit_protocol: Option<MaybeProtocol>,
    ) -> ProtocolInspector {
        let mut inspector = ProtocolInspector::new(
            self.audit_handle.server_tcp_portmap(),
            self.audit_handle.client_tcp_portmap(),
        );
        if let Some(p) = explicit_protocol {
            inspector.push_protocol(p);
        }
        inspector
    }

    #[inline]
    pub(crate) fn max_idle_count(&self) -> usize {
        self.task_notes.max_idle_count()
    }

    #[inline]
    pub(crate) fn current_inspection_depth(&self) -> usize {
        self.inspection_depth
    }

    #[inline]
    fn increase_inspection_depth(&mut self) {
        self.inspection_depth += 1;
    }

    #[inline]
    pub(crate) fn tls_interception(&self) -> Option<TlsInterceptionContext> {
        self.audit_handle.tls_interception()
    }

    pub(crate) fn user_site_tls_client(&self) -> Option<&OpensslClientConfig> {
        self.task_notes
            .user_ctx
            .as_ref()
            .and_then(|v| v.user_site.as_ref())
            .and_then(|v| v.tls_client())
    }

    fn log_uri_max_chars(&self) -> usize {
        self.task_notes
            .site_ctx
            .as_ref()
            .and_then(|s| s.log_uri_max_chars())
            .or_else(|| {
                self.task_notes
                    .user_ctx
                    .as_ref()
                    .and_then(|cx| cx.user.log_uri_max_chars())
            })
            .unwrap_or_else(|| self.audit_handle.log_uri_max_chars())
    }

    #[inline]
    fn h1_interception(&self) -> &H1InterceptionConfig {
        self.audit_handle.h1_interception()
    }

    fn rsp_hdr_recv_timeout(&self, fallback: Duration) -> Duration {
        if let Some(site) = &self.task_notes.site_ctx {
            return site.rsp_hdr_recv_timeout().unwrap_or(fallback);
        }
        self.task_notes
            .user_ctx
            .as_ref()
            .and_then(|ctx| ctx.http_rsp_hdr_recv_timeout())
            .unwrap_or(fallback)
    }

    fn h1_rsp_hdr_recv_timeout(&self) -> Duration {
        self.rsp_hdr_recv_timeout(self.h1_interception().rsp_head_recv_timeout)
    }

    #[inline]
    fn h2_inspect_action(&self, host: &Host) -> ProtocolInspectAction {
        match self.audit_handle.h2_inspect_policy.check(host) {
            (true, policy_action) => policy_action,
            (false, missing_policy_action) => missing_policy_action,
        }
    }

    #[inline]
    fn h2_interception(&self) -> &H2InterceptionConfig {
        self.audit_handle.h2_interception()
    }

    fn h2_rsp_hdr_recv_timeout(&self) -> Duration {
        self.rsp_hdr_recv_timeout(self.h2_interception().rsp_head_recv_timeout)
    }

    #[inline]
    fn websocket_inspect_action(&self, host: &Host) -> ProtocolInspectAction {
        match self.audit_handle.websocket_inspect_policy.check(host) {
            (true, policy_action) => policy_action,
            (false, missing_policy_action) => missing_policy_action,
        }
    }

    #[inline]
    fn smtp_inspect_action(&self, host: &Host) -> ProtocolInspectAction {
        match self.audit_handle.smtp_inspect_policy.check(host) {
            (true, policy_action) => policy_action,
            (false, missing_policy_action) => missing_policy_action,
        }
    }

    #[inline]
    fn smtp_interception(&self) -> &SmtpInterceptionConfig {
        self.audit_handle.smtp_interception()
    }

    #[inline]
    fn imap_inspect_action(&self, host: &Host) -> ProtocolInspectAction {
        match self.audit_handle.imap_inspect_policy.check(host) {
            (true, policy_action) => policy_action,
            (false, missing_policy_action) => missing_policy_action,
        }
    }

    #[inline]
    fn imap_interception(&self) -> &ImapInterceptionConfig {
        self.audit_handle.imap_interception()
    }

    fn belongs_to_blocked_user(&self) -> bool {
        self.task_notes.is_blocked()
    }
}

pub(crate) enum StreamInspection<SC: ServerConfig> {
    End,
    StreamUnknown(stream::StreamInspectObject<SC>),
    StreamInspect(stream::StreamInspectObject<SC>),
    TlsModern(tls::TlsInterceptObject<SC>),
    #[cfg(tongsuo)]
    TlsTlcp(tls::TlsInterceptObject<SC>),
    StartTls(start_tls::StartTlsInterceptObject<SC>),
    H1(http::H1InterceptObject<SC>),
    H2(http::H2InterceptObject<SC>),
    Websocket(websocket::H1WebsocketInterceptObject<SC>),
    Smtp(smtp::SmtpInterceptObject<SC>),
    Imap(imap::ImapInterceptObject<SC>),
}

type BoxAsyncRead = Box<dyn AsyncRead + Send + Sync + Unpin + 'static>;
type BoxAsyncWrite = Box<dyn AsyncWrite + Send + Sync + Unpin + 'static>;
