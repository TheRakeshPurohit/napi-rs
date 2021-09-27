#[macro_use]
pub mod attrs;

use std::cell::Cell;
use std::collections::HashMap;
use std::str::Chars;

use attrs::{BindgenAttr, BindgenAttrs};

use convert_case::{Case, Casing};
use napi_derive_backend::{
  BindgenResult, CallbackArg, Diagnostic, FnKind, FnSelf, Napi, NapiEnum, NapiEnumVariant, NapiFn,
  NapiFnArgKind, NapiImpl, NapiItem, NapiStruct, NapiStructField,
};
use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::ToTokens;
use syn::parse::{Parse, ParseStream, Result as SynResult};
use syn::{Attribute, Signature, Visibility};

use crate::parser::attrs::{check_recorded_struct_for_impl, record_struct};

struct AnyIdent(Ident);

impl Parse for AnyIdent {
  fn parse(input: ParseStream) -> SynResult<Self> {
    input.step(|cursor| match cursor.ident() {
      Some((ident, remaining)) => Ok((AnyIdent(ident), remaining)),
      None => Err(cursor.error("expected an identifier")),
    })
  }
}

impl Parse for BindgenAttrs {
  fn parse(input: ParseStream) -> SynResult<Self> {
    let mut attrs = BindgenAttrs::default();
    if input.is_empty() {
      return Ok(attrs);
    }

    let opts = syn::punctuated::Punctuated::<_, syn::token::Comma>::parse_terminated(input)?;
    attrs.attrs = opts.into_iter().map(|c| (Cell::new(false), c)).collect();
    Ok(attrs)
  }
}

impl Parse for BindgenAttr {
  fn parse(input: ParseStream) -> SynResult<Self> {
    let original = input.fork();
    let attr: AnyIdent = input.parse()?;
    let attr = attr.0;
    let attr_span = attr.span();
    let attr_string = attr.to_string();
    let raw_attr_string = format!("r#{}", attr_string);

    macro_rules! parsers {
      ($(($name:ident, $($contents:tt)*),)*) => {
        $(
          if attr_string == stringify!($name) || raw_attr_string == stringify!($name) {
            parsers!(
              @parser
              $($contents)*
            );
          }
        )*
      };

      (@parser $variant:ident(Span)) => ({
        return Ok(BindgenAttr::$variant(attr_span));
      });

      (@parser $variant:ident(Span, Ident)) => ({
        input.parse::<Token![=]>()?;
        let ident = input.parse::<AnyIdent>()?.0;
        return Ok(BindgenAttr::$variant(attr_span, ident))
      });

      (@parser $variant:ident(Span, Option<Ident>)) => ({
        if input.parse::<Token![=]>().is_ok() {
          let ident = input.parse::<AnyIdent>()?.0;
          return Ok(BindgenAttr::$variant(attr_span, Some(ident)))
        } else {
          return Ok(BindgenAttr::$variant(attr_span, None));
        }
      });

        (@parser $variant:ident(Span, syn::Path)) => ({
            input.parse::<Token![=]>()?;
            return Ok(BindgenAttr::$variant(attr_span, input.parse()?));
        });

        (@parser $variant:ident(Span, syn::Expr)) => ({
            input.parse::<Token![=]>()?;
            return Ok(BindgenAttr::$variant(attr_span, input.parse()?));
        });

        (@parser $variant:ident(Span, String, Span)) => ({
          input.parse::<Token![=]>()?;
          let (val, span) = match input.parse::<syn::LitStr>() {
            Ok(str) => (str.value(), str.span()),
            Err(_) => {
              let ident = input.parse::<AnyIdent>()?.0;
              (ident.to_string(), ident.span())
            }
          };
          return Ok(BindgenAttr::$variant(attr_span, val, span))
        });

        (@parser $variant:ident(Span, Vec<String>, Vec<Span>)) => ({
          input.parse::<Token![=]>()?;
          let (vals, spans) = match input.parse::<syn::ExprArray>() {
            Ok(exprs) => {
              let mut vals = vec![];
              let mut spans = vec![];

              for expr in exprs.elems.iter() {
                if let syn::Expr::Lit(syn::ExprLit {
                  lit: syn::Lit::Str(ref str),
                  ..
                }) = expr {
                  vals.push(str.value());
                  spans.push(str.span());
                } else {
                  return Err(syn::Error::new(expr.span(), "expected string literals"));
                }
              }

              (vals, spans)
            },
            Err(_) => {
              let ident = input.parse::<AnyIdent>()?.0;
              (vec![ident.to_string()], vec![ident.span()])
            }
          };
          return Ok(BindgenAttr::$variant(attr_span, vals, spans))
        });
      }

    attrgen!(parsers);

    Err(original.error("unknown attribute"))
  }
}

pub trait ConvertToAST {
  fn convert_to_ast(&mut self, opts: BindgenAttrs) -> BindgenResult<Napi>;
}

pub trait ParseNapi {
  fn parse_napi(&mut self, tokens: &mut TokenStream, opts: BindgenAttrs) -> BindgenResult<Napi>;
}

fn get_ty(mut ty: &syn::Type) -> &syn::Type {
  while let syn::Type::Group(g) = ty {
    ty = &g.elem;
  }

  ty
}

fn replace_self(ty: syn::Type, self_ty: Option<&Ident>) -> syn::Type {
  let self_ty = match self_ty {
    Some(i) => i,
    None => return ty,
  };
  let path = match get_ty(&ty) {
    syn::Type::Path(syn::TypePath { qself: None, path }) => path.clone(),
    other => return other.clone(),
  };
  let new_path = if path.segments.len() == 1 && path.segments[0].ident == "Self" {
    self_ty.clone().into()
  } else {
    path
  };
  syn::Type::Path(syn::TypePath {
    qself: None,
    path: new_path,
  })
}

/// Extracts the last ident from the path
fn extract_path_ident(path: &syn::Path) -> BindgenResult<Ident> {
  for segment in path.segments.iter() {
    match segment.arguments {
      syn::PathArguments::None => {}
      _ => bail_span!(path, "paths with type parameters are not supported yet"),
    }
  }

  match path.segments.last() {
    Some(value) => Ok(value.ident.clone()),
    None => {
      bail_span!(path, "empty idents are not supported");
    }
  }
}

fn extract_fn_types(
  arguments: &syn::PathArguments,
) -> BindgenResult<(Vec<syn::Type>, Option<syn::Type>)> {
  match arguments {
    // <T: Fn>
    syn::PathArguments::None => Ok((vec![], None)),
    syn::PathArguments::AngleBracketed(_) => {
      bail_span!(arguments, "use parentheses for napi callback trait")
    }
    syn::PathArguments::Parenthesized(arguments) => {
      let args = arguments.inputs.iter().cloned().collect::<Vec<_>>();

      let ret = match &arguments.output {
        syn::ReturnType::Type(_, ret_ty) => {
          let ret_ty = &**ret_ty;
          match ret_ty {
            syn::Type::Path(syn::TypePath {
              qself: None,
              ref path,
            }) if path.segments.len() == 1 => {
              let segment = path.segments.first().unwrap();

              if segment.ident != "Result" {
                bail_span!(ret_ty, "The return type of callback can only be `Result`");
              } else {
                match &segment.arguments {
                  syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
                    args,
                    ..
                  }) => {
                    // fast test
                    if args.to_token_stream().to_string() == "()" {
                      None
                    } else {
                      let ok_arg = args.first().unwrap();
                      match ok_arg {
                        syn::GenericArgument::Type(ty) => Some(ty.clone()),
                        _ => bail_span!(ok_arg, "unsupported generic type"),
                      }
                    }
                  }
                  _ => {
                    bail_span!(segment, "Too many arguments")
                  }
                }
              }
            }
            _ => bail_span!(ret_ty, "The return type of callback can only be `Result`"),
          }
        }
        _ => {
          bail_span!(
            arguments,
            "The return type of callback can only be `Result`. Try with `Result<()>`"
          );
        }
      };

      Ok((args, ret))
    }
  }
}

fn get_expr(mut expr: &syn::Expr) -> &syn::Expr {
  while let syn::Expr::Group(g) = expr {
    expr = &g.expr;
  }

  expr
}

/// Extract the documentation comments from a Vec of attributes
fn extract_doc_comments(attrs: &[syn::Attribute]) -> Vec<String> {
  attrs
    .iter()
    .filter_map(|a| {
      // if the path segments include an ident of "doc" we know this
      // this is a doc comment
      if a.path.segments.iter().any(|s| s.ident == "doc") {
        Some(
          // We want to filter out any Puncts so just grab the Literals
          a.tokens.clone().into_iter().filter_map(|t| match t {
            TokenTree::Literal(lit) => {
              let quoted = lit.to_string();
              Some(try_unescape(&quoted).unwrap_or(quoted))
            }
            _ => None,
          }),
        )
      } else {
        None
      }
    })
    //Fold up the [[String]] iter we created into Vec<String>
    .fold(vec![], |mut acc, a| {
      acc.extend(a);
      acc
    })
}

// Unescapes a quoted string. char::escape_debug() was used to escape the text.
fn try_unescape(s: &str) -> Option<String> {
  if s.is_empty() {
    return Some(String::new());
  }
  let mut result = String::with_capacity(s.len());
  let mut chars = s.chars();
  for i in 0.. {
    let c = match chars.next() {
      Some(c) => c,
      None => {
        if result.ends_with('"') {
          result.pop();
        }
        return Some(result);
      }
    };
    if i == 0 && c == '"' {
      // ignore it
    } else if c == '\\' {
      let c = chars.next()?;
      match c {
        't' => result.push('\t'),
        'r' => result.push('\r'),
        'n' => result.push('\n'),
        '\\' | '\'' | '"' => result.push(c),
        'u' => {
          if chars.next() != Some('{') {
            return None;
          }
          let (c, next) = unescape_unicode(&mut chars)?;
          result.push(c);
          if next != '}' {
            return None;
          }
        }
        _ => return None,
      }
    } else {
      result.push(c);
    }
  }
  None
}

fn unescape_unicode(chars: &mut Chars) -> Option<(char, char)> {
  let mut value = 0;
  for i in 0..7 {
    let c = chars.next()?;
    let num = match c {
      '0'..='9' => c as u32 - '0' as u32,
      'a'..='f' => c as u32 - 'a' as u32,
      'A'..='F' => c as u32 - 'A' as u32,
      _ => {
        if i == 0 {
          return None;
        }

        if i == 0 {
          return None;
        }
        let decoded = char::from_u32(value)?;
        return Some((decoded, c));
      }
    };

    if i >= 6 {
      return None;
    }
    value = (value << 4) | num;
  }
  None
}

fn extract_fn_closure_generics(
  generics: &syn::Generics,
) -> BindgenResult<HashMap<String, syn::PathArguments>> {
  let mut errors = vec![];

  let mut map = HashMap::default();
  if generics.params.is_empty() {
    return Ok(map);
  }

  if let Some(where_clause) = &generics.where_clause {
    for prediction in where_clause.predicates.iter() {
      match prediction {
        syn::WherePredicate::Type(syn::PredicateType {
          bounded_ty, bounds, ..
        }) => {
          for bound in bounds {
            match bound {
              syn::TypeParamBound::Trait(t) => {
                for segment in t.path.segments.iter() {
                  match segment.ident.to_string().as_str() {
                    "Fn" | "FnOnce" | "FnMut" => {
                      map.insert(
                        bounded_ty.to_token_stream().to_string(),
                        segment.arguments.clone(),
                      );
                    }
                    _ => {}
                  };
                }
              }
              syn::TypeParamBound::Lifetime(lifetime) => {
                if lifetime.ident != "static" {
                  errors.push(err_span!(
                    bound,
                    "only 'static is supported in lifetime bound for fn arguments"
                  ));
                }
              }
            }
          }
        }
        _ => errors.push(err_span! {
          prediction,
          "unsupported where clause prediction in napi"
        }),
      };
    }
  }

  for param in generics.params.iter() {
    match param {
      syn::GenericParam::Type(syn::TypeParam { ident, bounds, .. }) => {
        for bound in bounds {
          match bound {
            syn::TypeParamBound::Trait(t) => {
              for segment in t.path.segments.iter() {
                match segment.ident.to_string().as_str() {
                  "Fn" | "FnOnce" | "FnMut" => {
                    map.insert(ident.to_string(), segment.arguments.clone());
                  }
                  _ => {}
                };
              }
            }
            syn::TypeParamBound::Lifetime(lifetime) => {
              if lifetime.ident != "static" {
                errors.push(err_span!(
                  bound,
                  "only 'static is supported in lifetime bound for fn arguments"
                ));
              }
            }
          }
        }
      }
      _ => {
        errors.push(err_span!(param, "unsupported napi generic param for fn"));
      }
    }
  }

  Diagnostic::from_vec(errors).and(Ok(map))
}

fn napi_fn_from_decl(
  sig: Signature,
  opts: &BindgenAttrs,
  attrs: Vec<Attribute>,
  vis: Visibility,
  parent: Option<&Ident>,
) -> BindgenResult<NapiFn> {
  let mut errors = vec![];

  let syn::Signature {
    ident,
    asyncness,
    inputs,
    output,
    generics,
    ..
  } = sig;

  let mut fn_self = None;
  let callback_traits = extract_fn_closure_generics(&generics)?;

  let args = inputs
    .into_iter()
    .filter_map(|arg| match arg {
      syn::FnArg::Typed(mut p) => {
        let ty_str = p.ty.to_token_stream().to_string();
        if let Some(path_arguments) = callback_traits.get(&ty_str) {
          match extract_fn_types(path_arguments) {
            Ok((fn_args, fn_ret)) => Some(NapiFnArgKind::Callback(Box::new(CallbackArg {
              pat: p.pat,
              args: fn_args,
              ret: fn_ret,
            }))),
            Err(e) => {
              errors.push(e);
              None
            }
          }
        } else {
          let ty = replace_self(*p.ty, parent);
          p.ty = Box::new(ty);
          Some(NapiFnArgKind::PatType(Box::new(p)))
        }
      }
      syn::FnArg::Receiver(r) => {
        if parent.is_some() {
          assert!(fn_self.is_none());
          if r.reference.is_none() {
            errors.push(err_span!(
              r,
              "The native methods can't move values from napi. Try `&self` or `&mut self` instead."
            ));
          } else if r.mutability.is_some() {
            fn_self = Some(FnSelf::MutRef);
          } else {
            fn_self = Some(FnSelf::Ref);
          }
        } else {
          errors.push(err_span!(r, "arguments cannot be `self`"));
        }
        None
      }
    })
    .collect::<Vec<_>>();

  let ret = match output {
    syn::ReturnType::Default => None,
    syn::ReturnType::Type(_, ty) => Some(replace_self(*ty, parent)),
  };

  Diagnostic::from_vec(errors).map(|_| {
    let js_name = if let Some(prop_name) = opts.getter() {
      if let Some(ident) = prop_name {
        ident.to_string()
      } else {
        ident
          .to_string()
          .trim_start_matches("get_")
          .to_case(Case::Camel)
      }
    } else if let Some(prop_name) = opts.setter() {
      if let Some(ident) = prop_name {
        ident.to_string()
      } else {
        ident
          .to_string()
          .trim_start_matches("set_")
          .to_case(Case::Camel)
      }
    } else {
      opts.js_name().map_or_else(
        || ident.to_string().to_case(Case::Camel),
        |(js_name, _)| js_name.to_owned(),
      )
    };

    NapiFn {
      name: ident,
      js_name,
      args,
      ret,
      is_async: asyncness.is_some(),
      vis,
      kind: fn_kind(opts),
      fn_self,
      parent: parent.cloned(),
      attrs,
      strict: opts.strict().is_some(),
    }
  })
}

impl ParseNapi for syn::Item {
  fn parse_napi(&mut self, tokens: &mut TokenStream, opts: BindgenAttrs) -> BindgenResult<Napi> {
    match self {
      syn::Item::Fn(f) => f.parse_napi(tokens, opts),
      syn::Item::Struct(s) => s.parse_napi(tokens, opts),
      syn::Item::Impl(i) => i.parse_napi(tokens, opts),
      syn::Item::Enum(e) => e.parse_napi(tokens, opts),
      _ => bail_span!(
        self,
        "#[napi] can only be applied to a function, struct, enum or impl."
      ),
    }
  }
}

impl ParseNapi for syn::ItemFn {
  fn parse_napi(&mut self, tokens: &mut TokenStream, opts: BindgenAttrs) -> BindgenResult<Napi> {
    self.to_tokens(tokens);
    self.convert_to_ast(opts)
  }
}
impl ParseNapi for syn::ItemStruct {
  fn parse_napi(&mut self, tokens: &mut TokenStream, opts: BindgenAttrs) -> BindgenResult<Napi> {
    let napi = self.convert_to_ast(opts);
    self.to_tokens(tokens);

    napi
  }
}
impl ParseNapi for syn::ItemImpl {
  fn parse_napi(&mut self, tokens: &mut TokenStream, opts: BindgenAttrs) -> BindgenResult<Napi> {
    // #[napi] macro will be remove from impl items after converted to ast
    let napi = self.convert_to_ast(opts);
    self.to_tokens(tokens);

    napi
  }
}

impl ParseNapi for syn::ItemEnum {
  fn parse_napi(&mut self, tokens: &mut TokenStream, opts: BindgenAttrs) -> BindgenResult<Napi> {
    let napi = self.convert_to_ast(opts);
    self.to_tokens(tokens);

    napi
  }
}

fn fn_kind(opts: &BindgenAttrs) -> FnKind {
  let mut kind = FnKind::Normal;

  if opts.getter().is_some() {
    kind = FnKind::Getter;
  }

  if opts.setter().is_some() {
    kind = FnKind::Setter;
  }

  if opts.constructor().is_some() {
    kind = FnKind::Constructor;
  }

  kind
}

impl ConvertToAST for syn::ItemFn {
  fn convert_to_ast(&mut self, opts: BindgenAttrs) -> BindgenResult<Napi> {
    let func = napi_fn_from_decl(
      self.sig.clone(),
      &opts,
      self.attrs.clone(),
      self.vis.clone(),
      None,
    )?;

    Ok(Napi {
      comments: vec![],
      item: NapiItem::Fn(func),
    })
  }
}

impl ConvertToAST for syn::ItemStruct {
  fn convert_to_ast(&mut self, opts: BindgenAttrs) -> BindgenResult<Napi> {
    let vis = self.vis.clone();
    let struct_name = self.ident.clone();
    let js_name = opts.js_name().map_or_else(
      || self.ident.to_string().to_case(Case::Pascal),
      |(js_name, _)| js_name.to_owned(),
    );
    let mut fields = vec![];
    let mut is_tuple = false;

    for (i, field) in self.fields.iter_mut().enumerate() {
      match field.vis {
        syn::Visibility::Public(..) => {}
        _ => continue,
      }

      let field_opts = BindgenAttrs::find(&mut field.attrs)?;

      let (js_name, name) = match &field.ident {
        Some(ident) => (
          field_opts
            .js_name()
            .map_or_else(|| ident.to_string(), |(js_name, _)| js_name.to_owned()),
          syn::Member::Named(ident.clone()),
        ),
        None => {
          is_tuple = true;
          (i.to_string(), syn::Member::Unnamed(i.into()))
        }
      };

      let ignored = field_opts.skip().is_some();
      let readonly = field_opts.readonly().is_some();

      fields.push(NapiStructField {
        name,
        js_name,
        ty: field.ty.clone(),
        getter: !ignored,
        setter: !(ignored || readonly),
      })
    }

    record_struct(&struct_name, js_name.clone(), &opts);

    Ok(Napi {
      comments: vec![],
      item: NapiItem::Struct(NapiStruct {
        js_name,
        name: struct_name,
        vis,
        fields,
        is_tuple,
        gen_default_ctor: opts.constructor().is_some(),
      }),
    })
  }
}

impl ConvertToAST for syn::ItemImpl {
  fn convert_to_ast(&mut self, _opts: BindgenAttrs) -> BindgenResult<Napi> {
    let struct_name = match get_ty(&self.self_ty) {
      syn::Type::Path(syn::TypePath {
        ref path,
        qself: None,
      }) => path,
      _ => {
        bail_span!(self.self_ty, "unsupported self type in #[napi] impl")
      }
    };

    let struct_name = extract_path_ident(struct_name)?;

    let mut struct_js_name = struct_name.to_string();
    let mut items = vec![];

    for item in self.items.iter_mut() {
      let method = match item {
        syn::ImplItem::Method(m) => m,
        _ => {
          bail_span!(item, "unsupported impl item in #[napi]")
        }
      };
      let opts = BindgenAttrs::find(&mut method.attrs)?;

      // it'd better only care methods decorated with `#[napi]` attribute
      if !opts.exists {
        continue;
      }

      if opts.constructor().is_some() {
        struct_js_name = check_recorded_struct_for_impl(&struct_name, &opts)?;
      }

      let vis = method.vis.clone();

      match &vis {
        Visibility::Public(_) => {}
        _ => {
          bail_span!(method.sig.ident, "only pub method supported by #[napi].",);
        }
      }

      let func = napi_fn_from_decl(
        method.sig.clone(),
        &opts,
        method.attrs.clone(),
        vis,
        Some(&struct_name),
      )?;

      items.push(func);
    }

    Ok(Napi {
      comments: vec![],
      item: NapiItem::Impl(NapiImpl {
        name: struct_name,
        js_name: struct_js_name,
        items,
      }),
    })
  }
}

impl ConvertToAST for syn::ItemEnum {
  fn convert_to_ast(&mut self, opts: BindgenAttrs) -> BindgenResult<Napi> {
    match self.vis {
      Visibility::Public(_) => {}
      _ => bail_span!(self, "only public enum allowed"),
    }
    if self.variants.is_empty() {
      bail_span!(self, "cannot export empty enum to JS");
    }

    self.attrs.push(Attribute {
      pound_token: Default::default(),
      style: syn::AttrStyle::Outer,
      bracket_token: Default::default(),
      path: syn::parse_quote! { derive },
      tokens: quote! { (Copy, Clone) },
    });

    let js_name = opts
      .js_name()
      .map_or_else(|| self.ident.to_string(), |(s, _)| s.to_string());

    let mut last_variant_val: i32 = -1;
    let variants = self
      .variants
      .iter()
      .map(|v| {
        match v.fields {
          syn::Fields::Unit => {}
          _ => bail_span!(v.fields, "Structured enum is not supported in #[napi]"),
        };

        let val = match &v.discriminant {
          Some((_, expr)) => {
            let mut symbol = 1;
            let mut inner_expr = get_expr(expr);
            if let syn::Expr::Unary(syn::ExprUnary {
              attrs: _,
              op: syn::UnOp::Neg(_),
              expr,
            }) = inner_expr
            {
              symbol = -1;
              inner_expr = expr;
            }

            match inner_expr {
              syn::Expr::Lit(syn::ExprLit {
                attrs: _,
                lit: syn::Lit::Int(int_lit),
              }) => match int_lit.base10_digits().parse::<i32>() {
                Ok(v) => symbol * v,
                Err(_) => {
                  bail_span!(
                    int_lit,
                    "enums with #[wasm_bindgen] can only support \
                      numbers that can be represented as i32",
                  );
                }
              },
              _ => bail_span!(
                expr,
                "enums with #[wasm_bindgen] may only have \
                  number literal values",
              ),
            }
          }
          None => last_variant_val + 1,
        };

        last_variant_val = val;
        let comments = extract_doc_comments(&v.attrs);
        Ok(NapiEnumVariant {
          name: v.ident.clone(),
          val,
          comments,
        })
      })
      .collect::<BindgenResult<Vec<NapiEnumVariant>>>()?;

    let comments = extract_doc_comments(&self.attrs);

    Ok(Napi {
      comments,
      item: NapiItem::Enum(NapiEnum {
        name: self.ident.clone(),
        js_name,
        variants,
      }),
    })
  }
}