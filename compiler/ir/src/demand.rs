//! Field demand across GPU function boundaries. An outlined function's unused result fields
//! otherwise keep entire derivative tracks alive, including their loop-carried locals and
//! arguments. Demand is unioned over callers; no code is cloned and signatures stay intact.
//!
//! Only structs are split. Arrays, vectors, enums and large structs stay whole. Calls and
//! control flow always stay, even with an unused result, so effects and termination do not
//! depend on demand. This pass is GPU-only: removing arithmetic can remove CPU traps.

use crate::*;

type Bits = u64;

struct Shapes {
    widths: Vec<u32>,
    fields: Vec<Option<Vec<(u32, TypeId)>>>,
}

fn bits(width: u32) -> Bits {
    if width == 64 { !0 } else { (1 << width) - 1 }
}

impl Shapes {
    fn new(types: &Types) -> Self {
        let mut out = Self { widths: Vec::new(), fields: Vec::new() };
        for (_, t) in types.iter() {
            let mut width = 1;
            let mut parts = None;
            if let TypeDef::Struct { fields, .. } = t {
                let size: u32 = fields.iter().map(|(_, t)| out.widths[t.index()]).sum();
                if size <= 64 && !fields.is_empty() {
                    width = size;
                    let mut offset = 0;
                    parts = Some(
                        fields
                            .iter()
                            .map(|(_, t)| {
                                let part = (offset, *t);
                                offset += out.widths[t.index()];
                                part
                            })
                            .collect(),
                    );
                }
            }
            out.widths.push(width);
            out.fields.push(parts);
        }
        out
    }

    fn all(&self, t: TypeId) -> Bits {
        bits(self.widths[t.index()])
    }

    fn field(&self, t: TypeId, k: u32) -> Option<(u32, TypeId)> {
        self.fields[t.index()].as_ref()?.get(k as usize).copied()
    }

    /// A fixed struct field path in the root's bit numbering. Other projections stay whole.
    fn path(&self, mut t: TypeId, path: &[Proj]) -> Option<(u32, TypeId)> {
        let mut offset = 0;
        for p in path {
            let Proj::Field(k) = p else { return None };
            let (part, next) = self.field(t, *k)?;
            offset += part;
            t = next;
        }
        Some((offset, t))
    }
}

#[derive(Default)]
struct State {
    values: Vec<Bits>,
    locals: Vec<Bits>,
}

struct Analysis<'a> {
    shapes: &'a Shapes,
    params: Vec<Vec<Bits>>,
    returns: Vec<Bits>,
    states: Vec<State>,
    changed: bool,
}

fn add(to: &mut Bits, more: Bits, changed: &mut bool) {
    *changed |= more & !*to != 0;
    *to |= more;
}

impl Analysis<'_> {
    fn value(&mut self, st: &mut State, v: ValueId, need: Bits) {
        add(&mut st.values[v.index()], need, &mut self.changed);
    }

    fn whole(&mut self, f: &Function, st: &mut State, v: ValueId) {
        self.value(st, v, self.shapes.all(f.value_ty(v)));
    }

    fn place(&mut self, f: &Function, at: usize, st: &mut State, p: &Place, need: Bits) {
        p.for_each_value(&mut |v| self.whole(f, st, v));
        match p.root {
            PlaceRoot::Local(l) => {
                let ty = f.locals[l.index()].ty;
                let need =
                    self.shapes.path(ty, &p.path).map_or(self.shapes.all(ty), |(offset, part)| {
                        (need & self.shapes.all(part)) << offset
                    });
                add(&mut st.locals[l.index()], need, &mut self.changed);
            }
            PlaceRoot::Param(i) => {
                let all = self.shapes.all(f.params[i as usize].ty);
                add(&mut self.params[at][i as usize], all, &mut self.changed);
            }
            _ => {}
        }
    }

    fn stored(&self, f: &Function, st: &State, p: &Place, v: ValueId) -> Bits {
        let all = self.shapes.all(f.value_ty(v));
        let PlaceRoot::Local(l) = p.root else { return all };
        let needed = st.locals[l.index()];
        self.shapes
            .path(f.locals[l.index()].ty, &p.path)
            .map_or(if needed == 0 { 0 } else { all }, |(offset, _)| (needed >> offset) & all)
    }

    fn expr(&mut self, f: &Function, at: usize, st: &mut State, e: &Expr, need: Bits) {
        match e {
            Expr::Call(g, args) => {
                add(&mut self.returns[g.index()], need, &mut self.changed);
                for (i, arg) in args.iter().enumerate() {
                    match arg {
                        Arg::Value(v) => self.value(st, *v, self.params[g.index()][i]),
                        Arg::Place(p) => self.place(f, at, st, p, !0),
                    }
                }
            }
            _ if need == 0 && !e.has_effect() => {}
            Expr::Param(i) => add(&mut self.params[at][*i as usize], need, &mut self.changed),
            Expr::Load(p) => self.place(f, at, st, p, need),
            Expr::Extract(v, k) => {
                let ty = f.value_ty(*v);
                let need = self
                    .shapes
                    .field(ty, *k)
                    .map_or(self.shapes.all(ty), |(offset, _)| need << offset);
                self.value(st, *v, need);
            }
            Expr::Construct(t, xs) if self.shapes.fields[t.index()].is_some() => {
                for (k, v) in xs.iter().enumerate() {
                    let (offset, part) = self.shapes.field(*t, k as u32).expect("struct field");
                    self.value(st, *v, (need >> offset) & self.shapes.all(part));
                }
            }
            Expr::Select { cond, if_true, if_false } => {
                self.whole(f, st, *cond);
                self.value(st, *if_true, need);
                self.value(st, *if_false, need);
            }
            _ => {
                e.for_each_value(&mut |v| self.whole(f, st, v));
                e.for_each_place(&mut |p| self.place(f, at, st, p, !0));
            }
        }
    }

    fn block(&mut self, f: &Function, at: usize, st: &mut State, b: &Block) {
        // Reverse order follows most SSA dependencies in one sweep. Further sweeps reach
        // loop-carried locals and demand communicated between callers and callees.
        for s in b.iter().rev() {
            match s {
                Stmt::Let(v, e) => {
                    let need = st.values[v.index()];
                    self.expr(f, at, st, e, need);
                }
                Stmt::Eval(e) => self.expr(f, at, st, e, 0),
                Stmt::Store(p, v) => {
                    let need = self.stored(f, st, p, *v);
                    if need != 0 {
                        self.value(st, *v, need);
                        p.for_each_value(&mut |v| self.whole(f, st, v));
                    }
                }
                Stmt::Return(Some(v)) => self.value(st, *v, self.returns[at]),
                Stmt::If { cond, then, else_ } => {
                    self.whole(f, st, *cond);
                    self.block(f, at, st, then);
                    self.block(f, at, st, else_);
                }
                Stmt::Loop { body, continuing } => {
                    self.block(f, at, st, continuing);
                    self.block(f, at, st, body);
                }
                _ => {}
            }
        }
    }
}

/// Union demanded fields across the GPU module, then erase unobserved computations. Keep
/// function signatures stable: dead fields and arguments are typed zeros at each boundary.
pub(crate) fn prune_gpu(m: &mut Module) {
    let shapes = Shapes::new(&m.types);
    let mut a = Analysis {
        shapes: &shapes,
        params: m.functions.iter().map(|f| vec![0; f.params.len()]).collect(),
        returns: vec![0; m.functions.len()],
        states: m
            .functions
            .iter()
            .map(|f| State { values: vec![0; f.values.len()], locals: vec![0; f.locals.len()] })
            .collect(),
        changed: false,
    };
    for entry in &m.entry_points {
        if let Some(t) = m.functions[entry.function.index()].ret {
            a.returns[entry.function.index()] = shapes.all(t);
        }
    }
    // Aliased and dynamically indexed locals stay whole. No new reasoning about pointer
    // lifetimes, index validity or mutation is introduced by this field-demand pass.
    for (i, f) in m.functions.iter().enumerate() {
        for (j, p) in f.params.iter().enumerate() {
            if p.by_ref {
                a.params[i][j] = shapes.all(p.ty);
            }
        }
        visit::walk(&f.body, &mut |s| {
            s.for_each_place(&mut |p| {
                if let PlaceRoot::Local(l) = p.root {
                    let borrowed =
                        matches!(s.expr(), Some(Expr::Call(..) | Expr::Addr(_) | Expr::Run(_)));
                    if borrowed || shapes.path(f.locals[l.index()].ty, &p.path).is_none() {
                        a.states[i].locals[l.index()] = shapes.all(f.locals[l.index()].ty);
                    }
                }
            });
        });
    }
    loop {
        a.changed = false;
        for (i, f) in m.functions.iter().enumerate().rev() {
            let mut st = std::mem::take(&mut a.states[i]);
            a.block(f, i, &mut st, &f.body);
            a.states[i] = st;
        }
        if !a.changed {
            break;
        }
    }
    let Analysis { params, returns, states, .. } = a;
    for (i, f) in m.functions.iter_mut().enumerate() {
        let body = std::mem::take(&mut f.body);
        f.body = rewrite(f, body, &shapes, &states[i], &params, returns[i]);
    }
}

/// Keep exactly the requested fields at an interface. This adds only extracts, constructs
/// and zero values; subsequent ordinary forwarding removes redundant aggregate operations.
fn masked(f: &mut Function, b: &mut Block, shapes: &Shapes, v: ValueId, need: Bits) -> ValueId {
    let t = f.value_ty(v);
    if need == shapes.all(t) {
        return v;
    }
    if need == 0 {
        return f.let_(b, t, Expr::Zero(t));
    }
    let Some(fields) = &shapes.fields[t.index()] else { return v };
    let mut parts = Vec::with_capacity(fields.len());
    for (k, &(offset, part)) in fields.iter().enumerate() {
        let part_need = (need >> offset) & shapes.all(part);
        let value = if part_need == 0 {
            f.let_(b, part, Expr::Zero(part))
        } else {
            let x = f.let_(b, part, Expr::Extract(v, k as u32));
            masked(f, b, shapes, x, part_need)
        };
        parts.push(value);
    }
    f.let_(b, t, Expr::Construct(t, parts))
}

fn rewrite(
    f: &mut Function,
    body: Block,
    shapes: &Shapes,
    st: &State,
    params: &[Vec<Bits>],
    ret: Bits,
) -> Block {
    let mut out = Vec::with_capacity(body.len());
    for mut s in body {
        match &mut s {
            Stmt::Let(v, e) if st.values[v.index()] == 0 && !e.has_effect() => continue,
            Stmt::Eval(e) if !e.has_effect() => continue,
            Stmt::Let(v, Expr::Construct(t, xs)) if shapes.fields[t.index()].is_some() => {
                for (k, x) in xs.iter_mut().enumerate() {
                    let (offset, part) = shapes.field(*t, k as u32).expect("struct field");
                    if (st.values[v.index()] >> offset) & shapes.all(part) == 0 {
                        *x = f.let_(&mut out, part, Expr::Zero(part));
                    }
                }
            }
            Stmt::Store(p, v) => {
                if let PlaceRoot::Local(l) = p.root {
                    let need = st.locals[l.index()];
                    if need == 0 {
                        continue;
                    }
                    let at = shapes.path(f.locals[l.index()].ty, &p.path);
                    if let Some((offset, _)) = at {
                        let part = (need >> offset) & shapes.all(f.value_ty(*v));
                        if part == 0 {
                            continue;
                        }
                        *v = masked(f, &mut out, shapes, *v, part);
                    }
                }
            }
            Stmt::Return(Some(v)) => *v = masked(f, &mut out, shapes, *v, ret),
            Stmt::If { then, else_, .. } => {
                *then = rewrite(f, std::mem::take(then), shapes, st, params, ret);
                *else_ = rewrite(f, std::mem::take(else_), shapes, st, params, ret);
            }
            Stmt::Loop { body, continuing } => {
                *body = rewrite(f, std::mem::take(body), shapes, st, params, ret);
                *continuing = rewrite(f, std::mem::take(continuing), shapes, st, params, ret);
            }
            _ => {}
        }
        if let Stmt::Let(_, Expr::Call(g, args)) | Stmt::Eval(Expr::Call(g, args)) = &mut s {
            for (i, arg) in args.iter_mut().enumerate() {
                if let Arg::Value(v) = arg {
                    *v = masked(f, &mut out, shapes, *v, params[g.index()][i]);
                }
            }
        }
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(effect: bool, other_field: bool) -> Module {
        let mut m = Module::default();
        let t = m.types.f32();
        let u = m.types.u32();
        let pair = m.types.intern(TypeDef::Struct {
            name: "Pair".into(),
            fields: vec![("a".into(), t), ("b".into(), t)],
        });
        m.resources.push(Resource {
            name: "out".into(),
            binding: 0,
            kind: ResourceKind::StorageReadWrite,
            ty: t,
        });
        let param = |name: &str| Param { name: name.into(), ty: t, by_ref: false, mutable: false };
        let mut helper = Function::new("helper", vec![param("x"), param("y")], Some(pair));
        let mut b = Vec::new();
        let x = helper.let_(&mut b, t, Expr::Param(0));
        let y = helper.let_(&mut b, t, Expr::Param(1));
        let sine = helper.let_(&mut b, t, Expr::Builtin(Builtin::Sin, vec![y]));
        if effect {
            let index = helper.let_(&mut b, u, Expr::Const(Const::U32(1)));
            b.push(Stmt::Store(
                Place::root(PlaceRoot::Resource(ResourceId(0))).with(Proj::Index(index)),
                sine,
            ));
        }
        let p = helper.let_(&mut b, pair, Expr::Construct(pair, vec![x, sine]));
        b.push(Stmt::Return(Some(p)));
        helper.body = b;
        let helper = m.add_function(helper);
        let mut main = Function::new("main", Vec::new(), None);
        let mut b = Vec::new();
        let x = main.let_(&mut b, t, Expr::Const(Const::F32(2.0)));
        let y = main.let_(&mut b, t, Expr::Const(Const::F32(3.0)));
        let index = main.let_(&mut b, u, Expr::Const(Const::U32(0)));
        let call = main.let_(&mut b, pair, Expr::Call(helper, vec![Arg::Value(x), Arg::Value(y)]));
        let first = main.let_(&mut b, t, Expr::Extract(call, 0));
        let output = if other_field {
            let second_call =
                main.let_(&mut b, pair, Expr::Call(helper, vec![Arg::Value(x), Arg::Value(y)]));
            let second = main.let_(&mut b, t, Expr::Extract(second_call, 1));
            main.let_(&mut b, t, Expr::Binary(BinOp::Add, first, second))
        } else {
            first
        };
        b.push(Stmt::Store(
            Place::root(PlaceRoot::Resource(ResourceId(0))).with(Proj::Index(index)),
            output,
        ));
        main.body = b;
        let main = m.add_function(main);
        m.entry_points.push(EntryPoint {
            name: "main".into(),
            function: main,
            inputs: Vec::new(),
            stage: Stage::Compute { workgroup_size: [1, 1, 1] },
        });
        m
    }

    fn has_sine(f: &Function) -> bool {
        let mut yes = false;
        visit::walk(&f.body, &mut |s| {
            yes |= matches!(s.expr(), Some(Expr::Builtin(Builtin::Sin, _)))
        });
        yes
    }

    #[test]
    fn unused_result_fields_stop_demanding_their_call_arguments() {
        let mut m = module(false, false);
        verify(&m).expect("initial IR");
        prune_gpu(&mut m);
        verify(&m).expect("pruned IR");
        assert!(!has_sine(&m.functions[0]));
        assert_eq!(visit::calls(&m.functions[1].body).len(), 1, "call remains");
    }

    #[test]
    fn result_demand_is_unioned_across_callers() {
        let mut m = module(false, true);
        prune_gpu(&mut m);
        verify(&m).expect("pruned IR");
        assert!(has_sine(&m.functions[0]));
        assert_eq!(visit::calls(&m.functions[1].body).len(), 2);
    }

    #[test]
    fn external_writes_keep_arguments_even_when_their_result_field_is_unused() {
        let mut m = module(true, false);
        prune_gpu(&mut m);
        verify(&m).expect("pruned IR");
        assert!(has_sine(&m.functions[0]));
        assert!(m.functions[0].body.iter().any(|s| matches!(s, Stmt::Store(..))));
    }

    #[test]
    fn nested_results_propagate_leaf_demand() {
        let mut m = module(false, false);
        let t = m.types.f32();
        let pair = m.functions[0].ret.unwrap();
        let outer = m.types.intern(TypeDef::Struct {
            name: "Outer".into(),
            fields: vec![("pad".into(), t), ("nested".into(), pair)],
        });
        let f = &mut m.functions[0];
        let Stmt::Return(Some(value)) = f.body.pop().unwrap() else { panic!("return") };
        let mut end = Vec::new();
        let pad = f.let_(&mut end, t, Expr::Zero(t));
        let result = f.let_(&mut end, outer, Expr::Construct(outer, vec![pad, value]));
        end.push(Stmt::Return(Some(result)));
        f.body.extend(end);
        f.ret = Some(outer);
        let f = &mut m.functions[1];
        f.values[3] = outer;
        let mut projection = Vec::new();
        let nested = f.let_(&mut projection, pair, Expr::Extract(ValueId(3), 1));
        f.body[4] = Stmt::Let(ValueId(4), Expr::Extract(nested, 0));
        f.body.splice(4..4, projection);
        verify(&m).expect("initial nested IR");
        prune_gpu(&mut m);
        verify(&m).expect("pruned nested IR");
        assert!(!has_sine(&m.functions[0]));
    }

    #[test]
    fn projected_stores_only_demand_the_observed_field() {
        let mut m = module(false, false);
        let t = m.types.f32();
        let pair = m.functions[0].ret.unwrap();
        let f = &mut m.functions[1];
        let local = f.new_local("state", pair);
        let Stmt::Store(out, _) = f.body.pop().unwrap() else { panic!("output") };
        let mut end = Vec::new();
        let second = f.let_(&mut end, t, Expr::Extract(ValueId(3), 1));
        end.push(Stmt::Store(Place::local(local).with(Proj::Field(0)), ValueId(4)));
        end.push(Stmt::Store(Place::local(local).with(Proj::Field(1)), second));
        let first = f.let_(&mut end, t, Expr::Load(Place::local(local).with(Proj::Field(0))));
        end.push(Stmt::Store(out, first));
        f.body.extend(end);
        verify(&m).expect("initial projected store");
        prune_gpu(&mut m);
        verify(&m).expect("pruned projected store");
        assert!(!has_sine(&m.functions[0]));
        assert!(!m.functions[1].body.iter().any(|s| matches!(
            s, Stmt::Store(p, _) if p.root == PlaceRoot::Local(local) && p.path == [Proj::Field(1)]
        )));
    }

    #[test]
    fn borrowed_locals_keep_all_fields_even_with_an_unused_call_result() {
        let mut m = module(false, false);
        let t = m.types.f32();
        let pair = m.functions[0].ret.unwrap();
        let mut observer = Function::new(
            "observer",
            vec![Param { name: "p".into(), ty: pair, by_ref: true, mutable: false }],
            Some(t),
        );
        let mut b = Vec::new();
        let value = observer.let_(
            &mut b,
            t,
            Expr::Load(Place::root(PlaceRoot::Param(0)).with(Proj::Field(1))),
        );
        b.push(Stmt::Return(Some(value)));
        observer.body = b;
        let observer = m.add_function(observer);
        let f = &mut m.functions[1];
        let local = f.new_local("borrowed", pair);
        f.body.push(Stmt::Store(Place::local(local), ValueId(3)));
        f.body.push(Stmt::Eval(Expr::Call(observer, vec![Arg::Place(Place::local(local))])));
        verify(&m).expect("initial borrowed local");
        prune_gpu(&mut m);
        verify(&m).expect("pruned borrowed local");
        assert!(has_sine(&m.functions[0]));
        assert_eq!(visit::calls(&m.functions[1].body).len(), 2);
    }

    #[test]
    fn indexed_locals_remain_whole() {
        let mut m = module(false, false);
        let t = m.types.f32();
        let array = m.types.intern(TypeDef::Array(t, 2));
        let f = &mut m.functions[1];
        let local = f.new_local("indexed", array);
        let mut end = Vec::new();
        let second = f.let_(&mut end, t, Expr::Extract(ValueId(3), 1));
        end.push(Stmt::Store(Place::local(local).with(Proj::Index(ValueId(2))), second));
        f.body.extend(end);
        verify(&m).expect("initial indexed local");
        prune_gpu(&mut m);
        verify(&m).expect("pruned indexed local");
        assert!(has_sine(&m.functions[0]));
        assert!(m.functions[1].body.iter().any(|s| matches!(
            s, Stmt::Store(p, _) if p.root == PlaceRoot::Local(local)
        )));
    }

    #[test]
    fn field_demand_flows_through_loop_carried_aggregates() {
        let mut m = module(false, false);
        let pair = m.functions[0].ret.expect("pair");
        let t = m.types.f32();
        let f = &mut m.functions[1];
        let local = f.new_local("state", pair);
        let last = f.body.pop().expect("output store");
        f.body.push(Stmt::Store(Place::local(local), ValueId(3)));
        let mut body = Vec::new();
        let state = f.let_(&mut body, pair, Expr::Load(Place::local(local)));
        let first = f.let_(&mut body, t, Expr::Extract(state, 0));
        let next = f.let_(
            &mut body,
            pair,
            Expr::Call(FuncId(0), vec![Arg::Value(first), Arg::Value(ValueId(1))]),
        );
        body.push(Stmt::Store(Place::local(local), next));
        body.push(Stmt::Break);
        f.body.push(Stmt::Loop { body, continuing: Vec::new() });
        let mut end = Vec::new();
        let state = f.let_(&mut end, pair, Expr::Load(Place::local(local)));
        let first = f.let_(&mut end, t, Expr::Extract(state, 0));
        let Stmt::Store(p, _) = last else { panic!("output store") };
        end.push(Stmt::Store(p, first));
        f.body.extend(end);
        verify(&m).expect("initial loop");
        prune_gpu(&mut m);
        verify(&m).expect("pruned loop");
        assert!(!has_sine(&m.functions[0]));
        assert_eq!(visit::calls(&m.functions[1].body).len(), 2, "both calls remain");
        assert!(m.functions[1].body.iter().any(|s| matches!(s, Stmt::Loop { .. })));
    }
}
