//! Behaviour diffs (`wrela diff-behaviour`, language.md §23.10): where two versions of a
//! package's free function behave differently, on the same seeded inputs.
//!
//! Each version is checked, and the function's signature read: every parameter and its result
//! must be made of `bool`, `i32`, `u32`, `f32`, their vectors, and structs, tuples and arrays of
//! those (§15.2's numbers). Then each version is built again with a harness appended to the
//! function's file (an overlay, so nothing is written): a test of code for each sample, which
//! calls the function on the sample's literals and prints what it returned and each `mut`
//! argument's final value as numbers. The tests run as `wrela test` runs them, on the CPU in
//! wasmtime, but from a release build's code.
//!
//! Two versions agree on a sample only if they ended the same way (returned, panicked with the
//! same message, trapped the same way, ran out of fuel) and, if they returned, gave the same
//! numbers: floats bit for bit (so `-0.0` isn't `0.0`, §11.3), or within a tolerance. Agreeing
//! on every sample is evidence, not proof, so the report never says the versions are
//! equivalent: it names the samples.

use crate::consts;
pub use crate::consts::Ending;
use crate::query::Index;
use std::path::{Path, PathBuf};
use wrela_diag::has_errors;
use wrela_sema::defs::{AdtKind, FnOwner, PackageKind};
use wrela_sema::effects::{Effect, Effects};
use wrela_sema::program::Program;
use wrela_sema::ty::{FloatTy, IntTy, ModuleId, TyId, TyKind, VecElem};
use wrela_syntax::ast::{Mode, RetMode};

/// What `diff` samples, and how it compares.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The seeded samples, besides those of boundary values.
    pub samples: u32,
    pub seed: u64,
    /// The absolute difference allowed between two floats; `None` compares their bits.
    pub within: Option<f64>,
}

impl Default for Options {
    fn default() -> Options {
        Options { samples: 256, seed: 1, within: None }
    }
}

/// The most boundary samples a function gets: one for each boundary value of each of its
/// numbers, up to this.
const MAX_BOUNDARY: usize = 512;

/// The differing samples a report shows; it counts them all.
pub const SHOWN: usize = 10;

/// What `diff` found.
#[derive(Debug)]
pub struct Report {
    /// The function's path, as `wrela query` names it.
    pub item: String,
    pub seed: u64,
    /// The samples of boundary values (the first ones), and the seeded samples after them.
    pub boundary: usize,
    pub seeded: usize,
    pub within: Option<f64>,
    /// How many samples differ.
    pub differing: usize,
    /// The first [`SHOWN`] that do.
    pub differences: Vec<Difference>,
}

impl Report {
    pub fn samples(&self) -> usize {
        self.boundary + self.seeded
    }
}

/// A sample on which the versions differ.
#[derive(Debug)]
pub struct Difference {
    /// Its index: boundary samples first.
    pub sample: usize,
    pub boundary: bool,
    /// Each parameter (the new version's names) and its value, as Wrela would write it.
    pub inputs: Vec<(String, String)>,
    pub old: Outcome,
    pub new: Outcome,
}

/// How one version's call ended, and, if it returned, what it gave.
#[derive(Debug, PartialEq)]
pub struct Outcome {
    pub ending: Ending,
    /// What it returned, if it returns a value and did return.
    pub result: Option<String>,
    /// Each `mut` parameter's final value, if it returned.
    pub muts: Vec<(String, String)>,
}

/// Where the functions named `item` of the packages at `old` and `new` behave differently, on
/// the samples `options` gives. An error (a usage or build error) if either doesn't check, the
/// item isn't a free function of the package with a signature that can be sampled, the two
/// signatures differ, or the harness doesn't build.
pub fn diff(old: &Path, new: &Path, item: &str, options: Options) -> Result<Report, String> {
    let old_err = |e: String| format!("the old version (`{}`): {e}", old.display());
    let new_err = |e: String| format!("the new version (`{}`): {e}", new.display());
    let old_fn = crate::on_compiler_thread(|| signature(old, item)).map_err(old_err)?;
    let new_fn = crate::on_compiler_thread(|| signature(new, item)).map_err(new_err)?;
    if old_fn.skeleton() != new_fn.skeleton() {
        return Err(format!(
            "`{item}`'s signature differs: the old takes {}, the new {}; a behaviour diff compares one signature",
            old_fn.skeleton(),
            new_fn.skeleton()
        ));
    }
    let (boundary, seeded) = samples(&new_fn.leaves(), options);
    let all: Vec<Vec<Leaf>> = boundary.iter().chain(&seeded).cloned().collect();
    let old_runs = crate::on_compiler_thread(|| run(old, &old_fn, &all)).map_err(old_err)?;
    let new_runs = crate::on_compiler_thread(|| run(new, &new_fn, &all)).map_err(new_err)?;
    let mut differences = Vec::new();
    let mut differing = 0;
    for (k, ((a, b), sample)) in old_runs.iter().zip(&new_runs).zip(&all).enumerate() {
        if agree(a, b, options.within) {
            continue;
        }
        differing += 1;
        if differences.len() < SHOWN {
            differences.push(Difference {
                sample: k,
                boundary: k < boundary.len(),
                inputs: new_fn.show_inputs(sample),
                old: old_fn.outcome(a),
                new: new_fn.outcome(b),
            });
        }
    }
    Ok(Report {
        item: new_fn.path.clone(),
        seed: options.seed,
        boundary: boundary.len(),
        seeded: seeded.len(),
        within: options.within,
        differing,
        differences,
    })
}

/// A number of a sample or a result: one of the leaves of a value's shape. A float is its bits.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Leaf {
    Bool(bool),
    I32(i32),
    U32(u32),
    F32(u32),
}

/// The kinds of number a value is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Num {
    Bool,
    I32,
    U32,
    F32,
}

/// A value's type, as the harness writes and reads it.
#[derive(Clone, Debug)]
enum Shape {
    Num(Num),
    /// A vector of `n` components.
    Vec(Num, u8),
    /// A struct: its path as the function's module names it, and its fields.
    Struct(String, Vec<(String, Shape)>),
    Tuple(Vec<Shape>),
    Array(Box<Shape>, u32),
}

/// A function to diff: its path, its file, and what its signature is made of.
struct Function {
    path: String,
    name: String,
    /// Its file, canonical, and that file's text: what the harness is appended to.
    file: PathBuf,
    text: String,
    params: Vec<(String, Mode, Shape)>,
    /// `None` for a function that returns nothing.
    result: Option<Shape>,
}

/// The function `item` of the package at `root`, read from a check of it.
fn signature(root: &Path, item: &str) -> Result<Function, String> {
    let (sources, mut diagnostics, loaded) = crate::load(root);
    let Some(crate::Loaded { units, packages, files, .. }) = loaded else {
        return Err(wrela_diag::render::render_all(&sources, &diagnostics));
    };
    let checked = wrela_sema::check_packages(units, &packages, &mut diagnostics);
    if has_errors(&diagnostics) {
        let errors: Vec<_> = diagnostics.into_iter().filter(|d| d.is_error()).collect();
        return Err(format!(
            "the package has errors\n{}",
            wrela_diag::render::render_all(&sources, &errors)
        ));
    }
    let ix = Index::new(&checked, &sources);
    let (f, path) = ix.one_fn(item)?;
    let p = &checked.program;
    let def = p.func(f);
    if def.owner != FnOwner::Free {
        return Err(format!("`{path}` is a method: a behaviour diff takes a free function"));
    }
    if p.package_of(def.module).kind != PackageKind::Program {
        return Err(format!(
            "`{path}` is in the package `{}`, not this one: diff that package's directory",
            p.package_of(def.module).name
        ));
    }
    if !def.generics.is_empty() {
        return Err(format!("`{path}` is generic: a behaviour diff can't choose its types"));
    }
    let fx = Effects::build(p, &checked.mir);
    let (why, _) = fx.explain(f).ok_or_else(|| format!("`{path}` has no body"))?;
    if let Some(w) =
        why.iter().find(|w| matches!(w.effect, Effect::Io | Effect::Nondet | Effect::Host))
    {
        return Err(format!(
            "`{path}` has the `{}` effect ({}: {}, at {}), so what it does isn't a function of its arguments",
            w.effect.name(),
            w.chain.join(" → "),
            w.what,
            sources.file(w.span.file).location(w.span.start),
        ));
    }
    let mut params = Vec::new();
    for param in &def.params {
        let shape = shape(p, def.module, param.ty).ok_or_else(|| {
            format!(
                "`{path}`'s parameter `{}` is a `{}`: {}",
                param.name,
                p.display_ty(param.ty),
                TAKES
            )
        })?;
        params.push((param.name.clone(), param.mode, shape));
    }
    let unit = matches!(p.types.kind(def.ret), TyKind::Tuple(t) if t.is_empty());
    let result =
        if unit {
            None
        } else if def.ret_mode != RetMode::Owned || def.opaque.is_some() {
            return Err(format!("`{path}` returns a projection or a trait's type: {TAKES}"));
        } else {
            Some(shape(p, def.module, def.ret).ok_or_else(|| {
                format!("`{path}` returns a `{}`: {TAKES}", p.display_ty(def.ret))
            })?)
        };
    let file = files
        .iter()
        .find(|(id, _, _)| *id == def.span.file)
        .map(|(_, _, path)| path.clone())
        .ok_or_else(|| format!("`{path}`'s file isn't one of the package's"))?;
    let file = file.canonicalize().map_err(|e| format!("`{}`: {e}", file.display()))?;
    let text = sources.file(def.span.file).text.clone();
    Ok(Function { path, name: def.name.clone(), file, text, params, result })
}

/// What a behaviour diff can sample and compare.
const TAKES: &str = "a behaviour diff takes and compares `bool`, `i32`, `u32`, `f32`, their vectors, and structs, tuples and arrays of them";

/// Type `t`'s shape, written as module `from` names it; `None` if it isn't made of numbers a
/// diff can sample.
fn shape(p: &Program, from: ModuleId, t: TyId) -> Option<Shape> {
    let num = |t: &TyKind| match t {
        TyKind::Bool => Some(Num::Bool),
        TyKind::Int(IntTy::I32) => Some(Num::I32),
        TyKind::Int(IntTy::U32) => Some(Num::U32),
        TyKind::Float(FloatTy::F32) => Some(Num::F32),
        _ => None,
    };
    Some(match p.types.kind(t) {
        TyKind::Vec(e, n) if *e != VecElem::F64 => Shape::Vec(num(&e.kind())?, *n),
        TyKind::Tuple(parts) if !parts.is_empty() => {
            Shape::Tuple(parts.iter().map(|&t| shape(p, from, t)).collect::<Option<_>>()?)
        }
        TyKind::Array(e, n) => Shape::Array(Box::new(shape(p, from, *e)?), *n),
        TyKind::Adt(a, args) if args.is_empty() => {
            let def = p.adt(*a);
            let AdtKind::Struct(fields) = &def.kind else { return None };
            if def.borrow || fields.is_empty() {
                return None;
            }
            let fields = fields
                .iter()
                .map(|f| Some((f.name.clone(), shape(p, from, f.ty)?)))
                .collect::<Option<_>>()?;
            Shape::Struct(adt_path(p, from, def.module, &def.name)?, fields)
        }
        k => Shape::Num(num(k)?),
    })
}

/// The path that module `from` names an item `name` of module `m` by: from its package's root,
/// through the dependency's name if it's another package's.
fn adt_path(p: &Program, from: ModuleId, m: ModuleId, name: &str) -> Option<String> {
    let module = p.module(m);
    let package = &p.packages[module.package.index()];
    let root = p.module(package.root).path.len();
    let mut path: Vec<String> = if p.same_package(from, m) {
        Vec::new()
    } else if package.kind == PackageKind::Std {
        vec!["std".into()]
    } else {
        let deps = &p.package_of(from).deps;
        vec![deps.iter().find(|(_, id)| **id == module.package)?.0.clone()]
    };
    path.extend(module.path[root.min(module.path.len())..].iter().cloned());
    path.push(name.to_string());
    Some(path.join("::"))
}

impl Shape {
    /// The kinds of its numbers, in order: fields, then elements, then components.
    fn leaves(&self, out: &mut Vec<Num>) {
        match self {
            Shape::Num(n) => out.push(*n),
            Shape::Vec(n, k) => out.extend(std::iter::repeat_n(*n, *k as usize)),
            Shape::Struct(_, fields) => fields.iter().for_each(|(_, s)| s.leaves(out)),
            Shape::Tuple(parts) => parts.iter().for_each(|s| s.leaves(out)),
            Shape::Array(e, n) => (0..*n).for_each(|_| e.leaves(out)),
        }
    }

    /// The type's structure, without the names of its structs, as the two versions must share
    /// it.
    fn skeleton(&self) -> String {
        match self {
            Shape::Num(n) => n.name().into(),
            Shape::Vec(n, k) => format!("vec{k}<{}>", n.name()),
            Shape::Struct(_, fields) => {
                let f: Vec<String> =
                    fields.iter().map(|(n, s)| format!("{n}: {}", s.skeleton())).collect();
                format!("{{{}}}", f.join(", "))
            }
            Shape::Tuple(parts) => {
                let p: Vec<String> = parts.iter().map(Shape::skeleton).collect();
                format!("({})", p.join(", "))
            }
            Shape::Array(e, n) => format!("[{}; {n}]", e.skeleton()),
        }
    }

    /// The value made of `leaves`' next numbers, as Wrela source in the function's module (for
    /// the harness), or as a person reads it (`shown`).
    fn write(&self, leaves: &mut std::slice::Iter<Leaf>, shown: bool) -> String {
        match self {
            Shape::Num(_) => {
                let leaf = leaves.next().expect("a sample has a number for each leaf");
                if shown { leaf.shown() } else { leaf.source() }
            }
            Shape::Vec(n, k) => {
                let c: Vec<String> = (0..*k).map(|_| Shape::Num(*n).write(leaves, shown)).collect();
                format!("vec{k}{}({})", n.vec_suffix(), c.join(", "))
            }
            Shape::Struct(path, fields) => {
                let f: Vec<String> = fields
                    .iter()
                    .map(|(n, s)| format!("{n}: {}", s.write(leaves, shown)))
                    .collect();
                let name = if shown { path.rsplit("::").next().unwrap_or(path) } else { path };
                format!("{name} {{ {} }}", f.join(", "))
            }
            Shape::Tuple(parts) => {
                let p: Vec<String> = parts.iter().map(|s| s.write(leaves, shown)).collect();
                if p.len() == 1 { format!("({},)", p[0]) } else { format!("({})", p.join(", ")) }
            }
            Shape::Array(e, n) => {
                let p: Vec<String> = (0..*n).map(|_| e.write(leaves, shown)).collect();
                format!("[{}]", p.join(", "))
            }
        }
    }

    /// The harness's f-string holes that print `expr`'s numbers, in order.
    fn printed(&self, expr: &str, out: &mut Vec<String>) {
        match self {
            Shape::Num(Num::F32) => out.push(format!("{{bitcast_u32({expr})}}")),
            Shape::Num(_) => out.push(format!("{{{expr}}}")),
            Shape::Vec(n, k) => {
                for c in &["x", "y", "z", "w"][..*k as usize] {
                    Shape::Num(*n).printed(&format!("{expr}.{c}"), out);
                }
            }
            Shape::Struct(_, fields) => {
                fields.iter().for_each(|(n, s)| s.printed(&format!("{expr}.{n}"), out))
            }
            Shape::Tuple(parts) => {
                parts.iter().enumerate().for_each(|(i, s)| s.printed(&format!("{expr}.{i}"), out))
            }
            Shape::Array(e, n) => (0..*n).for_each(|i| e.printed(&format!("{expr}[{i}]"), out)),
        }
    }
}

impl Num {
    fn name(self) -> &'static str {
        match self {
            Num::Bool => "bool",
            Num::I32 => "i32",
            Num::U32 => "u32",
            Num::F32 => "f32",
        }
    }

    fn vec_suffix(self) -> &'static str {
        match self {
            Num::I32 => "i",
            Num::U32 => "u",
            _ => "",
        }
    }

    /// A printed number of this kind, read back.
    fn parse(self, s: &str) -> Option<Leaf> {
        Some(match self {
            Num::Bool => Leaf::Bool(s.parse().ok()?),
            Num::I32 => Leaf::I32(s.parse().ok()?),
            Num::U32 => Leaf::U32(s.parse().ok()?),
            Num::F32 => Leaf::F32(s.parse().ok()?),
        })
    }

    /// Its boundary values: zeros, ones, the ends of its range, and (for a float) the smallest
    /// normal and values near 1e-30 and 1e30.
    fn boundaries(self) -> Vec<Leaf> {
        match self {
            Num::Bool => vec![Leaf::Bool(false), Leaf::Bool(true)],
            Num::I32 => [0, 1, -1, i32::MAX, i32::MIN].map(Leaf::I32).to_vec(),
            Num::U32 => [0, 1, u32::MAX].map(Leaf::U32).to_vec(),
            Num::F32 => [0.0, -0.0, 1.0, -1.0, f32::MIN_POSITIVE, f32::MAX, 1e-30, 1e30]
                .map(|x: f32| Leaf::F32(x.to_bits()))
                .to_vec(),
        }
    }

    /// A seeded value: mostly small, and over a few magnitudes.
    fn seeded(self, rng: &mut Rng) -> Leaf {
        let pick = rng.next() % 4;
        match self {
            Num::Bool => Leaf::Bool(rng.next() & 1 == 1),
            Num::I32 => Leaf::I32(match pick {
                0 | 1 => rng.below(33) as i32 - 16,
                2 => rng.below(2001) as i32 - 1000,
                _ => rng.next() as i32,
            }),
            Num::U32 => Leaf::U32(match pick {
                0 | 1 => rng.below(17) as u32,
                2 => rng.below(1001) as u32,
                _ => rng.next() as u32,
            }),
            Num::F32 => {
                let x = match pick {
                    0 | 1 => rng.unit() * 2.0 - 1.0,
                    2 => rng.unit() * 200.0 - 100.0,
                    _ => {
                        let sign = if rng.next() & 1 == 1 { -1.0 } else { 1.0 };
                        sign * 10f64.powf(rng.unit() * 12.0 - 6.0)
                    }
                };
                Leaf::F32((x as f32).to_bits())
            }
        }
    }
}

impl Leaf {
    /// As the harness writes it: a float by its bits, through the harness's helper.
    fn source(self) -> String {
        match self {
            Leaf::Bool(b) => b.to_string(),
            Leaf::I32(i32::MIN) => "(-2147483647 - 1)".into(),
            Leaf::I32(v) if v < 0 => format!("({v})"),
            Leaf::I32(v) => v.to_string(),
            Leaf::U32(v) => v.to_string(),
            Leaf::F32(bits) => format!("{F32_HELPER}({bits})"),
        }
    }

    /// As a person reads it: a float shortest, as it reads back (`-0.0`, `1e30`, `NaN`).
    fn shown(self) -> String {
        match self {
            Leaf::Bool(b) => b.to_string(),
            Leaf::I32(v) => v.to_string(),
            Leaf::U32(v) => v.to_string(),
            Leaf::F32(bits) => format!("{:?}", f32::from_bits(bits)),
        }
    }

    /// Whether two numbers are the same: floats bit for bit, or within `within` (two NaNs
    /// agree, and infinities agree only with themselves).
    fn agrees(self, other: Leaf, within: Option<f64>) -> bool {
        match (self, other, within) {
            (Leaf::F32(a), Leaf::F32(b), Some(e)) if a != b => {
                let (a, b) = (f32::from_bits(a) as f64, f32::from_bits(b) as f64);
                (a.is_nan() && b.is_nan()) || (a - b).abs() <= e
            }
            _ => self == other,
        }
    }
}

/// The harness's names: its helper that makes a float from its bits, each sample's test, and
/// what starts the line each prints.
const F32_HELPER: &str = "wrela_diff_f32";
const SAMPLE: &str = "wrela_diff_sample_";
const MARK: &str = "wrela-diff:";

/// A seeded generator: splitmix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// In 0..n.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// In [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The samples for numbers of kinds `nums`: the boundary samples (every number at its kind's
/// first boundary value, then its second, and so on; then each number at each of its boundary
/// values in turn, the others seeded, to at most [`MAX_BOUNDARY`] in all), and the seeded ones.
fn samples(nums: &[Num], options: Options) -> (Vec<Vec<Leaf>>, Vec<Vec<Leaf>>) {
    let mut rng = Rng(options.seed);
    let seeded = |rng: &mut Rng| nums.iter().map(|n| n.seeded(rng)).collect::<Vec<Leaf>>();
    let mut boundary: Vec<Vec<Leaf>> = Vec::new();
    let widest = nums.iter().map(|n| n.boundaries().len()).max().unwrap_or(0);
    for k in 0..widest {
        let s: Vec<Leaf> = nums
            .iter()
            .map(|n| {
                let b = n.boundaries();
                b[k % b.len()]
            })
            .collect();
        if !boundary.contains(&s) {
            boundary.push(s);
        }
    }
    'all: for (i, n) in nums.iter().enumerate() {
        for b in n.boundaries() {
            if boundary.len() >= MAX_BOUNDARY {
                break 'all;
            }
            let mut s = seeded(&mut rng);
            s[i] = b;
            if !boundary.contains(&s) {
                boundary.push(s);
            }
        }
    }
    let seeded = (0..options.samples).map(|_| seeded(&mut rng)).collect();
    (boundary, seeded)
}

/// How a version's call of one sample went: how it ended, and the numbers it gave (its result's,
/// then each `mut` argument's) if it returned.
#[derive(Debug, PartialEq)]
struct Run {
    ending: Ending,
    numbers: Vec<Leaf>,
}

fn agree(a: &Run, b: &Run, within: Option<f64>) -> bool {
    a.ending == b.ending
        && a.numbers.len() == b.numbers.len()
        && a.numbers.iter().zip(&b.numbers).all(|(x, y)| x.agrees(*y, within))
}

impl Function {
    fn leaves(&self) -> Vec<Num> {
        let mut out = Vec::new();
        self.params.iter().for_each(|(_, _, s)| s.leaves(&mut out));
        out
    }

    /// The kinds of the numbers a call gives: its result's, then each `mut` argument's.
    fn outputs(&self) -> Vec<Num> {
        let mut out = Vec::new();
        if let Some(r) = &self.result {
            r.leaves(&mut out);
        }
        self.muts().for_each(|(_, s)| s.leaves(&mut out));
        out
    }

    fn muts(&self) -> impl Iterator<Item = (&String, &Shape)> {
        self.params.iter().filter(|(_, m, _)| *m == Mode::Mut).map(|(n, _, s)| (n, s))
    }

    fn skeleton(&self) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|(_, m, s)| match m {
                Mode::Borrow => s.skeleton(),
                m => format!("{} {}", m.keyword(), s.skeleton()),
            })
            .collect();
        let result = self.result.as_ref().map_or("()".into(), Shape::skeleton);
        format!("({}) -> {result}", params.join(", "))
    }

    /// The harness: a helper, and a test of code for each sample that calls the function and
    /// prints the numbers it gives.
    fn harness(&self, samples: &[Vec<Leaf>]) -> String {
        let mut s = format!(
            "\n\n// The harness of `wrela diff-behaviour`.\nfn {F32_HELPER}(bits: u32) -> f32 {{\n    bitcast_f32(bits)\n}}\n"
        );
        let mut holes = Vec::new();
        if let Some(r) = &self.result {
            r.printed("wrela_diff_out", &mut holes);
        }
        for (i, (_, s)) in self.muts().enumerate() {
            s.printed(&format!("wrela_diff_mut{i}"), &mut holes);
        }
        for (k, sample) in samples.iter().enumerate() {
            let mut leaves = sample.iter();
            let mut lets = String::new();
            let mut args = Vec::new();
            let mut muts = 0;
            for (_, mode, shape) in &self.params {
                let value = shape.write(&mut leaves, false);
                if *mode == Mode::Mut {
                    lets.push_str(&format!("    var wrela_diff_mut{muts} = {value}\n"));
                    args.push(format!("mut wrela_diff_mut{muts}"));
                    muts += 1;
                } else {
                    args.push(value);
                }
            }
            let call = format!("{}({})", self.name, args.join(", "));
            let call =
                if self.result.is_some() { format!("let wrela_diff_out = {call}") } else { call };
            s.push_str(&format!(
                "\n@test\nfn {SAMPLE}{k}() {{\n{lets}    {call}\n    std::io::print(f\"{MARK} {}\")\n}}\n",
                holes.join(" ")
            ));
        }
        s
    }

    /// The parameters' values in `sample`, as a person reads them.
    fn show_inputs(&self, sample: &[Leaf]) -> Vec<(String, String)> {
        let mut leaves = sample.iter();
        self.params.iter().map(|(n, _, s)| (n.clone(), s.write(&mut leaves, true))).collect()
    }

    fn outcome(&self, run: &Run) -> Outcome {
        let mut leaves = run.numbers.iter();
        let returned = run.ending == Ending::Returned;
        let result = self.result.as_ref().filter(|_| returned).map(|r| r.write(&mut leaves, true));
        let muts = if returned {
            self.muts().map(|(n, s)| (n.clone(), s.write(&mut leaves, true))).collect()
        } else {
            Vec::new()
        };
        Outcome { ending: run.ending.clone(), result, muts }
    }
}

/// Builds the package at `root` with `f`'s harness for `samples`, and runs each sample.
fn run(root: &Path, f: &Function, samples: &[Vec<Leaf>]) -> Result<Vec<Run>, String> {
    let mut overlay = crate::Overlay::new();
    overlay.insert(f.file.clone(), format!("{}{}", f.text, f.harness(samples)));
    let (sources, mut diagnostics, loaded) = crate::load_with(root, &overlay);
    let Some(crate::Loaded { units, packages, dirs, .. }) = loaded else {
        return Err(wrela_diag::render::render_all(&sources, &diagnostics));
    };
    let checked = wrela_sema::check_packages(units, &packages, &mut diagnostics);
    // A release build's code, as the program ships it.
    let mut data = wrela_lower::BuildData::default();
    let cache = crate::const_cache::dirs_of(root);
    crate::prepare(&checked, &sources, &dirs, &mut data, &mut diagnostics, &cache);
    let harness_failed = |diagnostics: Vec<wrela_diag::Diagnostic>| {
        let errors: Vec<_> = diagnostics.into_iter().filter(|d| d.is_error()).collect();
        format!(
            "the harness that calls `{}` doesn't build\n{}",
            f.path,
            wrela_diag::render::render_all(&sources, &errors)
        )
    };
    if has_errors(&diagnostics) {
        return Err(harness_failed(diagnostics));
    }
    let p = &checked.program;
    let mut fns = vec![None; samples.len()];
    for (i, def) in p.fns.iter().enumerate() {
        if def.owner == FnOwner::Free
            && p.package_of(def.module).kind == PackageKind::Program
            && let Some(k) = def.name.strip_prefix(SAMPLE).and_then(|k| k.parse::<usize>().ok())
            && let Some(slot) = fns.get_mut(k)
        {
            *slot = Some(wrela_sema::ty::FnId(i as u32));
        }
    }
    let fns: Vec<_> = fns.into_iter().collect::<Option<_>>().ok_or("the harness lost a sample")?;
    let (ran, d) = consts::run_calls(&checked, &data, &fns, consts::FUEL);
    if has_errors(&d) {
        return Err(harness_failed(d));
    }
    let outputs = f.outputs();
    ran.into_iter()
        .map(|(ending, traced)| {
            let ending = ending?;
            let mut numbers = Vec::new();
            if ending == Ending::Returned {
                let line = traced
                    .lines
                    .iter()
                    .rev()
                    .find_map(|l| l.strip_prefix(MARK))
                    .ok_or("a sample returned without printing its numbers")?;
                let words: Vec<&str> = line.split_whitespace().collect();
                if words.len() != outputs.len() {
                    return Err(format!(
                        "a sample printed `{line}`, not {} numbers",
                        outputs.len()
                    ));
                }
                for (w, n) in words.iter().zip(&outputs) {
                    numbers.push(n.parse(w).ok_or_else(|| format!("a sample printed `{w}`"))?);
                }
            }
            Ok(Run { ending, numbers })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A package of one module, `calc.wrela`, holding `source`, in a new directory `name`.
    fn package(name: &str, source: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("wrela-behaviour-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("calc.wrela"), source).expect("write");
        dir
    }

    fn diff_of(name: &str, old: &str, new: &str, item: &str, options: Options) -> Report {
        let a = package(&format!("{name}-old"), old);
        let b = package(&format!("{name}-new"), new);
        let report = diff(&a, &b, item, options).unwrap_or_else(|e| panic!("{e}"));
        let _ = std::fs::remove_dir_all(a);
        let _ = std::fs::remove_dir_all(b);
        report
    }

    const FEW: Options = Options { samples: 32, seed: 7, within: None };

    #[test]
    fn a_refactor_has_no_differences() {
        let old = "pub fn blend(a: f32, b: f32, t: f32) -> f32 {\n    let d = b - a\n    let s = t * t\n    a + d * s\n}\n";
        let new = "pub fn blend(a: f32, b: f32, t: f32) -> f32 {\n    let squared = t * t\n    let span = b - a\n    a + span * squared\n}\n";
        let r = diff_of("refactor", old, new, "blend", FEW);
        assert_eq!(r.differing, 0, "{:?}", r.differences);
        assert_eq!(r.seeded, 32);
        assert!(r.boundary >= 8 * 3, "each number at each boundary value: {}", r.boundary);
    }

    #[test]
    fn a_branch_on_a_rare_input_is_found_by_the_boundary_samples() {
        let old = "pub fn damp(x: f32) -> f32 {\n    x * 0.5\n}\n";
        let new = "pub fn damp(x: f32) -> f32 {\n    if x > 1e20 {\n        return x\n    }\n    x * 0.5\n}\n";
        let r = diff_of("rare", old, new, "damp", FEW);
        assert!(r.differing >= 1);
        assert!(r.differences.iter().all(|d| d.boundary), "{:?}", r.differences);
        let d = &r.differences[0];
        assert!(d.inputs[0].1 == "1e30" || d.inputs[0].1 == "3.4028235e38", "{:?}", d.inputs);
        assert_ne!(d.old.result, d.new.result);
    }

    #[test]
    fn a_change_of_sign_of_zero_differs_bitwise_and_passes_within() {
        let old = "pub fn lift(v: vec2) -> vec2 {\n    v * 0.0\n}\n";
        let new = "pub fn lift(v: vec2) -> vec2 {\n    vec2(0.0, 0.0)\n}\n";
        let bits = diff_of("zero-bits", old, new, "lift", FEW);
        assert!(bits.differing >= 1);
        let d = &bits.differences[0];
        assert!(d.old.result.as_deref().is_some_and(|r| r.contains("-0.0")), "{d:?}");
        let within = Options { within: Some(1e-6), ..FEW };
        let near = diff_of("zero-within", old, new, "lift", within);
        // Only an infinite or NaN input (none is sampled) would differ by more than zero.
        assert_eq!(near.differing, 0, "{:?}", near.differences);
    }

    #[test]
    fn a_mut_argument_changed_differently_is_found() {
        let old = "pub struct Acc: Copy {\n    pub sum: f32,\n    pub n: u32,\n}\n\npub fn add(acc: mut Acc, x: f32) -> bool {\n    acc.sum += x\n    acc.n += 1\n    x > 0.0\n}\n";
        let new = "pub struct Acc: Copy {\n    pub sum: f32,\n    pub n: u32,\n}\n\npub fn add(acc: mut Acc, x: f32) -> bool {\n    acc.sum += x\n    x > 0.0\n}\n";
        let r = diff_of("mut", old, new, "calc::add", FEW);
        // Every sample but those whose `n` overflows in the old version.
        assert!(r.differing > r.samples() / 2, "{} of {}", r.differing, r.samples());
        let d =
            r.differences.iter().find(|d| d.old.ending == Ending::Returned).expect("one returned");
        assert_eq!(d.old.result, d.new.result, "both return the same");
        assert_ne!(d.old.muts, d.new.muts);
        assert_eq!(d.new.muts[0].0, "acc");
    }

    #[test]
    fn a_new_panic_is_a_different_ending() {
        let old = "pub fn index(i: i32) -> i32 {\n    i * 2\n}\n";
        let new = "pub fn index(i: i32) -> i32 {\n    assert(i != 7, \"seven\")\n    i * 2\n}\n";
        let r = diff_of("panic", old, new, "index", Options { samples: 400, ..FEW });
        assert!(r.differing >= 1);
        // `i * 2` overflows and traps in both versions on most boundary samples; only 7 panics.
        for d in &r.differences {
            assert_eq!(d.inputs[0].1, "7");
            assert_eq!(d.old.ending, Ending::Returned);
            assert!(matches!(&d.new.ending, Ending::Panicked(m) if m.contains("seven")), "{d:?}");
        }
    }

    /// A float out of range for an integer and a NaN trap differently in WASM; each ending says
    /// which it was.
    #[test]
    fn a_trap_is_named_as_the_program_would_name_it() {
        let old = "pub fn whole(x: f32) -> i32 {\n    i32(sqrt(x))\n}\n";
        let new = "pub fn whole(x: f32) -> i32 {\n    0\n}\n";
        let r = diff_of("trap", old, new, "whole", Options { samples: 0, ..FEW });
        let ending = |x: &str| {
            let d = r.differences.iter().find(|d| d.inputs[0].1 == x).expect("it differs there");
            d.old.ending.clone()
        };
        assert_eq!(ending("-1.0"), Ending::Trapped("a NaN converted to an integer".into()));
        assert!(
            matches!(ending("1e30"), Ending::Trapped(w) if w.contains("a float out of range")),
            "{:?}",
            r.differences
        );
    }

    #[test]
    fn signatures_it_cannot_sample_are_refused_by_name() {
        let src = "pub fn count(xs: [f32], scale: f32) -> u32 {\n    xs.len()\n}\n";
        let a = package("refuse-a", src);
        let e = diff(&a, &a, "count", FEW).expect_err("a run isn't sampled");
        assert!(e.contains("parameter `xs`"), "{e}");
        let _ = std::fs::remove_dir_all(a);
    }

    #[test]
    fn functions_with_host_effects_are_refused() {
        let src = "use std::time::now\n\npub fn late(x: f32) -> f32 {\n    x + f32(now())\n}\n";
        let a = package("nondet", src);
        let e = diff(&a, &a, "late", FEW).expect_err("the clock is nondet");
        assert!(e.contains("`nondet` effect"), "{e}");
        let _ = std::fs::remove_dir_all(a);
    }
}
