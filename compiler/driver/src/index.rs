//! `wrela index`: every item of a package, for a tool that keeps one index a revision (forge)
//! and compares them: who calls an item at a revision, what changed between two, which items a
//! change touched.
//!
//! Each item (function, method, struct, enum, trait, trait set, constant and type alias) has:
//!
//! - `path`: as `wrela query` names it (`main::Grid::at`). A path that several items share gets
//!   what tells them apart: an impl's method its impl (`main::V::add[impl Add<f32> for V]`),
//!   another item its kind (`main::blob[struct]`), and then a number, in source order (`#2`).
//! - `kind`, `file` (in the package), and `line` and `end_line` (1-based, attributes included).
//! - `signature` and `body`: hashes of their tokens. A signature is what comes before the body:
//!   attributes, `pub`, and the declaration to the end of its return type, its `{` or its `=`.
//!   Comments and whitespace change neither hash; any other change to a token changes one.
//! - for a function, `effects` (language.md §8.1), everything it reaches included, and `calls`:
//!   each function its body (its closures' too) calls, uses as a value, dispatches or draws.
//!   A trait method called on a known type is its impl's method; on a generic parameter, the
//!   trait's. A function whose body has an error has neither, since its code isn't known, and
//!   nor has a `@fieldwise` trait's method, whose code is derived for each type (§3).
//!
//! The hashes are SHA-256 (the first 128 bits, in hex) of each token's kind and text: the same
//! on every machine. The items are sorted by path.

use crate::query::Index;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use wrela_diag::{FileId, Span};
use wrela_sema::defs::{FnOwner, PackageKind};
use wrela_sema::mir::{self, Callee, Rvalue};
use wrela_sema::program::Program;
use wrela_sema::ty::{ConstId, FnId, ModuleId};
use wrela_syntax::ast::{ImplMemberKind, ItemKind, TraitMemberKind};
use wrela_syntax::{Token, TokenKind};

/// What an index entry is of.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Of {
    Fn(FnId),
    Adt(u32),
    Trait(u32),
    TraitSet(u32),
    Const(ConstId),
    Alias(u32),
}

/// One item before its path is made unique.
struct Entry {
    of: Of,
    path: String,
    kind: &'static str,
    module: ModuleId,
    span: Span,
}

/// The index of the package at `root` (with std's items if `with_std`), and the program's
/// errors, rendered (`None` if it checks). `Err` with the errors if the package doesn't load.
pub fn index(root: &Path, with_std: bool) -> Result<(Value, Option<String>), String> {
    crate::with_partial(root, false, |part| {
        let ix = Index::new(part.checked, part.sources);
        let p = ix.p;
        let entries = entries(&ix);
        let paths = unique_paths(p, &entries);
        let fn_path: HashMap<FnId, &str> = entries
            .iter()
            .zip(&paths)
            .filter_map(|(e, path)| match e.of {
                Of::Fn(f) => Some((f, path.as_str())),
                _ => None,
            })
            .collect();
        let fx = wrela_sema::effects::Effects::build(p, &part.checked.mir);
        let mut files: HashMap<FileId, Tokens> = HashMap::new();
        let mut items = Vec::new();
        for (e, path) in entries.iter().zip(&paths) {
            let shown = match p.package_of(e.module).kind {
                PackageKind::Program => true,
                PackageKind::Std => with_std,
                PackageKind::Dependency => false,
            };
            if !shown {
                continue;
            }
            let source = part.sources.file(e.span.file);
            let toks =
                files.entry(e.span.file).or_insert_with(|| Tokens::of(e.span.file, &source.text));
            let (start, sig, body) = toks.split(p, e);
            let mut v = json!({
                "path": path,
                "kind": e.kind,
                "file": source.name,
                "line": source.line_index(start) + 1,
                "end_line": source.line_index(e.span.end.max(start + 1) - 1) + 1,
                "signature": sig,
                "body": body,
            });
            if let Of::Fn(f) = e.of {
                if let Some(effects) = fx.of(f) {
                    let names: Vec<&str> = effects.iter().map(|e| e.name()).collect();
                    v["effects"] = json!(names);
                    v["calls"] = json!(calls(p, &part.checked.mir[&f], &fn_path, &ix));
                } else if p.func(f).body.is_none() {
                    v["effects"] = json!([]);
                    v["calls"] = json!([]);
                }
            }
            items.push(v);
        }
        items.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        let index = json!({ "errors": part.errors.len(), "items": items });
        (index, part.rendered())
    })
}

/// The program's items, as `wrela query` names them, in a fixed order: functions, types,
/// traits, trait sets, constants, aliases, each by id. Functions without source (derived ones,
/// an opted-in impl's) and the types the compiler makes (an entry point's bound type) aren't.
fn entries(ix: &Index) -> Vec<Entry> {
    let p = ix.p;
    let join = |m: ModuleId, name: &str| {
        let mut path = p.module(m).path.clone();
        path.push(name.to_string());
        path.join("::")
    };
    let mut out = Vec::new();
    for (f, path) in &ix.fns {
        let d = p.func(*f);
        let opted_in = matches!(d.owner, FnOwner::Impl(im) if p.impl_(im).from_opt_in);
        if d.derived.is_some() || opted_in {
            continue;
        }
        let kind = if d.owner == FnOwner::Free { "function" } else { "method" };
        out.push(Entry {
            of: Of::Fn(*f),
            path: path.join("::"),
            kind,
            module: d.module,
            span: d.span,
        });
    }
    for (a, path) in &ix.adts {
        let d = p.adt(*a);
        if d.entry.is_some() {
            continue;
        }
        let kind = match d.kind {
            wrela_sema::defs::AdtKind::Struct(_) => "struct",
            wrela_sema::defs::AdtKind::Enum(_) => "enum",
        };
        out.push(Entry {
            of: Of::Adt(a.0),
            path: path.join("::"),
            kind,
            module: d.module,
            span: d.span,
        });
    }
    for (t, path) in &ix.traits {
        let d = p.trait_(*t);
        out.push(Entry {
            of: Of::Trait(t.0),
            path: path.join("::"),
            kind: "trait",
            module: d.module,
            span: d.span,
        });
    }
    for (i, d) in p.trait_sets.iter().enumerate() {
        let path = join(d.module, &d.name);
        out.push(Entry {
            of: Of::TraitSet(i as u32),
            path,
            kind: "trait set",
            module: d.module,
            span: d.span,
        });
    }
    for (i, d) in p.consts.iter().enumerate() {
        if d.default {
            continue;
        }
        let path = join(d.module, &d.name);
        out.push(Entry {
            of: Of::Const(ConstId(i as u32)),
            path,
            kind: "constant",
            module: d.module,
            span: d.span,
        });
    }
    for (i, d) in p.aliases.iter().enumerate() {
        let path = join(d.module, &d.name);
        out.push(Entry {
            of: Of::Alias(i as u32),
            path,
            kind: "type",
            module: d.module,
            span: d.span,
        });
    }
    out
}

/// Each entry's path, made unique (see the top of this file).
fn unique_paths(p: &Program, entries: &[Entry]) -> Vec<String> {
    let mut by_path: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, e) in entries.iter().enumerate() {
        by_path.entry(&e.path).or_default().push(i);
    }
    let mut out: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    for shared in by_path.values().filter(|is| is.len() > 1) {
        for &i in shared {
            let e = &entries[i];
            let qualifier = match e.of {
                Of::Fn(f) => match p.func(f).owner {
                    FnOwner::Impl(im) => {
                        let im = p.impl_(im);
                        match &im.trait_ref {
                            Some(t) => format!(
                                "impl {} for {}",
                                p.display_trait_ref(t),
                                p.display_ty(im.self_ty)
                            ),
                            None => format!("impl {}", p.display_ty(im.self_ty)),
                        }
                    }
                    _ => e.kind.to_string(),
                },
                _ => e.kind.to_string(),
            };
            out[i] = format!("{}[{qualifier}]", e.path);
        }
    }
    // What's still shared is numbered, in source order.
    let mut by_path: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, path) in out.iter().enumerate() {
        by_path.entry(path.clone()).or_default().push(i);
    }
    for mut shared in by_path.into_values().filter(|is| is.len() > 1) {
        shared.sort_by_key(|&i| (entries[i].span.file, entries[i].span.start, i));
        for (n, i) in shared.into_iter().enumerate().skip(1) {
            out[i] = format!("{}#{}", out[i], n + 1);
        }
    }
    out
}

/// The paths of the functions `body` uses, each once, sorted.
fn calls(p: &Program, body: &mir::Body, paths: &HashMap<FnId, &str>, ix: &Index) -> Vec<String> {
    let mut fns = BTreeSet::new();
    for (s, r) in mir::rvalues(body) {
        if let Rvalue::Call(c) = r
            && let Callee::TraitMethod { method, self_ty, trait_args, method_args } = &c.callee
        {
            // On a known type, the impl's method is what's called, or the trait's if the
            // impl's is derived.
            let resolved = (!p.types.has_params(*self_ty))
                .then(|| {
                    wrela_sema::traits::resolve_trait_method(
                        p,
                        *method,
                        *self_ty,
                        trait_args,
                        method_args,
                    )
                })
                .flatten()
                .map(|(g, _)| p.func(g).derived.as_ref().map_or(g, |d| d.method));
            fns.insert(resolved.unwrap_or(*method));
            continue;
        }
        mir::fn_uses(p, s, r, |u, _| {
            fns.insert(u.func());
        });
    }
    let named: BTreeSet<String> = fns
        .into_iter()
        .map(|f| paths.get(&f).map_or_else(|| ix.fn_path(f), |s| s.to_string()))
        .collect();
    named.into_iter().collect()
}

/// A file's tokens (comments and whitespace aren't tokens), and where each function's
/// declaration starts: its attributes, or its `pub`, by where its signature starts.
struct Tokens {
    tokens: Vec<Token>,
    text: String,
    fn_starts: HashMap<u32, u32>,
}

impl Tokens {
    fn of(file: FileId, text: &str) -> Tokens {
        let lexed = wrela_syntax::lex(file, text);
        let tokens = lexed.tokens.clone();
        let parsed = wrela_syntax::parser::parse_tokens(file, text, lexed);
        let mut fn_starts = HashMap::new();
        for item in &parsed.file.items {
            match &item.kind {
                ItemKind::Fn(f) => {
                    fn_starts.insert(f.sig_span.start, item.span.start);
                }
                ItemKind::Impl(im) => {
                    for m in &im.members {
                        if let ImplMemberKind::Fn(f) = &m.kind {
                            fn_starts.insert(f.sig_span.start, m.span.start);
                        }
                    }
                }
                ItemKind::Trait(t) => {
                    for m in &t.members {
                        if let TraitMemberKind::Fn(f) = &m.kind {
                            fn_starts.insert(f.sig_span.start, m.span.start);
                        }
                    }
                }
                _ => {}
            }
        }
        Tokens { tokens, text: text.to_string(), fn_starts }
    }

    /// The tokens from byte `start` to byte `end`.
    fn range(&self, start: u32, end: u32) -> &[Token] {
        let lo = self.tokens.partition_point(|t| t.span.start < start);
        let hi = self.tokens.partition_point(|t| t.span.start < end);
        &self.tokens[lo..hi.max(lo)]
    }

    /// Where `e`'s declaration starts, and the hashes of its signature and its body.
    fn split(&self, p: &Program, e: &Entry) -> (u32, String, String) {
        let span = e.span;
        if let Of::Fn(f) = e.of {
            let sig = p.func(f).sig_span;
            let start = self.fn_starts.get(&sig.start).copied().unwrap_or(span.start);
            let body = self.range(sig.end, span.end);
            return (start, self.hash(self.range(start, sig.end)), self.hash(body));
        }
        let all = self.range(span.start, span.end);
        // A type's or a trait's body is its braces; a constant's, an alias's or a trait set's
        // is what follows its `=`.
        let at = match e.of {
            Of::Adt(_) | Of::Trait(_) => all.iter().position(|t| t.kind == TokenKind::LBrace),
            _ => {
                let mut depth = 0i32;
                all.iter().position(|t| {
                    match t.kind {
                        TokenKind::LParen
                        | TokenKind::LBracket
                        | TokenKind::LBrace
                        | TokenKind::Lt => depth += 1,
                        TokenKind::RParen
                        | TokenKind::RBracket
                        | TokenKind::RBrace
                        | TokenKind::Gt => depth -= 1,
                        TokenKind::Shr => depth -= 2,
                        _ => {}
                    }
                    depth <= 0 && t.kind == TokenKind::Eq
                })
            }
        };
        let (sig, body) = all.split_at(at.unwrap_or(all.len()));
        (span.start, self.hash(sig), self.hash(body))
    }

    /// The hash of `tokens`: each one's kind, and its text if its kind's text varies. Line
    /// breaks the grammar sees (NEWLINE) count once, and not at either end.
    fn hash(&self, tokens: &[Token]) -> String {
        let mut h = Sha256::new();
        let mut newline = false;
        let mut any = false;
        for t in tokens {
            match t.kind {
                TokenKind::Eof => continue,
                TokenKind::Newline => {
                    newline = any;
                    continue;
                }
                _ => {}
            }
            if newline {
                h.update(b"\n\0");
                newline = false;
            }
            any = true;
            h.update(t.kind.grammar_name().as_bytes());
            h.update([0]);
            if t.kind.fixed_text().is_none() {
                let text = self.text.get(t.span.start as usize..t.span.end as usize).unwrap_or("");
                h.update(text.as_bytes());
                h.update([0]);
            }
        }
        h.finalize()[..16].iter().map(|b| format!("{b:02x}")).collect()
    }
}
