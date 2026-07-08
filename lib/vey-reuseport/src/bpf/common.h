// SPDX-License-Identifier: GPL-2.0-only
// SPDX-FileCopyrightText: 2026 VEY-OSS Developers.

#ifndef VEY_REUSEPORT_COMMON_H
#define VEY_REUSEPORT_COMMON_H 1

#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>

const volatile __s32 load_pid = 0;
const volatile __u16 load_generation = 0;

struct socket_id {
	__s32 pid;
	__u16 generation;
	__u16 worker;
};

struct proc_info_key {
	__s32 pid;
	__u16 generation;
	__u16 padding;
};

struct proc_info_value {
	__u32 invalid;
	__u16 count;
	__u16 padding;
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

#endif
