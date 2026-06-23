#![allow(clippy::all)]
use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, Lit, Meta, PatType, Type, parse_macro_input};

/// `#[tool]` attribute macro for declarative tool definition.
///
/// # Usage
///
/// ```rust,ignore
/// use maple_macro::tool;
/// use anyhow::Result;
/// use serde_json::Value;
///
/// #[tool(description = "Read a file from disk")]
/// async fn read_file(path: String) -> Result<Value> {
///     let content = tokio::fs::read_to_string(&path).await?;
///     Ok(serde_json::json!({ "content": content }))
/// }
/// ```
///
/// This generates:
/// - `read_file_definition()` → `ToolDefinition` with JSON Schema
/// - `read_file_execute(args: Value)` → executor wrapper
///
/// Parameter types are mapped to JSON Schema:
/// - `String` → `{ "type": "string" }`
/// - `i32/i64/isize/u32/u64/usize` → `{ "type": "integer" }`
/// - `f32/f64` → `{ "type": "number" }`
/// - `bool` → `{ "type": "boolean" }`
/// - `Vec<T>` → `{ "type": "array", "items": ... }`
/// - `Option<T>` → same as T but not in "required" list
///
/// Use `#[tool(required)]` to force all params required.
/// Use `#[tool(name = "custom_name")]` to override the tool name.
#[proc_macro_attribute]
pub fn tool(args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);
    let args_meta = parse_macro_input!(args as ToolArgs);

    match impl_tool(args_meta, input_fn) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct ToolArgs {
    description: String,
    name_override: Option<String>,
}

impl syn::parse::Parse for ToolArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut description = String::new();
        let mut name_override = None;

        while !input.is_empty() {
            let meta: Meta = input.parse()?;
            if let Meta::NameValue(nv) = meta {
                let ident = nv
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                if let syn::Expr::Lit(expr_lit) = &nv.value
                    && let Lit::Str(s) = &expr_lit.lit
                {
                    match ident.as_str() {
                        "description" => description = s.value(),
                        "name" => name_override = Some(s.value()),
                        _ => {}
                    }
                }
            }
            if !input.is_empty() {
                let _ = input.parse::<syn::Token![,]>();
            }
        }

        Ok(ToolArgs {
            description,
            name_override,
        })
    }
}

fn impl_tool(args: ToolArgs, input_fn: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    let fn_name = &input_fn.sig.ident;
    let fn_name_str = args.name_override.unwrap_or_else(|| fn_name.to_string());
    let description = args.description;
    let vis = &input_fn.vis;
    let attrs = &input_fn.attrs;
    let sig = &input_fn.sig;
    let block = &input_fn.block;

    // Extract parameters
    let mut properties = Vec::new();
    let mut required = Vec::new();
    let mut param_names = Vec::new();
    let mut param_types = Vec::new();

    for input in &sig.inputs {
        if let syn::FnArg::Typed(PatType { pat, ty, .. }) = input
            && let syn::Pat::Ident(pat_ident) = pat.as_ref()
        {
            let name = pat_ident.ident.to_string();
            let (schema, is_optional) = type_to_json_schema(ty)?;

            properties.push(quote! {
                (#name.to_string(), #schema)
            });

            if !is_optional {
                required.push(quote! { #name.to_string() });
            }

            param_names.push(pat_ident.ident.clone());
            param_types.push(ty.clone());
        }
    }

    // Generate the definition function
    let def_fn_name = syn::Ident::new(&format!("{}_definition", fn_name), fn_name.span());
    let exec_fn_name = syn::Ident::new(&format!("{}_execute", fn_name), fn_name.span());

    let expanded = quote! {
        #(#attrs)*
        #vis #sig #block

        /// Auto-generated tool definition
        pub fn #def_fn_name() -> maple_llm::request::ToolDefinition {
            use serde_json::json;
            let properties: serde_json::Map<String, serde_json::Value> = {
                let mut map = serde_json::Map::new();
                for (key, val) in vec![#(#properties),*] {
                    map.insert(key, val);
                }
                map
            };
            let required_fields: Vec<String> = vec![#(#required),*];

            maple_llm::request::ToolDefinition {
                name: #fn_name_str.to_string(),
                description: #description.to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": properties,
                    "required": required_fields,
                }),
            }
        }

        /// Auto-generated tool executor wrapper
        pub async fn #exec_fn_name(args: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
            #(
                let #param_names: #param_types = serde_json::from_value(
                    args.get(stringify!(#param_names))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)
                ).map_err(|e| anyhow::anyhow!("Parameter '{}' error: {}", stringify!(#param_names), e))?;
            )*
            #fn_name(#(#param_names),*).await
        }
    };

    Ok(expanded)
}

fn type_to_json_schema(ty: &Type) -> syn::Result<(proc_macro2::TokenStream, bool)> {
    match ty {
        Type::Path(type_path) => {
            let segments = &type_path.path.segments;
            let last = segments.last().unwrap();
            let type_name = last.ident.to_string();

            let (schema, optional) = match type_name.as_str() {
                "String" => (quote! { json!({"type": "string"}) }, false),
                "str" => (quote! { json!({"type": "string"}) }, false),
                "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize" => {
                    (quote! { json!({"type": "integer"}) }, false)
                }
                "f32" | "f64" => (quote! { json!({"type": "number"}) }, false),
                "bool" => (quote! { json!({"type": "boolean"}) }, false),
                "Option" => {
                    if let syn::PathArguments::AngleBracketed(args) = &last.arguments
                        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
                    {
                        let (inner_schema, _) = type_to_json_schema(inner)?;
                        return Ok((inner_schema, true));
                    }
                    (quote! { json!({"type": "string"}) }, true)
                }
                "Vec" => {
                    if let syn::PathArguments::AngleBracketed(args) = &last.arguments
                        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
                    {
                        let (item_schema, _) = type_to_json_schema(inner)?;
                        (
                            quote! { json!({"type": "array", "items": #item_schema}) },
                            false,
                        )
                    } else {
                        (
                            quote! { json!({"type": "array", "items": {"type": "string"}}) },
                            false,
                        )
                    }
                }
                "Value" | "JsonValue" => (quote! { json!({}) }, false),
                _ => (quote! { json!({"type": "object"}) }, false),
            };

            Ok((schema, optional))
        }
        _ => Ok((quote! { json!({"type": "string"}) }, false)),
    }
}
