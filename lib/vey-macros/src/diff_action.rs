/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! Generate `diff_action` from per-field reload behavior.
//!
//! Each field is compared at most once. A full `PartialEq` followed by another
//! check of the same field is not emitted. A field with custom comparators
//! must list every part that should be observable; those parts replace `!=`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Expr, Fields, Ident, Meta, Path, Type, TypePath};

struct ActionArgs {
    new_ty: Type,
    variant: Ident,
    mismatch: Expr,
    unchanged: Option<Expr>,
    changed: Option<Expr>,
    flags: Option<Type>,
}

struct FieldRule {
    cmp: Option<Ident>,
    by: Option<Path>,
    changed: Option<Expr>,
    flag: Option<Expr>,
}

struct FieldDiff {
    ident: Ident,
    cfgs: Vec<Attribute>,
    skip: bool,
    rules: Vec<FieldRule>,
}

pub(super) fn derive(input: DeriveInput) -> TokenStream {
    match derive_impl(&input) {
        Ok(expanded) => expanded,
        Err(err) => err.to_compile_error(),
    }
}

fn derive_impl(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "DiffAction can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "DiffAction requires named struct fields",
        ));
    };

    let args = parse_action_args(input)?;
    let action_ty = action_type(
        args.unchanged
            .as_ref()
            .or(args.changed.as_ref())
            .unwrap_or(&args.mismatch),
    )?;
    let field_diffs = fields
        .named
        .iter()
        .map(parse_field)
        .collect::<syn::Result<Vec<_>>>()?;

    let compares = args.unchanged.is_some() && args.changed.is_some();
    if !compares {
        for field in &field_diffs {
            if field.skip || !field.rules.is_empty() {
                return Err(syn::Error::new_spanned(
                    &field.ident,
                    "field diff rules require both `unchanged` and `changed` on diff_action",
                ));
            }
        }
    }
    if args.flags.is_some() && !compares {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "`flags` requires both `unchanged` and `changed`",
        ));
    }
    if let Some(changed) = &args.changed
        && args.flags.is_some()
        && !matches!(changed, Expr::Path(_))
    {
        return Err(syn::Error::new_spanned(
            changed,
            "when `flags` is set, `changed` must be the variant constructor path",
        ));
    }

    let new_ty = &args.new_ty;
    let variant = &args.variant;
    let mismatch = &args.mismatch;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let (binding, body) = if !compares {
        let action = args.unchanged.as_ref().or(args.changed.as_ref()).unwrap();
        (quote!(_new), quote!(#action))
    } else {
        let changed = args.changed.as_ref().unwrap();
        let unchanged = args.unchanged.as_ref().unwrap();
        (
            quote!(new),
            compare_body(&field_diffs, &args, changed, unchanged)?,
        )
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_generics #name #ty_generics #where_clause {
            pub(crate) fn diff_action(&self, new: &#new_ty) -> #action_ty {
                let #new_ty::#variant(#binding) = new else {
                    return #mismatch;
                };
                #body
            }
        }
    })
}

fn compare_body(
    fields: &[FieldDiff],
    args: &ActionArgs,
    changed: &Expr,
    unchanged: &Expr,
) -> syn::Result<TokenStream> {
    let mut escalates = Vec::new();
    let mut flag_checks = Vec::new();
    let mut defaults = Vec::new();
    let flags_mode = args.flags.is_some();

    for field in fields {
        if field.skip {
            continue;
        }
        let implicit = FieldRule {
            cmp: None,
            by: None,
            changed: None,
            flag: None,
        };
        let rules: &[FieldRule] = if field.rules.is_empty() {
            std::slice::from_ref(&implicit)
        } else {
            &field.rules
        };

        for rule in rules {
            if rule.flag.is_some() && !flags_mode {
                return Err(syn::Error::new_spanned(
                    &field.ident,
                    "field `flag` requires `flags` on diff_action",
                ));
            }
            let cond = condition(&field.ident, rule);
            let cfgs = &field.cfgs;
            if let Some(action) = &rule.changed {
                escalates.push(quote! {
                    #(#cfgs)*
                    if #cond {
                        return #action;
                    }
                });
            } else if let Some(flag) = &rule.flag {
                flag_checks.push(quote! {
                    #(#cfgs)*
                    if #cond {
                        flags |= #flag;
                        changed = true;
                    }
                });
            } else if flags_mode {
                defaults.push(quote! {
                    #(#cfgs)*
                    if #cond {
                        changed = true;
                    }
                });
            } else {
                defaults.push(quote! {
                    #(#cfgs)*
                    if #cond {
                        return #changed;
                    }
                });
            }
        }
    }

    let tail = if flags_mode {
        let flags_ty = args.flags.as_ref().unwrap();
        let ctor = changed;
        let flags_binding = if flag_checks.is_empty() {
            quote! { let flags = #flags_ty::empty(); }
        } else {
            quote! { let mut flags = #flags_ty::empty(); }
        };
        quote! {
            #flags_binding
            let mut changed = false;
            #(#flag_checks)*
            #(#defaults)*
            if changed {
                #ctor(flags.bits())
            } else {
                #unchanged
            }
        }
    } else {
        quote! {
            #(#defaults)*
            #unchanged
        }
    };

    Ok(quote! {
        #(#escalates)*
        #tail
    })
}

fn condition(field: &Ident, rule: &FieldRule) -> TokenStream {
    match (&rule.cmp, &rule.by) {
        (Some(method), None) => quote! { self.#field.#method(&new.#field) },
        (None, Some(path)) => quote! { #path(&self.#field, &new.#field) },
        (None, None) => quote! { self.#field != new.#field },
        (Some(_), Some(_)) => unreachable!("cmp and by are rejected while parsing"),
    }
}

fn parse_action_args(input: &DeriveInput) -> syn::Result<ActionArgs> {
    let mut found = None;
    for attr in &input.attrs {
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        if !list.path.is_ident("diff_action") {
            continue;
        }
        if found.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "only one #[diff_action(...)] attribute is allowed",
            ));
        }
        found = Some(parse_action_list(attr)?);
    }
    found.ok_or_else(|| {
        syn::Error::new_spanned(
            &input.ident,
            "missing #[diff_action(new = ..., variant = ..., mismatch = ..., unchanged = ... and/or changed = ...)]",
        )
    })
}

fn parse_action_list(attr: &Attribute) -> syn::Result<ActionArgs> {
    let mut new_ty = None;
    let mut variant = None;
    let mut mismatch = None;
    let mut unchanged = None;
    let mut changed = None;
    let mut flags = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("new") {
            new_ty = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("variant") {
            variant = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("mismatch") {
            mismatch = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("unchanged") {
            unchanged = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("changed") {
            changed = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("flags") {
            flags = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected one of new, variant, mismatch, unchanged, changed, flags"))
        }
    })?;

    let missing =
        |name: &str| syn::Error::new_spanned(attr, format!("#[diff_action] is missing `{name}`"));
    if unchanged.is_none() && changed.is_none() {
        return Err(syn::Error::new_spanned(
            attr,
            "#[diff_action] needs `unchanged`, `changed`, or both",
        ));
    }
    Ok(ActionArgs {
        new_ty: new_ty.ok_or_else(|| missing("new"))?,
        variant: variant.ok_or_else(|| missing("variant"))?,
        mismatch: mismatch.ok_or_else(|| missing("mismatch"))?,
        unchanged,
        changed,
        flags,
    })
}

fn parse_field(field: &syn::Field) -> syn::Result<FieldDiff> {
    let ident = field
        .ident
        .clone()
        .ok_or_else(|| syn::Error::new_spanned(field, "DiffAction requires named struct fields"))?;
    let cfgs = field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
        .cloned()
        .collect();

    let mut skip = false;
    let mut rules = Vec::new();
    for attr in &field.attrs {
        if !attr.path().is_ident("diff") {
            continue;
        }
        let rule = parse_field_rule(attr)?;
        if rule.skip {
            if skip || !rules.is_empty() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "#[diff(skip)] cannot be combined with other diff rules",
                ));
            }
            skip = true;
        } else {
            if skip {
                return Err(syn::Error::new_spanned(
                    attr,
                    "#[diff(skip)] cannot be combined with other diff rules",
                ));
            }
            rules.push(rule.rule.unwrap());
        }
    }

    Ok(FieldDiff {
        ident,
        cfgs,
        skip,
        rules,
    })
}

struct ParsedRule {
    skip: bool,
    rule: Option<FieldRule>,
}

fn parse_field_rule(attr: &Attribute) -> syn::Result<ParsedRule> {
    let mut skip = false;
    let mut cmp = None;
    let mut by = None;
    let mut changed = None;
    let mut flag = None;
    let mut saw_key = false;

    attr.parse_nested_meta(|meta| {
        saw_key = true;
        if meta.path.is_ident("skip") {
            skip = true;
            Ok(())
        } else if meta.path.is_ident("cmp") {
            cmp = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("by") {
            by = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("changed") {
            changed = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("flag") {
            flag = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected one of skip, cmp, by, changed, flag"))
        }
    })?;

    if !saw_key {
        return Err(syn::Error::new_spanned(
            attr,
            "#[diff] needs skip, cmp, by, changed, or flag",
        ));
    }
    if skip && (cmp.is_some() || by.is_some() || changed.is_some() || flag.is_some()) {
        return Err(syn::Error::new_spanned(
            attr,
            "#[diff(skip)] cannot be combined with other keys",
        ));
    }
    if cmp.is_some() && by.is_some() {
        return Err(syn::Error::new_spanned(
            attr,
            "#[diff] accepts only one of `cmp` and `by`",
        ));
    }
    if changed.is_some() && flag.is_some() {
        return Err(syn::Error::new_spanned(
            attr,
            "#[diff] `changed` returns immediately, so it cannot also set `flag`",
        ));
    }
    if skip {
        return Ok(ParsedRule {
            skip: true,
            rule: None,
        });
    }
    Ok(ParsedRule {
        skip: false,
        rule: Some(FieldRule {
            cmp,
            by,
            changed,
            flag,
        }),
    })
}

fn action_type(expr: &Expr) -> syn::Result<Type> {
    let (qself, path) = match expr {
        Expr::Path(path) => (path.qself.clone(), &path.path),
        Expr::Call(call) => match &*call.func {
            Expr::Path(path) => (path.qself.clone(), &path.path),
            _ => {
                return Err(syn::Error::new_spanned(
                    expr,
                    "diff action must be an enum variant",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                expr,
                "diff action must be an enum variant",
            ));
        }
    };
    if path.segments.len() < 2 {
        return Err(syn::Error::new_spanned(
            expr,
            "diff action must be a path to an enum variant",
        ));
    }
    let mut path = path.clone();
    path.segments.pop().unwrap();
    // `Diff::Update` stores the `::` on the preceding segment. Drop it so the
    // type path is `Diff`, not `Diff::`.
    path.segments.pop_punct();
    Ok(Type::Path(TypePath {
        qself,
        path,
        attrs: Vec::new(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn split_field_is_compared_once_per_part() {
        let input: DeriveInput = parse_quote! {
            #[diff_action(
                new = AnyCfg,
                variant = Quic,
                mismatch = Diff::Spawn,
                unchanged = Diff::Same,
                changed = Diff::Update,
                flags = Flags,
            )]
            struct QuicCfg {
                #[diff(cmp = need_respawn, changed = Diff::Respawn)]
                #[diff(cmp = need_reload, flag = Flags::LISTEN)]
                listen: Listen,
                #[diff(flag = Flags::FILTER)]
                filter: u8,
                name: u8,
            }
        };
        let expanded = derive(input).to_string();
        assert_eq!(expanded.matches("need_respawn").count(), 1, "{expanded}");
        assert_eq!(expanded.matches("need_reload").count(), 1, "{expanded}");
        assert!(!expanded.contains(".eq("), "{expanded}");
        assert!(expanded.contains("flags . bits ()"), "{expanded}");
    }

    #[test]
    fn invalid_target_is_a_compile_error() {
        let input: DeriveInput = parse_quote! {
            enum Config { A }
        };
        let expanded = derive(input).to_string();
        assert!(expanded.contains("compile_error"), "{expanded}");
    }
}
