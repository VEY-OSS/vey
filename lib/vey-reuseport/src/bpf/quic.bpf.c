// SPDX-License-Identifier: GPL-2.0-only
// SPDX-FileCopyrightText: 2026 VEY-OSS Developers.

#include "common.h"

#include <linux/udp.h>

#define QUIC_PACKET_LONG 0x80
#define COOKIE_LEN 8

struct {
	__uint(type, BPF_MAP_TYPE_LRU_HASH);
	__uint(max_entries, 65536);
	__type(key, __u32);
	__type(value, struct socket_id);
	__uint(map_flags, BPF_F_NO_COMMON_LRU | BPF_F_RDONLY);
} udp_conn_track SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 2048);
	__type(key, __u64);
	__type(value, struct socket_id);
	__uint(map_flags, 0);
} quic_conn_track SEC(".maps");

SEC("sk_reuseport")
int quic_select_reuseport(struct sk_reuseport_md *ctx)
{
	unsigned char *start = ctx->data;
	unsigned char *end = ctx->data_end;

	if (start + sizeof(struct udphdr) + 1 > end) {
		goto fail;
	}
	unsigned char *packet = start + sizeof(struct udphdr);

	__u32 hash = ctx->hash;
	unsigned char *dcid = NULL;
	if (packet[0] & QUIC_PACKET_LONG) {
		struct socket_id *sock_id = bpf_map_lookup_elem(&udp_conn_track, &hash);
		if (sock_id) {
			if (bpf_sk_select_reuseport(ctx, &socket_map, sock_id, 0) == 0) {
				return SK_PASS;
			}
			bpf_map_delete_elem(&udp_conn_track, &hash);
		}
	} else {
		dcid = packet + 1;
		if (dcid + COOKIE_LEN > end) {
			goto fail;
		}

		__u64 cookie = (__u64)dcid[7] | \
			((__u64)dcid[6] << 8)     | \
			((__u64)dcid[5] << 16)    | \
			((__u64)dcid[4] << 24)    | \
			((__u64)dcid[3] << 32)    | \
			((__u64)dcid[2] << 40)    | \
			((__u64)dcid[1] << 48)    | \
			((__u64)dcid[0] << 56);

		struct socket_id *sock_id = bpf_map_lookup_elem(&quic_conn_track, &cookie);
		if (sock_id) {
			if (bpf_sk_select_reuseport(ctx, &socket_map, sock_id, 0) == 0)
				return SK_PASS;
			bpf_map_delete_elem(&quic_conn_track, &cookie);
		}

		goto fail;
	}

	__u32 random = bpf_get_prandom_u32();
	struct socket_id selected = {};

	struct proc_info_key main_key = {
		.pid = load_pid,
		.generation = load_generation,
	};
	struct proc_info_value *main_value = bpf_map_lookup_elem(&proc_map, &main_key);
	if (main_value && !main_value->invalid) {
		selected.pid = load_pid;
		selected.generation = load_generation;
		selected.worker = random % main_value->count;

		if (bpf_sk_select_reuseport(ctx, &socket_map, &selected, 0) == 0) {
			bpf_map_update_elem(&udp_conn_track, &hash, &selected, 0);
			return SK_PASS;
		}

		if (main_value->count > 1) {
			// try another one in the same pid+generation group
			selected.worker += 1;
			if (selected.worker >= main_value->count) {
				selected.worker -= main_value->count;
			}
			if (bpf_sk_select_reuseport(ctx, &socket_map, &selected, 0) == 0) {
				bpf_map_update_elem(&udp_conn_track, &hash, &selected, 0);
				return SK_PASS;
			}
		}

		// Mark the selected pid+generation as invalid
		__sync_fetch_and_add(&main_value->invalid, 1);
	}

	// try again for other valid pid+generation
	struct proc_info_result r = {};
	bpf_for_each_map_elem(&proc_map, select_valid, &r, 0);
	if (!r.k || !r.v) {
		return SK_PASS;
	}
	selected.pid = r.k->pid;
	selected.generation = r.k->generation;
	selected.worker = random % r.v->count;
	if (bpf_sk_select_reuseport(ctx, &socket_map, &selected, 0) == 0) {
		bpf_map_update_elem(&udp_conn_track, &hash, &selected, 0);
		return SK_PASS;
	} else {
		// Mark the selected pid+generation as invalid
		__sync_fetch_and_add(&r.v->invalid, 1);
	}

fail:
	return SK_PASS;
}

char LICENSE[] SEC("license") = "GPL";
