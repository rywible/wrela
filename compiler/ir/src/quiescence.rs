//! GPU counted loops may stop when a guard becomes false and no later iteration can
//! change observable state. This proves a closed state, not float monotonicity: even
//! NaNs use the original comparison. The counter must be dead outside the loop and
//! count up by one toward an invariant u32 bound, so the removed tail terminates.
//! Calls, derivatives and collective operations exclude a loop. Explicit-LOD texture
//! reads and GPU arithmetic can be discarded; this reasoning does not apply on CPU.

use crate::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct Gate {
    local: LocalId,
    comparison: ValueId,
    load: ValueId,
    op: BinOp,
    constant: Const,
    ty: TypeId,
}

type Defs = HashMap<ValueId, Expr>;

/// Run after GPU inlining and local load forwarding. No function ABI changes.
pub(crate) fn eliminate(f: &mut Function, types: &Types) {
    let Some(bool_ty) = types.lookup(&TypeDef::Scalar(Scalar::Bool)) else { return };
    let mut defs = HashMap::new();
    visit::walk(&f.body, &mut |s| {
        if let Stmt::Let(v, e) = s {
            defs.insert(*v, e.clone());
        }
    });
    let reads = read_counts(&f.body);
    let mut plans = HashMap::new();
    visit::walk(&f.body, &mut |s| {
        if let Stmt::Loop { body, continuing } = s
            && let Some((test, gate)) = plan(f, types, body, continuing, &defs, &reads)
        {
            plans.insert(test, gate);
        }
    });
    let body = std::mem::take(&mut f.body);
    f.body = rewrite(f, body, bool_ty, &plans);
}

fn whole(p: &Place) -> Option<LocalId> {
    if p.path.is_empty() { p.root_local() } else { None }
}

fn read_counts(b: &[Stmt]) -> HashMap<LocalId, usize> {
    let mut out = HashMap::new();
    visit::walk(b, &mut |s| {
        if let Some(e) = s.expr() {
            e.for_each_place(&mut |p| {
                if let Some(l) = p.root_local() {
                    *out.entry(l).or_default() += 1;
                }
            });
        }
    });
    out
}

fn exit(s: &Stmt) -> Option<ValueId> {
    let Stmt::If { cond, then, else_ } = s else { return None };
    let then: Vec<_> = then.iter().filter(|s| !matches!(s, Stmt::At(_))).collect();
    (matches!(then.as_slice(), [Stmt::Break]) && else_.iter().all(|s| matches!(s, Stmt::At(_))))
        .then_some(*cond)
}

fn gate(v: ValueId, defs: &Defs, f: &Function) -> Option<Gate> {
    let Expr::Binary(op, a, b) = defs.get(&v)? else { return None };
    if !op.is_comparison() {
        return None;
    }
    let Expr::Load(p) = defs.get(a)? else { return None };
    let Expr::Const(constant) = defs.get(b)? else { return None };
    Some(Gate {
        local: whole(p)?,
        comparison: v,
        load: *a,
        op: *op,
        constant: constant.clone(),
        ty: f.value_ty(*a),
    })
}

fn plan(
    f: &Function,
    types: &Types,
    body: &Block,
    continuing: &Block,
    defs: &Defs,
    all_reads: &HashMap<LocalId, usize>,
) -> Option<(ValueId, Gate)> {
    let test_pos = body.iter().position(|s| exit(s).is_some())?;
    let test = exit(&body[test_pos])?;
    let Expr::Binary(BinOp::Ge, count, bound) = defs.get(&test)? else { return None };
    let Expr::Load(p) = defs.get(count)? else { return None };
    let counter = whole(p)?;
    if types.get(f.value_ty(*count)) != &TypeDef::Scalar(Scalar::U32)
        || !body[..test_pos].iter().any(|s| matches!(s, Stmt::Let(v, _) if v == count))
    {
        return None;
    }
    // The header reads only the counter and constants, plus immutable outside values.
    if !body[..test_pos].iter().all(|s| match s {
        Stmt::At(_) => true,
        Stmt::Let(_, Expr::Load(p)) => whole(p) == Some(counter),
        Stmt::Let(_, Expr::Const(_) | Expr::Binary(..) | Expr::Unary(..)) => true,
        _ => false,
    }) {
        return None;
    }
    let mut defined = HashSet::new();
    let mut writes = HashSet::new();
    let mut forbidden = false;
    visit::walk(body, &mut |s| {
        if let Stmt::Let(v, _) = s {
            defined.insert(*v);
        }
        if let Stmt::Store(p, _) = s
            && let Some(l) = p.root_local()
        {
            writes.insert(l);
        }
        if let Some(e) = s.expr() {
            forbidden |=
                matches!(e, Expr::Call(..) | Expr::Addr(..) | Expr::Run(..) | Expr::Barrier)
                    || matches!(
                        e,
                        Expr::Builtin(Builtin::Dpdx | Builtin::Dpdy | Builtin::Fwidth, _)
                    )
                    || matches!(e, Expr::Texture(op, ..) if op.uses_derivatives());
        }
    });
    if forbidden
        || writes.contains(&counter)
        || (defined.contains(bound) && !matches!(defs.get(bound), Some(Expr::Const(Const::U32(_)))))
    {
        return None;
    }
    // Exactly counter += 1, with only the load, literal and addition feeding it.
    let mut steps = 0;
    let mut available = HashSet::new();
    for s in continuing {
        match s {
            Stmt::At(_) => {}
            Stmt::Let(_, Expr::Load(p)) if whole(p) == Some(counter) => {}
            Stmt::Let(_, Expr::Const(Const::U32(1))) => {}
            Stmt::Let(_, Expr::Binary(BinOp::Add | BinOp::WrappingAdd, a, b))
                if available.contains(a)
                    && matches!(defs.get(a), Some(Expr::Load(p)) if whole(p) == Some(counter))
                    && matches!(defs.get(b), Some(Expr::Const(Const::U32(1)))) => {}
            Stmt::Store(p, v) if whole(p) == Some(counter) => {
                let Some(Expr::Binary(BinOp::Add | BinOp::WrappingAdd, a, b)) = defs.get(v) else {
                    return None;
                };
                if !available.contains(v)
                    || !matches!(defs.get(a), Some(Expr::Load(p)) if whole(p) == Some(counter))
                    || !matches!(defs.get(b), Some(Expr::Const(Const::U32(1))))
                {
                    return None;
                }
                steps += 1;
            }
            _ => return None,
        }
        if let Stmt::Let(v, _) = s {
            available.insert(*v);
        }
    }
    if steps != 1 {
        return None;
    }
    let mut reads = read_counts(body);
    for (l, n) in read_counts(continuing) {
        *reads.entry(l).or_default() += n;
    }
    let protected: HashSet<_> = all_reads
        .iter()
        .filter_map(|(l, n)| (*n > reads.get(l).copied().unwrap_or(0)).then_some(*l))
        .collect();
    if protected.contains(&counter) {
        return None;
    }
    let mut candidates = Vec::new();
    visit::walk(body, &mut |s| {
        if let Stmt::Let(v, _) = s
            && let Some(g) = gate(*v, defs, f)
            && defined.contains(&g.load)
            && writes.contains(&g.local)
            && protected.contains(&g.local)
        {
            candidates.push(g);
        }
    });
    for g in candidates {
        let mut state = State::default();
        if quiet(&body[test_pos + 1..], &g, &protected, &mut state) {
            return Some((test, g));
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fact {
    Bool(bool),
    Symbol(ValueId),
    Unknown,
}

#[derive(Clone, Default)]
struct State {
    values: HashMap<ValueId, Fact>,
    locals: HashMap<LocalId, Fact>,
}

impl State {
    fn value(&self, v: ValueId) -> Fact {
        self.values.get(&v).copied().unwrap_or(Fact::Symbol(v))
    }

    fn expr(&self, v: ValueId, e: &Expr, g: &Gate) -> Fact {
        if v == g.comparison {
            return Fact::Bool(false);
        }
        match e {
            Expr::Const(Const::Bool(b)) => Fact::Bool(*b),
            Expr::Load(p) => {
                whole(p).and_then(|l| self.locals.get(&l).copied()).unwrap_or(Fact::Symbol(v))
            }
            Expr::Unary(UnOp::Not, a) => match self.value(*a) {
                Fact::Bool(b) => Fact::Bool(!b),
                _ => Fact::Symbol(v),
            },
            Expr::Binary(BinOp::And, a, b) => match (self.value(*a), self.value(*b)) {
                (Fact::Bool(false), _) | (_, Fact::Bool(false)) => Fact::Bool(false),
                (Fact::Bool(true), b) => b,
                (a, Fact::Bool(true)) => a,
                _ => Fact::Symbol(v),
            },
            Expr::Binary(BinOp::Or, a, b) => match (self.value(*a), self.value(*b)) {
                (Fact::Bool(true), _) | (_, Fact::Bool(true)) => Fact::Bool(true),
                (Fact::Bool(false), b) => b,
                (a, Fact::Bool(false)) => a,
                _ => Fact::Symbol(v),
            },
            Expr::Select { cond, if_true, if_false } => {
                let c = self.value(*cond);
                let (mut a, mut b) = (self.value(*if_true), self.value(*if_false));
                if c == Fact::Bool(true) {
                    return a;
                }
                if c == Fact::Bool(false) {
                    return b;
                }
                if c != Fact::Unknown && a == c {
                    a = Fact::Bool(true);
                }
                if c != Fact::Unknown && b == c {
                    b = Fact::Bool(false);
                }
                if a == b { a } else { Fact::Symbol(v) }
            }
            _ => Fact::Symbol(v),
        }
    }
}

// Dynamic bounds checks may terminate an invocation; do not discard those reads.
fn removable(e: &Expr) -> bool {
    !e.has_effect()
        && !matches!(e, Expr::ExtractDyn(..) | Expr::Addr(..) | Expr::Run(..))
        && !matches!(e, Expr::Load(p) if p.has_index())
}

fn quiet(b: &[Stmt], gate: &Gate, protected: &HashSet<LocalId>, state: &mut State) -> bool {
    for s in b {
        match s {
            Stmt::Let(v, e) if removable(e) => {
                state.values.insert(*v, state.expr(*v, e, gate));
            }
            Stmt::Eval(e) if removable(e) => {}
            Stmt::Store(p, v) => {
                let Some(l) = p.root_local() else { return false };
                // The assumed guard must stay false even if its local were otherwise dead.
                if protected.contains(&l) || l == gate.local || p.has_index() {
                    return false;
                }
                let fact = if p.path.is_empty() { state.value(*v) } else { Fact::Unknown };
                state.locals.insert(l, fact);
            }
            Stmt::If { cond, then, else_ } => {
                match state.value(*cond) {
                    Fact::Bool(c) => {
                        if !quiet(if c { then } else { else_ }, gate, protected, state) {
                            return false;
                        }
                    }
                    _ => {
                        let mut other = state.clone();
                        if !quiet(then, gate, protected, state)
                            || !quiet(else_, gate, protected, &mut other)
                        {
                            return false;
                        }
                        // Values declared in branches cannot escape their lexical block;
                        // only local stores join. Missing stores preserve the incoming fact.
                        for l in state
                            .locals
                            .keys()
                            .chain(other.locals.keys())
                            .copied()
                            .collect::<HashSet<_>>()
                        {
                            let a = state.locals.get(&l).copied().unwrap_or(Fact::Unknown);
                            let b = other.locals.get(&l).copied().unwrap_or(Fact::Unknown);
                            state.locals.insert(l, if a == b { a } else { Fact::Unknown });
                        }
                    }
                }
            }
            Stmt::At(_) => {}
            // Conservatively reject even early exits: this proof covers a full iteration.
            _ => return false,
        }
    }
    true
}

fn rewrite(f: &mut Function, b: Block, bool_ty: TypeId, plans: &HashMap<ValueId, Gate>) -> Block {
    b.into_iter()
        .map(|s| match s {
            Stmt::If { cond, then, else_ } => Stmt::If {
                cond,
                then: rewrite(f, then, bool_ty, plans),
                else_: rewrite(f, else_, bool_ty, plans),
            },
            Stmt::Loop { body, continuing } => {
                let g = body.iter().find_map(exit).and_then(|v| plans.get(&v));
                let mut out = Vec::new();
                if let Some(g) = g {
                    let value = f.let_(&mut out, g.ty, Expr::Load(Place::local(g.local)));
                    let bound = f.let_(&mut out, g.ty, Expr::Const(g.constant.clone()));
                    let open = f.let_(&mut out, bool_ty, Expr::Binary(g.op, value, bound));
                    let closed = f.let_(&mut out, bool_ty, Expr::Unary(UnOp::Not, open));
                    out.push(Stmt::If { cond: closed, then: vec![Stmt::Break], else_: vec![] });
                }
                out.extend(rewrite(f, body, bool_ty, plans));
                Stmt::Loop { body: out, continuing: rewrite(f, continuing, bool_ty, plans) }
            }
            s => s,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Types, Function, LocalId, LocalId) {
        let mut t = Types::default();
        let (float, uint, boolean) = (t.f32(), t.u32(), t.bool());
        let params = [("start", float), ("dense", boolean), ("bound", uint)]
            .into_iter()
            .map(|(name, ty)| Param { name: name.into(), ty, by_ref: false, mutable: false })
            .collect();
        let mut f = Function::new("closed_guard", params, Some(float));
        let counter = f.new_local("counter", uint);
        let state = f.new_local("state", float);
        let sum = f.new_local("sum", float);
        let logic = f.new_local("logic", boolean);
        let mut outer = Vec::new();
        let start = f.let_(&mut outer, float, Expr::Param(0));
        let dense = f.let_(&mut outer, boolean, Expr::Param(1));
        let bound = f.let_(&mut outer, uint, Expr::Param(2));
        let zero = f.let_(&mut outer, uint, Expr::Const(Const::U32(0)));
        outer.push(Stmt::Store(Place::local(counter), zero));
        outer.push(Stmt::Store(Place::local(state), start));
        let zero = f.let_(&mut outer, float, Expr::Const(Const::F32(0.0)));
        outer.push(Stmt::Store(Place::local(sum), zero));
        let mut body = Vec::new();
        let i = f.let_(&mut body, uint, Expr::Load(Place::local(counter)));
        let end = f.let_(&mut body, boolean, Expr::Binary(BinOp::Ge, i, bound));
        body.push(Stmt::If { cond: end, then: vec![Stmt::Break], else_: vec![] });
        body.push(Stmt::Store(Place::local(logic), dense));
        let old = f.let_(&mut body, float, Expr::Load(Place::local(state)));
        let cutoff = f.let_(&mut body, float, Expr::Const(Const::F32(0.01)));
        let open = f.let_(&mut body, boolean, Expr::Binary(BinOp::Ge, old, cutoff));
        let alias = f.let_(&mut body, boolean, Expr::Load(Place::local(logic)));
        let both = f.let_(
            &mut body,
            boolean,
            Expr::Select { cond: dense, if_true: open, if_false: alias },
        );
        body.push(Stmt::Store(Place::local(logic), both));
        let both = f.let_(&mut body, boolean, Expr::Load(Place::local(logic)));
        let mut then = Vec::new();
        let prev = f.let_(&mut then, float, Expr::Load(Place::local(sum)));
        let next = f.let_(&mut then, float, Expr::Binary(BinOp::Add, prev, old));
        then.push(Stmt::Store(Place::local(sum), next));
        let half = f.let_(&mut then, float, Expr::Const(Const::F32(0.5)));
        let next = f.let_(&mut then, float, Expr::Binary(BinOp::Mul, old, half));
        then.push(Stmt::Store(Place::local(state), next));
        body.push(Stmt::If { cond: both, then, else_: vec![] });
        let mut continuing = Vec::new();
        let i = f.let_(&mut continuing, uint, Expr::Load(Place::local(counter)));
        let one = f.let_(&mut continuing, uint, Expr::Const(Const::U32(1)));
        let next = f.let_(&mut continuing, uint, Expr::Binary(BinOp::Add, i, one));
        continuing.push(Stmt::Store(Place::local(counter), next));
        outer.push(Stmt::Loop { body, continuing });
        let state_value = f.let_(&mut outer, float, Expr::Load(Place::local(state)));
        let sum_value = f.let_(&mut outer, float, Expr::Load(Place::local(sum)));
        let result = f.let_(&mut outer, float, Expr::Binary(BinOp::Add, state_value, sum_value));
        outer.push(Stmt::Return(Some(result)));
        f.body = outer;
        (t, f, counter, state)
    }

    fn loop_parts(f: &mut Function) -> (&mut Block, &mut Block) {
        f.body
            .iter_mut()
            .find_map(|s| match s {
                Stmt::Loop { body, continuing } => Some((body, continuing)),
                _ => None,
            })
            .unwrap()
    }

    fn changed(f: &mut Function, t: &Types) -> bool {
        let before = f.clone();
        eliminate(f, t);
        *f != before
    }

    // A bounded interpreter for this scalar fixture checks outputs and actual iterations;
    // the pass's proof and rewrites are not reused by this evaluator.
    fn run(f: &Function, start: f32, dense: bool, bound: u32) -> (u32, u32) {
        struct Machine {
            values: HashMap<ValueId, Const>,
            locals: HashMap<LocalId, Const>,
            params: [Const; 3],
            iterations: u32,
            result: Option<f32>,
        }
        impl Machine {
            fn block(&mut self, b: &Block) -> bool {
                for s in b {
                    match s {
                        Stmt::Let(v, e) => {
                            let x = match e {
                                Expr::Const(c) => c.clone(),
                                Expr::Param(i) => self.params[*i as usize].clone(),
                                Expr::Load(p) => self.locals[&whole(p).unwrap()].clone(),
                                Expr::Unary(UnOp::Not, a) => {
                                    let Const::Bool(a) = self.values[a] else { panic!() };
                                    Const::Bool(!a)
                                }
                                Expr::Binary(op, a, b) => {
                                    match (*op, &self.values[a], &self.values[b]) {
                                        (BinOp::Add, Const::U32(a), Const::U32(b)) => {
                                            Const::U32(a + b)
                                        }
                                        (BinOp::Ge, Const::U32(a), Const::U32(b)) => {
                                            Const::Bool(a >= b)
                                        }
                                        (BinOp::Ge, Const::F32(a), Const::F32(b)) => {
                                            Const::Bool(a >= b)
                                        }
                                        (BinOp::Add, Const::F32(a), Const::F32(b)) => {
                                            Const::F32(a + b)
                                        }
                                        (BinOp::Mul, Const::F32(a), Const::F32(b)) => {
                                            Const::F32(a * b)
                                        }
                                        _ => panic!(),
                                    }
                                }
                                Expr::Select { cond, if_true, if_false } => {
                                    let Const::Bool(c) = self.values[cond] else { panic!() };
                                    self.values[if c { if_true } else { if_false }].clone()
                                }
                                _ => panic!(),
                            };
                            self.values.insert(*v, x);
                        }
                        Stmt::Store(p, v) => {
                            self.locals.insert(whole(p).unwrap(), self.values[v].clone());
                        }
                        Stmt::If { cond, then, else_ } => {
                            let Const::Bool(c) = self.values[cond] else { panic!() };
                            if self.block(if c { then } else { else_ }) {
                                return true;
                            }
                        }
                        Stmt::Loop { body, continuing } => loop {
                            assert!(self.iterations < 128);
                            if self.block(body) {
                                break;
                            }
                            self.iterations += 1;
                            assert!(!self.block(continuing));
                        },
                        Stmt::Break => return true,
                        Stmt::Return(Some(v)) => {
                            let Const::F32(x) = self.values[v] else { panic!() };
                            self.result = Some(x);
                            return true;
                        }
                        _ => panic!(),
                    }
                }
                false
            }
        }
        let mut m = Machine {
            values: HashMap::new(),
            locals: HashMap::new(),
            params: [Const::F32(start), Const::Bool(dense), Const::U32(bound)],
            iterations: 0,
            result: None,
        };
        m.block(&f.body);
        (m.result.unwrap().to_bits(), m.iterations)
    }

    #[test]
    fn closed_guard_preserves_results_and_removes_tail() {
        let (t, mut f, _, _) = fixture();
        let before = f.clone();
        assert!(changed(&mut f, &t));
        for start in [-1.0, 0.0, 0.009, 0.01, 0.02, 1.0, f32::NAN] {
            for dense in [false, true] {
                for bound in [0, 1, 32] {
                    let expected = run(&before, start, dense, bound);
                    let actual = run(&f, start, dense, bound);
                    assert_eq!(actual.0, expected.0);
                    assert!(actual.1 <= expected.1);
                }
            }
        }
        assert_eq!(run(&before, 1.0, true, 32).1, 32);
        assert_eq!(run(&f, 1.0, true, 32).1, 7);
        assert_eq!(run(&f, 0.009, true, 32).1, 0);
    }

    #[test]
    fn observable_counter_and_gate_writes_reject() {
        let (t, mut f, counter, _) = fixture();
        let v = f.new_value(t.lookup(&TypeDef::Scalar(Scalar::U32)).unwrap());
        f.body.push(Stmt::Let(v, Expr::Load(Place::local(counter))));
        assert!(!changed(&mut f, &t));
        let (t, mut f, _, state) = fixture();
        let start = ValueId(0);
        loop_parts(&mut f).0.insert(3, Stmt::Store(Place::local(state), start));
        assert!(!changed(&mut f, &t));
    }

    #[test]
    fn stale_counter_load_or_nonunit_step_reject() {
        let (t, mut f, _, _) = fixture();
        let load = loop_parts(&mut f).1.remove(0);
        f.body.insert(0, load);
        assert!(!changed(&mut f, &t));
        let (t, mut f, _, _) = fixture();
        for s in loop_parts(&mut f).1 {
            if let Stmt::Let(_, Expr::Const(c)) = s {
                *c = Const::U32(2);
            }
        }
        assert!(!changed(&mut f, &t));
        let (t, mut f, _, _) = fixture();
        let load = loop_parts(&mut f).0.remove(0);
        f.body.insert(0, load);
        assert!(!changed(&mut f, &t));
    }

    #[test]
    fn captured_gate_load_and_reopening_scratch_reject() {
        let (t, mut f, _, state) = fixture();
        let (body, _) = loop_parts(&mut f);
        let at = body
            .iter()
            .position(|s| matches!(s, Stmt::Let(_, Expr::Load(p)) if whole(p) == Some(state)))
            .unwrap();
        let load = body.remove(at);
        let at = f.body.iter().position(|s| matches!(s, Stmt::Loop { .. })).unwrap();
        f.body.insert(at, load);
        assert!(!changed(&mut f, &t));

        let (t, mut f, _, state) = fixture();
        let boolean = t.lookup(&TypeDef::Scalar(Scalar::Bool)).unwrap();
        let scratch = f.new_local("scratch", boolean);
        let old = f.new_value(boolean);
        let (body, _) = loop_parts(&mut f);
        body.splice(
            3..3,
            [
                Stmt::Let(old, Expr::Load(Place::local(scratch))),
                Stmt::If {
                    cond: old,
                    then: vec![Stmt::Store(Place::local(state), ValueId(0))],
                    else_: vec![],
                },
                Stmt::Store(Place::local(scratch), ValueId(1)),
            ],
        );
        assert!(!changed(&mut f, &t), "a scratch value may reopen the guard on a later iteration");
    }

    #[test]
    fn implicit_lod_derivatives_calls_and_effects_reject() {
        for expr in [
            Expr::Barrier,
            Expr::Call(FuncId(0), vec![]),
            Expr::Discard,
            Expr::Texture(TextureOp::Sample, ResourceId(0), None, vec![]),
            Expr::Builtin(Builtin::Dpdx, vec![]),
            Expr::TextureStore(ResourceId(0), vec![]),
        ] {
            let (t, mut f, _, _) = fixture();
            loop_parts(&mut f).0.insert(3, Stmt::Eval(expr));
            assert!(!changed(&mut f, &t));
        }
        let (t, mut f, _, _) = fixture();
        loop_parts(&mut f).0.insert(
            3,
            Stmt::Eval(Expr::Texture(TextureOp::SampleLevel, ResourceId(0), None, vec![])),
        );
        assert!(changed(&mut f, &t));
    }
}
