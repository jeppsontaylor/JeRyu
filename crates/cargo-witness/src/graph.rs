use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::model::{CrateWitness, PubItem, WitnessGraph};

/// Build a witness graph for the workspace at `workspace_root`.
///
/// For each workspace member crate, parses all `.rs` files in `src/` using
/// `syn`, extracts public item signatures, and computes dual hashes:
/// - **Interface hash**: SHA-256 of sorted pub-item signatures
/// - **Implementation hash**: SHA-256 of all source content minus pub signatures
pub fn build_witness_graph(
    _workspace_root: &Path,
    manifest_path: Option<&Path>,
) -> Result<WitnessGraph> {
    let snapshot = cargo_vrc::load_workspace(manifest_path)?;

    let mut crates = Vec::new();
    for package in &snapshot.packages {
        let witness = build_crate_witness(&snapshot.workspace_root, package)?;
        crates.push(witness);
    }

    Ok(WitnessGraph {
        generated_at: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        workspace_root: display_workspace_root(),
        crates,
    })
}

/// Build witness data for a single crate.
fn build_crate_witness(
    workspace_root: &Path,
    package: &cargo_vrc::workspace::PackageSnapshot,
) -> Result<CrateWitness> {
    let src_dir = package.package_root.join("src");
    let mut pub_items = Vec::new();
    let mut interface_hasher = Sha256::new();
    let mut impl_hasher = Sha256::new();
    let mut file_count = 0usize;
    let mut total_lines = 0usize;

    if src_dir.exists() {
        let mut rs_files: Vec<PathBuf> = WalkDir::new(&src_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().is_file()
                    && entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs")
            })
            .map(|entry| entry.path().to_path_buf())
            .collect();

        // Sort for deterministic hashing.
        rs_files.sort();

        for rs_file in rs_files {
            let content = fs::read_to_string(&rs_file)
                .with_context(|| format!("failed to read {}", rs_file.display()))?;

            file_count += 1;
            total_lines += content.lines().count();

            let relative = rs_file
                .strip_prefix(workspace_root)
                .unwrap_or(&rs_file)
                .display()
                .to_string();

            let extracted = extract_pub_items(&relative, &content);
            let pub_signatures: BTreeSet<String> = extracted
                .iter()
                .map(|item| item.signature.clone())
                .collect();

            // Interface hash: sorted pub signatures.
            for sig in &pub_signatures {
                interface_hasher.update(sig.as_bytes());
                interface_hasher.update(b"\n");
            }

            // Implementation hash: everything that isn't a pub signature.
            // We hash the full content and also the "non-pub" marker so that
            // identical pub signatures with different implementations produce
            // different implementation hashes.
            impl_hasher.update(relative.as_bytes());
            impl_hasher.update(content.as_bytes());
            for sig in &pub_signatures {
                // XOR-remove the pub signatures from the impl hash by including
                // a distinguishing prefix.
                impl_hasher.update(b"PUB:");
                impl_hasher.update(sig.as_bytes());
            }

            pub_items.extend(extracted);
        }
    }

    let interface_hash = hex_digest(interface_hasher);
    let implementation_hash = hex_digest(impl_hasher);

    Ok(CrateWitness {
        name: package.name.clone(),
        interface_hash,
        implementation_hash,
        pub_items,
        direct_deps: package.direct_dependencies.clone(),
        reverse_deps: package.reverse_dependencies.clone(),
        file_count,
        total_lines,
    })
}

/// Extract public items from a Rust source file using `syn`.
fn extract_pub_items(relative_path: &str, content: &str) -> Vec<PubItem> {
    let Ok(file) = syn::parse_file(content) else {
        // If parsing fails (e.g., the file has syntax errors), skip it.
        return Vec::new();
    };

    let mut items = Vec::new();

    for item in &file.items {
        match item {
            syn::Item::Fn(func) => {
                if matches!(func.vis, syn::Visibility::Public(_)) {
                    let signature = format_fn_signature(&func.sig);
                    items.push(PubItem {
                        kind: "fn".to_string(),
                        name: func.sig.ident.to_string(),
                        signature,
                    });
                }
            }
            syn::Item::Struct(s) => {
                if matches!(s.vis, syn::Visibility::Public(_)) {
                    let name = s.ident.to_string();
                    let generics = s.generics.params.iter().count();
                    let fields = match &s.fields {
                        syn::Fields::Named(named) => named.named.len(),
                        syn::Fields::Unnamed(unnamed) => unnamed.unnamed.len(),
                        syn::Fields::Unit => 0,
                    };
                    items.push(PubItem {
                        kind: "struct".to_string(),
                        name: name.clone(),
                        signature: format!(
                            "pub struct {name}{}({fields} fields)",
                            if generics > 0 {
                                format!("<{generics} generics>")
                            } else {
                                String::new()
                            }
                        ),
                    });
                }
            }
            syn::Item::Enum(e) => {
                if matches!(e.vis, syn::Visibility::Public(_)) {
                    let name = e.ident.to_string();
                    let variants = e.variants.len();
                    items.push(PubItem {
                        kind: "enum".to_string(),
                        name: name.clone(),
                        signature: format!("pub enum {name}({variants} variants)"),
                    });
                }
            }
            syn::Item::Trait(t) => {
                if matches!(t.vis, syn::Visibility::Public(_)) {
                    let name = t.ident.to_string();
                    let method_count = t
                        .items
                        .iter()
                        .filter(|item| matches!(item, syn::TraitItem::Fn(_)))
                        .count();
                    items.push(PubItem {
                        kind: "trait".to_string(),
                        name: name.clone(),
                        signature: format!("pub trait {name}({method_count} methods)"),
                    });
                }
            }
            syn::Item::Type(t) => {
                if matches!(t.vis, syn::Visibility::Public(_)) {
                    let name = t.ident.to_string();
                    items.push(PubItem {
                        kind: "type".to_string(),
                        name: name.clone(),
                        signature: format!("pub type {name} = ..."),
                    });
                }
            }
            syn::Item::Const(c) => {
                if matches!(c.vis, syn::Visibility::Public(_)) {
                    let name = c.ident.to_string();
                    items.push(PubItem {
                        kind: "const".to_string(),
                        name: name.clone(),
                        signature: format!("pub const {name}: ..."),
                    });
                }
            }
            syn::Item::Static(s) => {
                if matches!(s.vis, syn::Visibility::Public(_)) {
                    let name = s.ident.to_string();
                    items.push(PubItem {
                        kind: "static".to_string(),
                        name: name.clone(),
                        signature: format!("pub static {name}: ..."),
                    });
                }
            }
            syn::Item::Mod(m) => {
                if matches!(m.vis, syn::Visibility::Public(_)) {
                    let name = m.ident.to_string();
                    items.push(PubItem {
                        kind: "mod".to_string(),
                        name: name.clone(),
                        signature: format!("pub mod {name}"),
                    });
                }
            }
            syn::Item::Impl(imp) => {
                // Extract pub methods from impl blocks.
                for impl_item in &imp.items {
                    if let syn::ImplItem::Fn(method) = impl_item
                        && matches!(method.vis, syn::Visibility::Public(_))
                    {
                        let self_ty = quote_type(&imp.self_ty);
                        let sig = format_fn_signature(&method.sig);
                        items.push(PubItem {
                            kind: "fn".to_string(),
                            name: method.sig.ident.to_string(),
                            signature: format!("impl {self_ty} :: {sig}"),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    // Sort for deterministic output.
    items.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));

    let _ = relative_path; // used for context but not needed in output
    items
}

/// Format a function signature as a concise string.
fn format_fn_signature(sig: &syn::Signature) -> String {
    let asyncness = if sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let unsafety = if sig.unsafety.is_some() {
        "unsafe "
    } else {
        ""
    };
    let name = &sig.ident;

    let generics = if sig.generics.params.is_empty() {
        String::new()
    } else {
        let params: Vec<String> = sig
            .generics
            .params
            .iter()
            .map(|param| match param {
                syn::GenericParam::Type(t) => t.ident.to_string(),
                syn::GenericParam::Lifetime(l) => format!("'{}", l.lifetime.ident),
                syn::GenericParam::Const(c) => format!("const {}", c.ident),
            })
            .collect();
        format!("<{}>", params.join(", "))
    };

    let inputs: Vec<String> = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Receiver(r) => {
                let prefix = if r.reference.is_some() {
                    if r.mutability.is_some() {
                        "&mut self"
                    } else {
                        "&self"
                    }
                } else {
                    "self"
                };
                prefix.to_string()
            }
            syn::FnArg::Typed(pat) => quote_type(&pat.ty),
        })
        .collect();

    let output = match &sig.output {
        syn::ReturnType::Default => String::new(),
        syn::ReturnType::Type(_, ty) => format!(" -> {}", quote_type(ty)),
    };

    format!(
        "{unsafety}{asyncness}fn {name}{generics}({}){output}",
        inputs.join(", ")
    )
}

/// Convert a type to a concise string representation.
fn quote_type(ty: &syn::Type) -> String {
    // Use the token stream for a faithful representation.
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

/// Finalize a SHA-256 hasher into a hex string.
fn hex_digest(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Write a witness graph to disk as JSON.
pub fn write_witness_graph(workspace_root: &Path, graph: &WitnessGraph) -> Result<()> {
    let output_dir = workspace_root.join(".witness");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let output_path = output_dir.join("witness-graph.json");
    let json = serde_json::to_string_pretty(graph)?;
    fs::write(&output_path, json)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(())
}

/// Load a witness graph from disk.
pub fn load_witness_graph(workspace_root: &Path) -> Result<WitnessGraph> {
    let path = workspace_root.join(".witness/witness-graph.json");
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

/// Try to load a witness graph, returning None if it doesn't exist.
pub fn load_witness_graph_if_present(workspace_root: &Path) -> Option<WitnessGraph> {
    let path = workspace_root.join(".witness/witness-graph.json");
    if !path.exists() {
        return None;
    }
    load_witness_graph(workspace_root).ok()
}

fn display_workspace_root() -> String {
    ".".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pub_fn() {
        let source = r#"
            pub fn hello(name: &str) -> String {
                format!("Hello, {name}")
            }

            fn private_helper() -> bool {
                true
            }
        "#;
        let items = extract_pub_items("test.rs", source);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "fn");
        assert_eq!(items[0].name, "hello");
        assert!(items[0].signature.contains("fn hello"));
    }

    #[test]
    fn extract_pub_struct() {
        let source = r#"
            pub struct Config {
                pub name: String,
                port: u16,
            }

            struct Internal {
                data: Vec<u8>,
            }
        "#;
        let items = extract_pub_items("test.rs", source);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "struct");
        assert_eq!(items[0].name, "Config");
    }

    #[test]
    fn extract_pub_enum() {
        let source = r#"
            pub enum Status {
                Active,
                Inactive,
                Pending,
            }
        "#;
        let items = extract_pub_items("test.rs", source);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "enum");
        assert_eq!(items[0].name, "Status");
        assert!(items[0].signature.contains("3 variants"));
    }

    #[test]
    fn extract_pub_trait() {
        let source = r#"
            pub trait Handler {
                fn handle(&self, request: &Request) -> Response;
                fn name(&self) -> &str;
            }
        "#;
        let items = extract_pub_items("test.rs", source);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, "trait");
        assert_eq!(items[0].name, "Handler");
        assert!(items[0].signature.contains("2 methods"));
    }

    #[test]
    fn extract_impl_pub_methods() {
        let source = r#"
            pub struct Foo;

            impl Foo {
                pub fn bar(&self) -> i32 { 42 }
                fn private(&self) {}
            }
        "#;
        let items = extract_pub_items("test.rs", source);
        // Should get: pub struct Foo, pub fn bar
        assert_eq!(items.len(), 2);
        let fn_items: Vec<_> = items.iter().filter(|i| i.kind == "fn").collect();
        assert_eq!(fn_items.len(), 1);
        assert_eq!(fn_items[0].name, "bar");
    }

    #[test]
    fn private_items_excluded() {
        let source = r#"
            fn private_fn() -> bool { true }
            struct PrivateStruct { x: i32 }
            enum PrivateEnum { A, B }
        "#;
        let items = extract_pub_items("test.rs", source);
        assert!(items.is_empty());
    }

    #[test]
    fn deterministic_ordering() {
        let source = r#"
            pub fn zebra() {}
            pub fn alpha() {}
            pub struct Middle;
        "#;
        let items = extract_pub_items("test.rs", source);
        assert_eq!(items.len(), 3);
        // Should be sorted by kind then name
        assert_eq!(items[0].name, "alpha");
        assert_eq!(items[1].name, "zebra");
        assert_eq!(items[2].name, "Middle");
    }

    #[test]
    fn hex_digest_is_deterministic() {
        let mut h1 = Sha256::new();
        let mut h2 = Sha256::new();
        h1.update(b"hello");
        h2.update(b"hello");
        assert_eq!(hex_digest(h1), hex_digest(h2));
    }

    #[test]
    fn workspace_root_display_is_stable() {
        assert_eq!(display_workspace_root(), ".");
    }
}
