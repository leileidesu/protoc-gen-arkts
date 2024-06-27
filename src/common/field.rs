 /**
  * Copyright 2024 ByteDance and/or its affiliates
  *
  * Original Files：protoc-gen-ts (https://github.com/thesayyn/protoc-gen-ts)
  * Copyright (c) 2024 Sahin Yort
  * SPDX-License-Identifier: MIT 
 */

use crate::{
    context::{Context, Syntax},
    descriptor::FieldDescriptorProto,
    runtime::Runtime,
};
use swc_common::DUMMY_SP;
use swc_ecma_ast::{
    ArrayLit, BinaryOp, ClassMember, ClassProp, Expr, PropName, TsArrayType, TsEntityName,
    TsKeywordType, TsType, TsTypeAnn, TsTypeParamInstantiation, TsTypeRef,
};
use swc_ecma_utils::{quote_ident, quote_str};

pub type AccessNormalizerFn = fn(expr: &Expr) -> Expr;

pub fn map_to_string_normalizer(expr: &Expr) -> Expr {
    crate::call_expr!(
        crate::member_expr_bare!(expr.clone(), "map"),
        vec![crate::expr_or_spread!(crate::arrow_func_short!(
            crate::call_expr!(crate::member_expr!("v", "toString")),
            vec![crate::pat_ident!(quote_ident!("v"))]
        ))]
    )
}

pub fn to_string_normalizer(expr: &Expr) -> Expr {
    crate::call_expr!(crate::member_expr_bare!(expr.clone(), "toString"))
}

pub type FieldAccessorFn = fn(field: &FieldDescriptorProto) -> Expr;

pub fn this_field_member(field: &FieldDescriptorProto) -> Expr {
    crate::member_expr!("this", field.name())
}

pub fn bare_field_member(field: &FieldDescriptorProto) -> Expr {
    Expr::Ident(quote_ident!(field.name()))
}

pub fn static_field_member(_field: &FieldDescriptorProto) -> Expr {
    Expr::Ident(quote_ident!("r"))
}

impl FieldDescriptorProto {
    pub fn into_accessor(&self, ctx: &Context) -> FieldAccessorFn {
        if self.is_repeated() && self.is_map(ctx) {
            bare_field_member
        } else if self.is_repeated() && !self.is_packed(ctx) {
            bare_field_member
        } else {
            this_field_member
        }
    }
}

impl FieldDescriptorProto {
    pub fn prop_name(&self) -> String {
        if self.has_oneof_index() {
            format!("_{}", self.name())
        } else {
            self.name().to_string()
        }
    }

    pub fn default_value_bin_expr(&self, ctx: &mut Context, accessor: FieldAccessorFn) -> Expr {
        let neq_undefined_check = crate::bin_expr!(
            accessor(self),
            quote_ident!("undefined").into(),
            BinaryOp::NotEqEq
        );

        // for oneof field we have to serialize the value unconditionally
        // even if the value is the default.
        if self.has_oneof_index() {
            return neq_undefined_check;
        }

        let presence_check = if self.is_map(ctx) {
            crate::bin_expr!(
                neq_undefined_check,
                crate::bin_expr!(
                    crate::member_expr_bare!(accessor(self), "size"),
                    Expr::Lit(crate::lit_num!(0)),
                    BinaryOp::NotEqEq
                )
            )
        } else if (self.is_bytes() && ctx.syntax == &Syntax::Proto3) || self.is_repeated() {
            crate::bin_expr!(
                neq_undefined_check,
                crate::bin_expr!(
                    crate::member_expr_bare!(accessor(self), "length"),
                    Expr::Lit(crate::lit_num!(0)),
                    BinaryOp::NotEqEq
                )
            )
        } else {
            neq_undefined_check
        };

        if ctx.syntax == &Syntax::Proto3 {
            let default_expr = self.proto3_default(ctx);
            if let Some(default_expr) = default_expr {
                crate::chain_bin_exprs_and!(
                    presence_check,
                    crate::bin_expr!(accessor(self), default_expr, BinaryOp::NotEqEq)
                )
            } else {
                presence_check
            }
        } else {
            presence_check
        }
    }

    pub fn proto3_default(&self, ctx: &mut Context) -> Option<Expr> {
        if self.is_repeated() || self.has_oneof_index() {
            return None;
        }
        if self.is_string() {
            Some(crate::lit_str!("").into())
        } else if self.is_bigint() {
            Some(crate::call_expr!(
                quote_ident!("BigInt").into(),
                vec![crate::expr_or_spread!(crate::lit_num!(0).into())]
            ))
        } else if self.is_number() {
            Some(crate::lit_num!(0).into())
        } else if self.is_booelan() {
            Some(crate::lit_bool!(false).into())
        } else if self.is_enum() {
            Some(crate::lit_num!(ctx.get_leading_enum_member(self.type_name())).into())
        } else {
            None
        }
    }

    pub fn default_value_expr(&self, ctx: &mut Context, include_message: bool) -> Expr {
        if self.has_oneof_index() {
            return Expr::Ident(quote_ident!("undefined"));
        }

        if ctx.options.with_sendable {
            ctx.get_sendable_import(&ctx.options.sendable_packege)
        }
        if self.is_map(ctx) {
            if ctx.options.with_sendable {
                crate::new_expr!(crate::member_expr_bare!(Expr::Ident(quote_ident!("collections")), "Map"))
            } else {
                crate::new_expr!(Expr::Ident(quote_ident!("Map")))
            }
        } else if self.is_repeated() {
            if ctx.options.with_sendable {
                crate::new_expr!(crate::member_expr_bare!(Expr::Ident(quote_ident!("collections")), "Array"))
            } else {
                Expr::Array(ArrayLit {
                    elems: vec![],
                    span: DUMMY_SP,
                })
            }
           
        } else if self.is_enum() {
            crate::lit_num!(ctx.get_leading_enum_member(self.type_name())).into()
        } else if self.is_message() && include_message {
            crate::new_expr!(ctx.lazy_type_ref(self.type_name()).into())
        } else if self.is_bytes() {
            if ctx.options.with_sendable {
                crate::new_expr!(crate::member_expr_bare!(Expr::Ident(quote_ident!("collections")), "Uint8Array"))
            } else {
                crate::new_expr!(quote_ident!("Uint8Array").into())
            }
            
        } else if self.is_string() {
            quote_str!(self.default_value()).into()
        } else if self.is_bigint() {
            crate::call_expr!(
                quote_ident!("BigInt").into(),
                vec![crate::expr_or_spread!(crate::lit_num!(0).into())]
            )
        } else if self.is_number() {
            crate::lit_num!(self
                .default_value
                .clone()
                .unwrap_or("0".to_string())
                .parse::<f64>()
                .expect("can not parse the default"))
            .into()
        } else if self.is_booelan() {
            crate::lit_bool!(self
                .default_value
                .clone()
                .unwrap_or("false".to_string())
                .parse::<bool>()
                .expect("can not parse the default"))
            .into()
        } else {
            if self.is_optional() || self.proto3_optional() {
                quote_ident!("undefined").into()
            } else {
                crate::new_expr!(ctx.lazy_type_ref(self.type_name()).into())
            }
        }
    }
    fn ts_type(&self, ctx: &mut Context) -> Option<TsType> {
        let mut ts_type: Option<TsType> = None;

        if let Some(typref) = self.type_ref(ctx) {
            ts_type = Some(TsType::TsTypeRef(typref))
        }

        if let Some(kind) = self.keyword_type_kind() {
            ts_type = Some(TsType::TsKeywordType(TsKeywordType {
                span: DUMMY_SP,
                kind,
            }))
        }

        if self.is_bytes() && ctx.options.with_sendable {
            ts_type = Some(TsType::TsTypeRef(TsTypeRef {
                span: DUMMY_SP,
                type_name: TsEntityName::Ident(quote_ident!(format!("{}.Uint8Array", "collections"))),
                type_params: None,
            }));
        }

        if self.is_repeated() && self.is_map(ctx) {
            let descriptor = ctx
                .get_map_type(self.type_name())
                .expect(format!("can not find the map type {}", self.type_name()).as_str());
            let mut type_name_ident = quote_ident!("Map");
            if ctx.options.with_sendable {
                type_name_ident = quote_ident!(format!("{}.Map", "collections"))
            }
            ts_type = Some(TsType::TsTypeRef(TsTypeRef {
                span: DUMMY_SP,
                type_name: TsEntityName::Ident(type_name_ident),
                type_params: Some(Box::new(TsTypeParamInstantiation {
                    span: DUMMY_SP,
                    params: descriptor
                        .field
                        .into_iter()
                        .map(|x: FieldDescriptorProto| {
                            x.type_annotation(ctx)
                                .expect("expect map fields to have corresponding type")
                                .type_ann
                        })
                        .collect(),
                })),
            }))
        } else if ts_type.is_some() && self.is_repeated() && !self.is_map(ctx) {
           
            if ctx.options.with_sendable {
                ts_type = Some(TsType::TsTypeRef(TsTypeRef {
                    span: DUMMY_SP,
                    type_name: TsEntityName::Ident(quote_ident!(format!("{}.Array", "collections"))),
                    type_params: Some(Box::new(TsTypeParamInstantiation {
                        span: DUMMY_SP,
                        params: vec![
                            Box::new(ts_type.unwrap())
                        ]
                    })),
                }))
            } else {
                ts_type = Some(TsType::TsArrayType(TsArrayType {
                    elem_type: Box::new(ts_type.unwrap()),
                    span: DUMMY_SP,
                }))
            }
           
        }

        ts_type
    }
    pub fn type_annotation(&self, ctx: &mut Context) -> Option<Box<TsTypeAnn>> {
        Some(Box::new(TsTypeAnn {
            span: DUMMY_SP,
            type_ann: Box::new(self.ts_type(ctx)?),
        }))
    }
    pub fn nullish_type_annotation(&self, ctx: &mut Context) -> Option<Box<TsTypeAnn>> {
        Some(Box::new(crate::type_union!(
            self.ts_type(ctx)?,
            crate::undefined_type!()
        )))
    }

    pub fn print_prop<T: Runtime>(&self, ctx: &mut Context, _runtime: &T) -> ClassMember {
        let mut value: Option<Box<Expr>> = None;
        if ctx.syntax == &Syntax::Proto3 || self.is_repeated() || self.is_map(&ctx) || !self.is_optional() {
            value = Some(Box::new(self.default_value_expr(ctx, false)))
        }
        let mut key_ident = quote_ident!(self.prop_name());
        if self.is_optional() {
            key_ident = crate::quote_ident_optional!(self.prop_name())
        }

        ClassMember::ClassProp(ClassProp {
            span: DUMMY_SP,
            key: PropName::Ident(key_ident),
            value,
            type_ann: self.type_annotation(ctx),
            declare: false,
            is_static: false,
            decorators: vec![],
            accessibility: None,
            is_abstract: false,
            is_optional: false,
            is_override: false,
            readonly: false,
            definite: false,
        })
    }
}
