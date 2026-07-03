// SPDX-License-Identifier: GPL-2.0-only
// SPDX-FileCopyrightText: 2026 VEY-OSS Developers.

#include "common.h"

SEC("sk_reuseport/migrate")
int tcp_select_reuseport(struct sk_reuseport_md *ctx)
{
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
		return SK_PASS;
	} else {
		// Mark the selected pid+generation as invalid
		__sync_fetch_and_add(&r.v->invalid, 1);
	}

	return SK_PASS;
}

char LICENSE[] SEC("license") = "GPL";
