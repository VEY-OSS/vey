// SPDX-License-Identifier: GPL-2.0-only
// SPDX-FileCopyrightText: 2026 VEY-OSS Developers.

#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>

const volatile __u32 load_pid = 0;
const volatile __u32 load_generation = 0;

struct socket_id {
	__u32 pid;
	__u32 generation;
	__u32 worker;
};

struct {
	__uint(type, BPF_MAP_TYPE_LRU_HASH);
	__uint(max_entries, 32768);
	__type(key, __u32);
	__type(value, struct socket_id);
	__uint(map_flags, BPF_F_NO_COMMON_LRU | BPF_F_RDONLY);
} conn_track SEC(".maps");

struct proc_info_key {
	__u32 pid;
	__u32 generation;
};

struct proc_info_value {
	__u32 count;
	__u32 invalid;
};

struct proc_info_result {
	const struct proc_info_key *k;
	struct proc_info_value *v;
};

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 32);
	__type(key, struct proc_info_key);
	__type(value, struct proc_info_value);
} proc_map SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_SOCKHASH);
	__uint(max_entries, 512);
	__type(key, struct socket_id);
	__type(value, __u64);
} socket_map SEC(".maps");

static long select_valid(void *map, const void *key, void *value, void *ctx)
{
	const struct proc_info_key *k = key;
    struct proc_info_value *v = value;
	struct proc_info_result *r = ctx;
	if (v->invalid) {
		return 0;
	}
	r->k = k;
	r->v = v;
	return 1;
}

SEC("sk_reuseport")
int udp_select_reuseport(struct sk_reuseport_md *ctx)
{
	__u32 hash = ctx->hash;

	struct socket_id *sock_id = bpf_map_lookup_elem(&conn_track, &hash);
	if (sock_id) {
		if (bpf_sk_select_reuseport(ctx, &socket_map, sock_id, 0) == 0)
			return SK_PASS;
		bpf_map_delete_elem(&conn_track, &hash);
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
		bpf_repeat(2) {
			if (bpf_sk_select_reuseport(ctx, &socket_map, &selected, 0) == 0) {
				bpf_map_update_elem(&conn_track, &hash, &selected, 0);
				return SK_PASS;
			}
			selected.worker += 1;
			if (selected.worker >= main_value->count) {
				selected.worker -= main_value->count;
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
		bpf_map_update_elem(&conn_track, &hash, &selected, 0);
		return SK_PASS;
	} else {
		// Mark the selected pid+generation as invalid
		__sync_fetch_and_add(&r.v->invalid, 1);
	}

	return SK_PASS;
}

char LICENSE[] SEC("license") = "GPL";
