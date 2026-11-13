/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

extern crate proc_macro;

use syn::{DeriveInput, parse_macro_input};

mod any_config;
mod diff_action;

#[proc_macro_derive(AnyConfig, attributes(def_fn, def_async_fn))]
pub fn any_config(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let expanded = any_config::derive(input);
    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_derive(DiffAction, attributes(diff_action, diff))]
pub fn diff_action(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    proc_macro::TokenStream::from(diff_action::derive(input))
}
