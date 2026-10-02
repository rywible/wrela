// Literal lifting for WGSL field files: find every numeric literal and named const, record source
// spans, rewrite the shader so literals read a parameter buffer, and splice edited values back.
// This is what D-105 asks the compiler to provide (provenance, stable literal identities). Here it's
// a tokenizer and a shallow parser, with no type checking; the README lists what that can't do.

const KEYWORDS = new Set(['return', 'let', 'var', 'const', 'if', 'else', 'for', 'while', 'loop', 'switch',
  'case', 'default', 'break', 'continue', 'continuing', 'discard', 'fn', 'struct', 'override', 'alias',
  'true', 'false', 'enable', 'requires', 'diagnostic', 'const_assert']);
const OPS = ['<<=', '>>=', '->', '<=', '>=', '==', '!=', '&&', '||', '<<', '>>', '++', '--', '+=', '-=', '*=',
  '/=', '%=', '&=', '|=', '^='];
const RE_NUM = /(?:0[xX](?:[0-9a-fA-F]+\.?[0-9a-fA-F]*|\.[0-9a-fA-F]+)(?:[pP][+-]?\d+)?[fhiu]?)|(?:(?:\d+\.\d*|\.\d+)(?:[eE][+-]?\d+)?[fh]?)|(?:\d+[eE][+-]?\d+[fh]?)|(?:\d+[fhiu]?)/y;
const RE_ID = /[A-Za-z_][A-Za-z0-9_]*/y;

export function tokenize(src) {
  const toks = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') {
      let j = i;
      while (j < n && /\s/.test(src[j])) j++;
      toks.push({ t: 'ws', s: i, e: j });
      i = j;
    } else if (c === '/' && src[i + 1] === '/') {
      let j = src.indexOf('\n', i);
      if (j < 0) j = n;
      toks.push({ t: 'comment', s: i, e: j });
      i = j;
    } else if (c === '/' && src[i + 1] === '*') {
      let depth = 1, j = i + 2;
      while (j < n && depth > 0) {
        if (src[j] === '/' && src[j + 1] === '*') { depth++; j += 2; } else if (src[j] === '*' && src[j + 1] === '/') { depth--; j += 2; } else j++;
      }
      toks.push({ t: 'comment', s: i, e: j });
      i = j;
    } else if ((c >= '0' && c <= '9') || (c === '.' && src[i + 1] >= '0' && src[i + 1] <= '9')) {
      RE_NUM.lastIndex = i;
      const m = RE_NUM.exec(src);
      toks.push({ t: 'num', s: i, e: i + m[0].length });
      i += m[0].length;
    } else if (/[A-Za-z_]/.test(c)) {
      RE_ID.lastIndex = i;
      const m = RE_ID.exec(src);
      toks.push({ t: 'id', s: i, e: i + m[0].length });
      i += m[0].length;
    } else {
      const op = OPS.find(o => src.startsWith(o, i)) || c;
      toks.push({ t: 'punct', s: i, e: i + op.length });
      i += op.length;
    }
  }
  for (const t of toks) t.text = src.slice(t.s, t.e);
  return toks;
}

function lineIndex(src) {
  const starts = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === '\n') starts.push(i + 1);
  return off => {
    let lo = 0, hi = starts.length - 1;
    while (lo < hi) { const mid = (lo + hi + 1) >> 1; if (starts[mid] <= off) lo = mid; else hi = mid - 1; }
    return { line: lo + 1, col: off - starts[lo] + 1 };
  };
}

function classifyNumber(text) {
  if (/^0[xX]/.test(text)) {
    if (/[.pP]/.test(text)) return { kind: 'float', hex: true };
    return { kind: 'int', value: parseInt(text.replace(/[iu]$/, ''), 16) };
  }
  if (/[.eE]/.test(text) || /[fh]$/.test(text)) return { kind: 'float', value: parseFloat(text.replace(/[fh]$/, '')) };
  return { kind: 'int', value: parseInt(text.replace(/[iu]$/, ''), 10) };
}

/** Parse the module shallowly: consts, functions, other declarations. Indices are into `sig`. */
function parseModule(sig) {
  const N = sig.length;
  const consts = [], fns = [], others = [];
  const closeOf = { '(': ')', '[': ']', '{': '}' };
  const matchClose = k => {
    const open = sig[k].text, close = closeOf[open];
    let d = 0;
    for (let j = k; j < N; j++) {
      if (sig[j].text === open) d++;
      else if (sig[j].text === close && --d === 0) return j;
    }
    throw new Error(`unbalanced '${open}' at offset ${sig[k].s}`);
  };
  const skipToSemi = k => {
    let d = 0;
    for (let j = k; j < N; j++) {
      const x = sig[j].text;
      if (x === '(' || x === '[' || x === '{') d++;
      else if (x === ')' || x === ']' || x === '}') d--;
      else if (x === ';' && d === 0) return j;
    }
    return N - 1;
  };
  let k = 0;
  while (k < N) {
    const t = sig[k];
    if (t.text === '@') {
      k += 2;
      if (sig[k] && sig[k].text === '(') k = matchClose(k) + 1;
    } else if (t.text === 'const') {
      let j = k + 2;
      while (j < N && sig[j].text !== '=') j++;
      const end = skipToSemi(j);
      consts.push({ name: sig[k + 1].text, nameTok: k + 1, start: k, initStart: j + 1, initEnd: end });
      k = end + 1;
    } else if (t.text === 'fn') {
      const po = k + 2, pc = matchClose(po);
      const params = [];
      for (let j = po + 1; j < pc; j++) {
        if (sig[j].t === 'id' && sig[j + 1].text === ':' && ['(', ',', ')'].includes(sig[j - 1].text)) params.push(sig[j].text);
      }
      let j = pc + 1, ret = '';
      if (sig[j].text === '->') {
        j++;
        const parts = [];
        while (sig[j].text !== '{') {
          if (sig[j].text === '@') { j += 2; if (sig[j].text === '(') j = matchClose(j) + 1; continue; }
          parts.push(sig[j].text);
          j++;
        }
        ret = parts.join('');
      } else {
        while (sig[j].text !== '{') j++;
      }
      const bc = matchClose(j);
      fns.push({ name: sig[k + 1].text, params, ret, start: k, paramsStart: po, paramsEnd: pc, bodyStart: j, bodyEnd: bc });
      k = bc + 1;
    } else if (t.text === 'struct') {
      let j = k;
      while (sig[j].text !== '{') j++;
      j = matchClose(j);
      others.push({ kind: 'struct', start: k, end: j });
      k = j + 1;
      if (sig[k] && sig[k].text === ';') k++;
    } else {
      const end = skipToSemi(k);
      others.push({ kind: t.text, start: k, end });
      k = end + 1;
    }
  }
  return { consts, fns, others, matchClose };
}

function applyEdits(src, s0, e0, edits) {
  const es = edits.filter(x => x.s >= s0 && x.e <= e0)
    .sort((a, b) => a.s - b.s || (a.e - a.s) - (b.e - b.s) || (a.order || 0) - (b.order || 0));
  let out = '', pos = s0;
  for (const x of es) {
    if (x.s < pos) throw new Error(`overlapping edits at offset ${x.s}`);
    out += src.slice(pos, x.s) + x.text;
    pos = x.e;
  }
  return out + src.slice(pos, e0);
}

/**
 * Lift a WGSL field file. Returns the literal table, the unlifted literals with reasons, the parts
 * (calls in the entry function's body), the rewritten shader text, and static provenance (which
 * literals can reach the entry function).
 */
export function lift(src, { entry = 'field', name = 'subject' } = {}) {
  const t0 = performance.now();
  const toks = tokenize(src);
  const sig = toks.filter(t => t.t !== 'ws' && t.t !== 'comment');
  sig.forEach((t, k) => { t.k = k; });
  const comments = toks.filter(t => t.t === 'comment');
  const pos = lineIndex(src);
  const lines = src.split('\n');
  const mod = parseModule(sig);
  const fnByName = new Map(mod.fns.map(f => [f.name, f]));
  const constByName = new Map(mod.consts.map(c => [c.name, c]));
  const lineComment = line => {
    const text = lines[line - 1] || '';
    const c = comments.find(x => pos(x.s).line === line && x.text.startsWith('//'));
    return c ? c.text.replace(/^\/\/\s*/, '').trim() : (text.includes('//') ? '' : '');
  };

  // Where is token k? (const initializer, function body, or elsewhere)
  const owner = k => {
    for (const c of mod.consts) if (k >= c.initStart && k < c.initEnd) return { kind: 'const', c };
    for (const f of mod.fns) {
      if (k > f.bodyStart && k < f.bodyEnd) return { kind: 'fn', f };
      if (k >= f.start && k <= f.bodyStart) return { kind: 'signature', f };
    }
    for (const o of mod.others) if (k >= o.start && k <= o.end) return { kind: 'other', o };
    return { kind: 'none' };
  };

  // Function-scope declarations, for shadowing and const->let rewrites.
  const fnLocals = new Map();
  const fnConstStmts = [];
  for (const f of mod.fns) {
    const names = new Set(f.params);
    for (let k = f.bodyStart + 1; k < f.bodyEnd; k++) {
      const x = sig[k].text;
      if ((x === 'let' || x === 'var' || x === 'const') && sig[k + 1]) {
        let j = k + 1;
        if (x === 'var' && sig[j].text === '<') { while (sig[j].text !== '>') j++; j++; }
        names.add(sig[j].text);
        if (x === 'const') {
          let e = j;
          while (sig[e].text !== ';') e++;
          fnConstStmts.push({ fn: f.name, kw: k, start: j, end: e });
        }
      }
    }
    fnLocals.set(f.name, names);
  }

  // ---- literals ----
  const literals = [], unlifted = [];
  const constLits = new Map();   // const name -> [literal ids]
  for (const t of sig) {
    if (t.t !== 'num') continue;
    const cls = classifyNumber(t.text);
    const where = owner(t.k);
    let s = t.s, text = t.text, neg = false;
    const prev = sig[t.k - 1], prev2 = sig[t.k - 2];
    if (prev && prev.text === '-' && !(prev2 && ((prev2.t === 'id' && !KEYWORDS.has(prev2.text)) || prev2.t === 'num' || prev2.text === ')' || prev2.text === ']'))) {
      s = prev.s; neg = true; text = src.slice(s, t.e);
    }
    const lc = pos(s);
    const base = { s, e: t.e, text, line: lc.line, col: lc.col };
    let reason = null;
    if (cls.kind === 'int') reason = 'integer literal: may be used where only an integer is allowed (octave count, index); telling uses apart needs type checking';
    else if (cls.hex) reason = 'hexadecimal float: not handled by this spike';
    else if (where.kind === 'signature') reason = `in the signature of fn ${where.f.name} (attribute or type): needs a constant expression`;
    else if (where.kind === 'other') reason = `in a module-scope '${where.o.kind}' declaration: needs a constant expression`;
    else if (where.kind === 'none') reason = 'outside any declaration';
    if (reason) { unlifted.push({ ...base, value: neg ? -cls.value : cls.value, reason }); continue; }
    const value = neg ? -cls.value : cls.value;
    const L = { id: literals.length, ...base, value, kind: where.kind, fn: null, constName: null };
    if (where.kind === 'const') {
      L.constName = where.c.name;
      if (!constLits.has(L.constName)) constLits.set(L.constName, []);
      constLits.get(L.constName).push(L.id);
    } else {
      L.fn = where.f.name;
      const fc = fnConstStmts.find(c => c.fn === L.fn && t.k > c.start && t.k < c.end);
      if (fc) fc.lifted = true;
    }
    literals.push(L);
  }

  // Names: const components get .x/.y/.z/.w when the initializer is a plain vector of literals.
  for (const c of mod.consts) {
    const ids = constLits.get(c.name) || [];
    const init = sig.slice(c.initStart, c.initEnd).map(x => x.text);
    const vecCtor = /^vec[234]([fh]|<f32>|<f16>)?$/.test(init[0] || '') || /^vec[234]$/.test(init[0] || '');
    const comps = ['x', 'y', 'z', 'w'];
    ids.forEach((id, i) => {
      literals[id].name = ids.length === 1 && !vecCtor ? c.name : (vecCtor && ids.length <= 4 ? `${c.name}.${comps[i]}` : `${c.name}[${i}]`);
    });
  }
  for (const L of literals) {
    if (!L.name) L.name = `${L.fn}:${L.line}:${L.col}`;
    L.comment = lineComment(L.line);
    L.source = (lines[L.line - 1] || '').trim();
  }

  // ---- consts: which carry literals (directly or through other consts) ----
  const constDeps = new Map();
  for (const c of mod.consts) {
    const deps = new Set();
    for (let k = c.initStart; k < c.initEnd; k++) {
      if (sig[k].t === 'id' && constByName.has(sig[k].text) && sig[k - 1].text !== '.') deps.add(sig[k].text);
    }
    constDeps.set(c.name, deps);
  }
  const liftedConst = new Set([...constLits.keys()]);
  for (let changed = true; changed;) {
    changed = false;
    for (const c of mod.consts) {
      if (liftedConst.has(c.name)) continue;
      if ([...constDeps.get(c.name)].some(d => liftedConst.has(d))) { liftedConst.add(c.name); changed = true; }
    }
  }

  // ---- function analysis: calls, const uses, shadowing ----
  const notes = [];
  const fnCalls = new Map(), fnConsts = new Map();
  const constUseEdits = [];   // { s, e, constName }
  for (const f of mod.fns) {
    const calls = new Set(), cuses = new Set();
    const locals = fnLocals.get(f.name);
    for (let k = f.bodyStart + 1; k < f.bodyEnd; k++) {
      const t = sig[k];
      if (t.t !== 'id' || sig[k - 1].text === '.') continue;
      if (fnByName.has(t.text) && sig[k + 1].text === '(') calls.add(t.text);
      if (liftedConst.has(t.text)) {
        if (locals.has(t.text)) {
          notes.push(`const ${t.text} is shadowed in fn ${f.name}; its uses there are not expanded`);
          continue;
        }
        cuses.add(t.text);
        constUseEdits.push({ s: t.s, e: t.e, constName: t.text, fn: f.name });
      }
    }
    fnCalls.set(f.name, calls);
    fnConsts.set(f.name, cuses);
  }
  // Lifted consts used where only a constant expression is allowed (other module declarations,
  // signatures): those uses keep the original value.
  for (const o of mod.others) {
    for (let k = o.start; k <= o.end; k++) {
      if (sig[k].t === 'id' && liftedConst.has(sig[k].text)) notes.push(`const ${sig[k].text} is used in a module-scope '${o.kind}' declaration (line ${pos(sig[k].s).line}); that use keeps the original value`);
    }
  }

  // ---- static provenance: what reaches the entry function ----
  const entryFn = fnByName.get(entry);
  if (!entryFn) throw new Error(`no fn ${entry} in ${name}`);
  const reachFns = new Set([entry]);
  const queue = [entry];
  while (queue.length) {
    const f = queue.pop();
    for (const g of fnCalls.get(f) || []) if (!reachFns.has(g)) { reachFns.add(g); queue.push(g); }
  }
  const reachConsts = new Set();
  const addConst = c => { if (reachConsts.has(c)) return; reachConsts.add(c); for (const d of constDeps.get(c) || []) addConst(d); };
  for (const f of reachFns) for (const c of fnConsts.get(f) || []) addConst(c);
  for (const L of literals) L.inField = L.kind === 'const' ? reachConsts.has(L.constName) : reachFns.has(L.fn);
  // Which consts each literal's const is used by (for display): functions using it.
  for (const L of literals) {
    if (L.kind !== 'const') continue;
    const users = [];
    for (const [fname, cs] of fnConsts) {
      for (const c of cs) {
        if (c === L.constName || dependsOn(c, L.constName, constDeps)) { users.push(fname); break; }
      }
    }
    L.usedBy = users;
  }

  // Static evaluation sites: how many call paths from the entry reach each function, and how many
  // uses each const has along them. A literal with many sites is shared (a helper's constant).
  const callMult = new Map();   // caller -> Map(callee -> count)
  for (const f of mod.fns) {
    const m = new Map();
    for (let k = f.bodyStart + 1; k < f.bodyEnd; k++) {
      const t = sig[k];
      if (t.t === 'id' && sig[k - 1].text !== '.' && fnByName.has(t.text) && sig[k + 1].text === '(') m.set(t.text, (m.get(t.text) || 0) + 1);
    }
    callMult.set(f.name, m);
  }
  const paths = new Map();
  const pathsTo = (f, stack = new Set()) => {
    if (paths.has(f)) return paths.get(f);
    if (f === entry) return 1;
    if (stack.has(f)) return 0;
    stack.add(f);
    let n = 0;
    for (const [caller, m] of callMult) if (m.has(f) && reachFns.has(caller)) n += m.get(f) * pathsTo(caller, stack);
    stack.delete(f);
    paths.set(f, n);
    return n;
  };
  const constSites = new Map();
  const sitesOfConst = c => {
    if (constSites.has(c)) return constSites.get(c);
    let n = 0;
    for (const u of constUseEdits) if (u.constName === c) n += pathsTo(u.fn);
    for (const [other, deps] of constDeps) if (deps.has(c) && other !== c) n += sitesOfConst(other);
    constSites.set(c, n);
    return n;
  };
  for (const L of literals) L.sites = L.kind === 'const' ? sitesOfConst(L.constName) : pathsTo(L.fn);
  // Helpers: functions called from three or more distinct functions (sd_ell, say). Their literals are
  // shared code; automatic literal selection leaves them out unless asked.
  const callers = new Map();
  for (const [caller, m] of callMult) for (const callee of m.keys()) { if (!callers.has(callee)) callers.set(callee, new Set()); callers.get(callee).add(caller); }
  for (const L of literals) L.helper = L.kind === 'fn' && (callers.get(L.fn) || new Set()).size >= 3;

  // ---- parts: calls in the entry body to subject functions returning f32 ----
  const parts = [];
  const tapEdits = [];
  for (let k = entryFn.bodyStart + 1; k < entryFn.bodyEnd; k++) {
    const t = sig[k];
    if (t.t !== 'id' || sig[k - 1].text === '.' || sig[k + 1].text !== '(') continue;
    const callee = fnByName.get(t.text);
    if (!callee || callee.ret !== 'f32') continue;
    const close = mod.matchClose(k + 1);
    const idx = parts.length;
    const dup = parts.filter(p => p.callee === t.text).length;
    parts.push({ index: idx, callee: t.text, name: dup ? `${t.text}#${dup + 1}` : t.text, line: pos(t.s).line, s: t.s, e: sig[close].e,
      fnLine: pos(sig[callee.start].s).line, fnEndLine: pos(sig[callee.bodyEnd].s).line });
    tapEdits.push({ s: t.s, e: t.s, text: `lens_tap(${idx}, `, order: 0 });
    tapEdits.push({ s: sig[close].e, e: sig[close].e, text: ')', order: 1 });
  }
  // Functions reachable from each part (for "does this literal belong to the clicked part?").
  for (const p of parts) {
    const fs = new Set([p.callee]);
    const q = [p.callee];
    while (q.length) { const f = q.pop(); for (const g of fnCalls.get(f) || []) if (!fs.has(g)) { fs.add(g); q.push(g); } }
    const cs = new Set();
    const add = c => { if (cs.has(c)) return; cs.add(c); for (const d of constDeps.get(c) || []) add(d); };
    for (const f of fs) for (const c of fnConsts.get(f) || []) add(c);
    // Arguments of the call itself (e.g. sd_paw(m - J_FPAW, ...)) are part of the part too.
    for (const e of constUseEdits) if (e.s >= p.s && e.e <= p.e) add(e.constName);
    p.fns = [...fs];
    p.consts = [...cs];
  }

  // ---- rewrite ----
  const litEdits = literals.map(L => ({ s: L.s, e: L.e, text: `lens_lit(${L.id})` }));
  const expansion = new Map();
  const expand = cname => {
    if (expansion.has(cname)) return expansion.get(cname);
    const c = constByName.get(cname);
    const s0 = sig[c.initStart].s, e0 = sig[c.initEnd - 1].e;
    const inner = [];
    for (const x of litEdits) if (x.s >= s0 && x.e <= e0) inner.push(x);
    for (let k = c.initStart; k < c.initEnd; k++) {
      if (sig[k].t === 'id' && liftedConst.has(sig[k].text) && sig[k - 1].text !== '.') inner.push({ s: sig[k].s, e: sig[k].e, text: expand(sig[k].text) });
    }
    const text = `(${applyEdits(src, s0, e0, inner)})`;
    expansion.set(cname, text);
    return text;
  };
  const edits = [];
  literals.forEach((L, i) => { if (L.kind === 'fn') edits.push(litEdits[i]); });
  for (const u of constUseEdits) edits.push({ s: u.s, e: u.e, text: expand(u.constName) });
  for (const fc of fnConstStmts) {
    if (fc.lifted) {
      edits.push({ s: sig[fc.kw].s, e: sig[fc.kw].e, text: 'let' });
      notes.push(`fn ${fc.fn}: function-scope const '${sig[fc.start].text}' rewritten to let`);
    }
  }
  edits.push(...tapEdits);
  const shader = applyEdits(src, 0, src.length, edits);

  const ms = performance.now() - t0;
  return {
    name, entry, src, shader, literals, unlifted, parts, notes: [...new Set(notes)], ms,
    fns: mod.fns.map(f => ({ name: f.name, ret: f.ret, line: pos(sig[f.start].s).line, endLine: pos(sig[f.bodyEnd].s).line, reachesEntry: reachFns.has(f.name) })),
    consts: mod.consts.map(c => ({ name: c.name, line: pos(sig[c.start].s).line, lifted: liftedConst.has(c.name), reachesEntry: reachConsts.has(c.name), literals: constLits.get(c.name) || [] })),
    structure: structureOf(sig, literals, unlifted),
    lines,
  };
}

// The token sequence with literals (and their unary minus) removed: equal before and after a
// write-back exactly when nothing but literal values changed.
function structureOf(sig, literals, unlifted) {
  const spans = [...literals, ...unlifted].map(l => [l.s, l.e]).sort((a, b) => a[0] - b[0]);
  const out = [];
  let j = 0;
  for (const t of sig) {
    while (j < spans.length && spans[j][1] <= t.s) j++;
    if (j < spans.length && t.s >= spans[j][0] && t.e <= spans[j][1]) continue;
    out.push(t.text);
  }
  return out.join(' ');
}

function dependsOn(c, target, deps, seen = new Set()) {
  if (seen.has(c)) return false;
  seen.add(c);
  for (const d of deps.get(c) || []) if (d === target || dependsOn(d, target, deps, seen)) return true;
  return false;
}

/** The WGSL that the rewritten shader needs: the parameter buffer, lens_lit and the part taps. */
export function preamble(L) {
  const nv = Math.max(1, Math.ceil(L.literals.length / 4));
  const np = Math.max(1, L.parts.length);
  return `
// ---- generated by lift.js: the lifted literals and the part taps ----
const LENS_NPARTS: i32 = ${L.parts.length};
struct LensLits { v: array<vec4f, ${nv}> }
@group(0) @binding(1) var<uniform> lens_lits: LensLits;
@group(0) @binding(6) var<storage, read> lens_litbuf: array<f32>;
var<private> lens_di: i32 = -1;     // literal perturbed by this invocation (finite differences), or -1
var<private> lens_dh: f32 = 0.0;    // its offset
var<private> lens_tap_v: array<f32, ${np}>;
fn lens_lit(i: i32) -> f32 { return lens_litbuf[i] + select(0.0, lens_dh, i == lens_di); }  // reference form; lens.js inlines reads
fn lens_tap(k: i32, v: f32) -> f32 { lens_tap_v[k] = v; return v; }
`;
}

// ---- write-back ----

function round(v, d) { const f = 10 ** d; return Math.round(v * f) / f; }

/** Format a new value in the original literal's style: decimals, exponent, suffix. */
export function formatLiteral(L, v, policy = 'adaptive') {
  const m = /^(-?)(\s*)(0[xX].*|[0-9.]+(?:[eE][+-]?\d+)?)([fh]?)$/.exec(L.text);
  const body = m ? m[3] : L.text, suffix = m ? m[4] : '';
  const change = Math.abs(v - L.value);
  if (/[eE]/.test(body)) {
    const mant = body.split(/[eE]/)[0];
    let p = Math.max(0, (mant.split('.')[1] || '').length);
    if (policy === 'adaptive') while (p < 8 && Math.abs(Number(v.toExponential(p)) - v) > 0.05 * change + 1e-12) p++;
    return v.toExponential(p).replace('e+', 'e') + suffix;
  }
  const dec0 = body.includes('.') ? body.split('.')[1].length : 0;
  let d = Math.max(dec0, 1);
  if (policy === 'adaptive') while (d < 7 && Math.abs(round(v, d) - v) > 0.05 * change + 1e-9) d++;
  let s = v.toFixed(d);
  if (/^-0\.0*$/.test(s)) s = s.slice(1);
  if (body.startsWith('.') && /^-?0\./.test(s)) s = s.replace('0.', '.');
  if (dec0 === 0 && body.endsWith('.') && d === 1 && s.endsWith('.0')) s = s.slice(0, -1);
  return s + suffix;
}

/** Splice new values into the source. Only literals whose formatted text changes are touched. */
export function writeBack(L, values, { policy = 'adaptive', only = null } = {}) {
  const t0 = performance.now();
  const edits = [], changes = [];
  for (const lit of L.literals) {
    if (only && !only.has(lit.id)) continue;
    const v = values[lit.id];
    if (v === lit.value) continue;
    let text = formatLiteral(lit, v, policy);
    if (text === lit.text) continue;
    if (text.startsWith('-') && L.src[lit.s - 1] === '-') text = ' ' + text;
    edits.push({ s: lit.s, e: lit.e, text });
    changes.push({ id: lit.id, name: lit.name, line: lit.line, from: lit.text, to: text.trim(), value: Number(text.trim().replace(/[fh]$/, '')) });
  }
  const source = applyEdits(L.src, 0, L.src.length, edits);
  return { source, changes, ms: performance.now() - t0 };
}

/** Line diff for same-line-count edits (write-back never adds lines), plus a generic fallback. */
export function diffLines(a, b, ctx = 0) {
  const A = a.split('\n'), B = b.split('\n');
  const out = [];
  let changedLines = 0, changedChars = 0;
  if (A.length === B.length) {
    for (let i = 0; i < A.length; i++) {
      if (A[i] === B[i]) continue;
      changedLines++;
      let p = 0;
      while (p < A[i].length && A[i][p] === B[i][p]) p++;
      let qa = A[i].length, qb = B[i].length;
      while (qa > p && qb > p && A[i][qa - 1] === B[i][qb - 1]) { qa--; qb--; }
      changedChars += Math.max(qa - p, qb - p);
      out.push(`@@ -${i + 1} +${i + 1} @@`);
      for (let c = Math.max(0, i - ctx); c < i; c++) out.push(' ' + A[c]);
      out.push('-' + A[i], '+' + B[i]);
    }
  } else {
    out.push(`(line count changed: ${A.length} -> ${B.length})`);
    changedLines = Math.abs(A.length - B.length);
  }
  return { text: out.join('\n'), changedLines, changedChars, sameLineCount: A.length === B.length };
}

/** Re-lift the new source and check that only the intended literals changed, to the intended values. */
export function verifyWriteBack(L, wb, values) {
  const t0 = performance.now();
  const L2 = lift(wb.source, { entry: L.entry, name: L.name });
  const problems = [];
  if (L2.literals.length !== L.literals.length) problems.push(`literal count ${L.literals.length} -> ${L2.literals.length}`);
  if (L2.structure !== L.structure) problems.push('token structure changed (something other than literals was edited)');
  const changed = new Map(wb.changes.map(c => [c.id, c]));
  let maxRoundErr = 0;
  for (let i = 0; i < Math.min(L.literals.length, L2.literals.length); i++) {
    const a = L.literals[i], b = L2.literals[i];
    if (changed.has(i)) {
      if (Math.abs(b.value - changed.get(i).value) > 1e-12) problems.push(`${a.name}: wrote ${changed.get(i).value}, re-lift reads ${b.value}`);
      maxRoundErr = Math.max(maxRoundErr, Math.abs(b.value - values[i]));
    } else if (b.value !== a.value) problems.push(`${a.name}: changed unexpectedly ${a.value} -> ${b.value}`);
  }
  return { ok: problems.length === 0, problems, lift: L2, maxRoundErr, ms: performance.now() - t0 };
}
