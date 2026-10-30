use anyhow::{Context, Result};
use quote::ToTokens;
use std::{fs, path::PathBuf};
use syn::parse::Parser;
use xsd_parser::{
    config::{Config, Resolver, Schema},
    generate,
    models::{format_ident, make_type_name, meta::MetaType, Naming as DefaultNaming},
    traits::{NameBuilder, Naming},
    Ident2, Name, TypeIdent,
};

fn main() -> Result<()> {
    let full_view = std::env::args().any(|arg| arg == "--full-view");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let schema_name = if full_view {
        "fpml-main-5-12.xsd"
    } else {
        "fpml-cd-5-12.xsd"
    };
    let schema = root.join("schemas/confirmation").join(schema_name);
    let mut config = Config::default()
        .with_serde_quick_xml()
        .with_naming(FpmlNaming::default());
    config.parser.resolver = vec![Resolver::File];
    config.parser.schemas = vec![Schema::File(schema.canonicalize()?)];
    let code = generate(config)
        .context("generating FpML confirmation bindings")?
        .to_string();
    let mut file = syn::parse_file(&code).context("formatting generated bindings")?;
    // Dynamic element aliases bypass Naming::make_type_name in xsd-parser 1.5.x.
    // Rename the standalone XSD alias; Rust's generic Option<T> usages stay intact.
    for item in &mut file.items {
        if let syn::Item::Type(item_type) = item {
            if item_type.ident == "Option" {
                item_type.ident = syn::Ident::new("FpmlOption", item_type.ident.span());
            }
        }
        if let syn::Item::Struct(item_struct) = item {
            if matches!(
                item_struct.ident.to_string().as_str(),
                "CurveInstrument" | "UnderlyingAsset" | "Exercise" | "Product"
            ) {
                // xsd-parser's open substitution-group wrappers use
                // Serialize + DeserializeOwned supertraits, which are not
                // object safe. These four types are not part of CDS example
                // deserialization, so retain compileable placeholders.
                item_struct.fields = syn::Fields::Unit;
                item_struct.semi_token = Some(Default::default());
            }
            if let syn::Fields::Named(fields) = &mut item_struct.fields {
                for field in &mut fields.named {
                    let is_optional = matches!(
                        &field.ty,
                        syn::Type::Path(path)
                            if path.path.segments.last().is_some_and(|segment| segment.ident == "Option")
                    );
                    if !is_optional {
                        continue;
                    }
                    if let Some(syn::Meta::List(serde)) = field
                        .attrs
                        .iter_mut()
                        .find_map(|attr| attr.path().is_ident("serde").then_some(&mut attr.meta))
                    {
                        serde
                            .tokens
                            .extend(quote::quote!(, skip_serializing_if = "Option::is_none"));
                    }
                }
            }
        }
        if let syn::Item::Enum(item_enum) = item {
            if !item_enum.ident.to_string().ends_with("EnumType") {
                continue;
            }
            let ident = item_enum.ident.clone();
            *item = syn::parse_quote! {
                #[derive(Debug, Deserialize, Serialize)]
                #[serde(transparent)]
                pub struct #ident(pub String);
            };
        }
    }
    for item in &mut file.items {
        let attrs = match item {
            syn::Item::Struct(item) => &mut item.attrs,
            syn::Item::Enum(item) => {
                // Schema choice groups naturally create large enums; their
                // layout is dictated by the generated FpML model.
                item.attrs
                    .push(syn::parse_quote!(#[allow(clippy::large_enum_variant)]));
                &mut item.attrs
            }
            _ => continue,
        };
        if let Some(syn::Meta::List(derive)) = attrs
            .iter_mut()
            .find_map(|attr| attr.path().is_ident("derive").then_some(&mut attr.meta))
        {
            let mut traits =
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
                    .parse2(derive.tokens.clone())?;
            traits.push(syn::parse_quote!(Clone));
            traits.push(syn::parse_quote!(PartialEq));
            derive.tokens = quote::quote!(#traits);
        }
    }
    // xsd-parser wraps some ordinary sequences in a second `...Content`
    // struct, then tags the outer field `$value`. quick-xml treats those
    // nested `$value` maps as repeated fields. Inline these sequence fields;
    // their serde element/attribute names remain on the moved fields.
    let sequence_fields: std::collections::HashMap<_, _> = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Struct(item_struct) => {
                Some((item_struct.ident.to_string(), item_struct.fields.clone()))
            }
            _ => None,
        })
        .collect();
    for item in &mut file.items {
        let syn::Item::Struct(item_struct) = item else {
            continue;
        };
        let syn::Fields::Named(fields) = &mut item_struct.fields else {
            continue;
        };
        let Some(index) = fields.named.iter().position(|field| {
            field.ident.as_ref().is_some_and(|ident| ident == "content")
                && field
                    .attrs
                    .iter()
                    .any(|attr| attr.meta.to_token_stream().to_string().contains("$value"))
        }) else {
            continue;
        };
        let syn::Type::Path(content_ty) = &fields.named[index].ty else {
            continue;
        };
        let Some(content_name) = content_ty
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
        else {
            continue;
        };
        let Some(syn::Fields::Named(content_fields)) = sequence_fields.get(&content_name) else {
            continue;
        };
        let mut flattened = syn::punctuated::Punctuated::new();
        for (field_index, field) in fields.named.iter().cloned().enumerate() {
            if field_index == index {
                flattened.extend(content_fields.named.iter().cloned());
            } else {
                flattened.push(field);
            }
        }
        fields.named = flattened;
    }
    file.items.retain(|item| {
        !matches!(item, syn::Item::Trait(item_trait) if [
            "CurveInstrumentTrait",
            "UnderlyingAssetTrait",
            "ExerciseTrait",
            "ProductTrait",
        ].contains(&item_trait.ident.to_string().as_str()))
    });
    file.items.retain(|item| {
        !matches!(item, syn::Item::Use(item_use) if item_use.to_token_stream().to_string().contains("xsd_parser_types :: AsAny"))
    });
    for item in &mut file.items {
        if let syn::Item::Use(item_use) = item {
            if item_use
                .to_token_stream()
                .to_string()
                .starts_with("use serde ::")
            {
                *item_use = syn::parse_quote!(
                    use serde::{Deserialize, Serialize};
                );
            }
        }
    }
    let code = prettyplease::unparse(&file);
    let output_name = if full_view {
        "fpml_confirmation.rs"
    } else {
        "fpml_cd.rs"
    };
    let output = root.join("generated").join(output_name);
    fs::write(
        &output,
        format!(
            "// Generated by `cargo run -p xtask -- generate`; do not edit.\n{}\n",
            code.trim_end()
        ),
    )?;
    println!("wrote {}", output.display());
    Ok(())
}

/// Keep FpML's colliding schema names distinct in Rust while preserving XML names.
#[derive(Debug, Clone, Default)]
struct FpmlNaming {
    inner: DefaultNaming,
}

impl Naming for FpmlNaming {
    fn clone_boxed(&self) -> Box<dyn Naming> {
        Box::new(self.clone())
    }
    fn builder(&self) -> Box<dyn NameBuilder> {
        self.inner.builder()
    }
    fn unify(&self, s: &str) -> String {
        self.inner.unify(s)
    }
    fn make_type_name(&self, postfixes: &[String], ty: &MetaType, ident: &TypeIdent) -> Name {
        let mut name = make_type_name(self, postfixes, ty, ident);
        if matches!(
            name.as_str(),
            "Option"
                | "NormalizedStringType"
                | "StringType"
                | "TokenType"
                | "LanguageType"
                | "AccountType"
                | "BrokerConfirmationType"
                | "CreditSupportAgreementType"
                | "EntityType"
                | "MasterAgreementType"
                | "MasterConfirmationType"
                | "OtherAgreementType"
                | "PartyRoleType"
                | "PaymentType"
                | "ProductType"
                | "ResourceType"
                | "SpreadScheduleType"
                | "RestructuringType"
        ) {
            name.as_mut().push_str(&format!("Ns{}", ident.ns.0));
        }
        if matches!(
            ident.name.as_str(),
            "AccountType"
                | "BrokerConfirmationType"
                | "CreditSupportAgreementType"
                | "EntityType"
                | "LanguageType"
                | "MasterAgreementType"
                | "MasterConfirmationType"
                | "OtherAgreementType"
                | "PartyRoleType"
                | "PaymentType"
                | "ProductType"
                | "ResourceType"
                | "SpreadScheduleType"
                | "RestructuringType"
                | "normalizedString"
                | "string"
                | "token"
                | "language"
        ) {
            name.as_mut().push_str(&format!("Ns{}", ident.ns.0));
        }
        name
    }
    fn make_unknown_variant(&self, id: usize) -> Ident2 {
        self.inner.make_unknown_variant(id)
    }
    fn format_module_name(&self, s: &str) -> String {
        self.inner.format_module_name(s)
    }
    fn format_type_name(&self, s: &str) -> String {
        if s == "Option" {
            format_ident("FpmlOption")
        } else {
            self.inner.format_type_name(s)
        }
    }
    fn format_field_name(&self, s: &str) -> String {
        self.inner.format_field_name(s)
    }
    fn format_variant_name(&self, s: &str) -> String {
        self.inner.format_variant_name(s)
    }
    fn format_constant_name(&self, s: &str) -> String {
        self.inner.format_constant_name(s)
    }
}
