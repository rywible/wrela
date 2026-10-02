//! A small incremental query system: every compiler phase is a pure function of its inputs,
//! memoized, and re-run only when something it read has changed.
//!
//! - **Inputs** ([`Input`]) are set from outside with [`Db::set`]. Each change starts a new
//!   [`Revision`]; setting an input to an equal value changes nothing.
//! - **Derived queries** ([`Derived`]) compute a value from a key, reading inputs and other
//!   queries through the [`Db`]. Each read is recorded as a dependency.
//! - **Re-validation (red/green):** a memo from an older revision is reused if none of its
//!   dependencies changed since it was last verified; derived dependencies are brought up to date
//!   first, recursively. Otherwise the query re-runs.
//! - **Early cutoff:** a re-run that produces a value equal to the old one keeps the old
//!   "changed at" revision, so queries that depend on it don't re-run.
//! - **Cycles** are errors, not panics: a query that (transitively) asks for itself gets
//!   [`QueryError::Cycle`], which a query can recover from or pass up with `?`.
//!
//! Values are cloned out of the memo table, so large values should be behind an `Arc`.
//! Single-threaded by design (M1 doesn't need parallel queries); the `Db` can move between
//! threads. Old memos are never evicted.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug};
use std::hash::Hash;
use std::sync::Arc;

/// A query's identity and types. Implement [`Input`] or [`Derived`] as well.
pub trait Query: 'static {
    type Key: Clone + Eq + Hash + Debug + Send + 'static;
    type Value: Clone + Eq + Debug + Send + 'static;
    /// The name used in cycle reports and the execution log, like `lex`.
    const NAME: &'static str;
}

/// A query whose values are set from outside with [`Db::set`] and read with [`Db::input`].
pub trait Input: Query {}

/// A query whose values are computed, read with [`Db::get`].
pub trait Derived: Query {
    /// Computes the value for `key`. Must be deterministic and read state only through `db`.
    fn compute(db: &Db, key: &Self::Key) -> Result<Self::Value, QueryError>;
}

/// A point in the history of the inputs. It advances each time an input changes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Revision(u64);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum QueryError {
    Cycle(Cycle),
    /// A query read an input that was never set: a bug in the driver, not in the user's program.
    MissingInput {
        query: &'static str,
        key: String,
    },
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::Cycle(cycle) => write!(f, "{cycle}"),
            QueryError::MissingInput { query, key } => {
                write!(f, "no value was set for the input `{query}({key})`")
            }
        }
    }
}

impl std::error::Error for QueryError {}

/// The queries in a cycle, each written `name(key)`. The list is rotated to start at the least
/// participant, so the same cycle reads the same whichever query was asked for first.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Cycle {
    participants: Vec<String>,
}

impl Cycle {
    fn new(mut participants: Vec<String>) -> Self {
        let first = (0..participants.len())
            .min_by_key(|&i| &participants[i])
            .unwrap_or(0);
        participants.rotate_left(first);
        Cycle { participants }
    }

    pub fn participants(&self) -> &[String] {
        &self.participants
    }
}

impl fmt::Display for Cycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("query cycle: ")?;
        for participant in &self.participants {
            write!(f, "{participant} -> ")?;
        }
        f.write_str(self.participants.first().map_or("", String::as_str))
    }
}

/// The query database: inputs, memos and the stack of queries being computed.
pub struct Db {
    runtime: RefCell<Runtime>,
}

impl Default for Db {
    fn default() -> Self {
        Db::new()
    }
}

impl Debug for Db {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let runtime = self.runtime.borrow();
        let names: Vec<&str> = runtime.tables.iter().map(|t| t.name).collect();
        f.debug_struct("Db")
            .field("revision", &runtime.revision)
            .field("tables", &names)
            .finish()
    }
}

/// A query type's table, with its key: the unit of dependency tracking.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct DepKey {
    table: u32,
    key: u32,
}

struct Runtime {
    revision: Revision,
    tables: Vec<Table>,
    table_index: HashMap<TypeId, u32>,
    /// The queries being computed or verified, innermost last.
    stack: Vec<Frame>,
    active: HashSet<DepKey>,
    log: Option<Vec<String>>,
}

struct Frame {
    query: DepKey,
    /// Dependencies read so far, in order, without repeats. Only computing frames record.
    deps: Vec<DepKey>,
    seen: HashSet<DepKey>,
    computing: bool,
}

/// One query type's storage, type-erased, with the monomorphized functions that operate on it.
struct Table {
    name: &'static str,
    data: Box<dyn Any + Send>,
    /// Brings the entry up to date and reports whether it changed after the given revision.
    changed_after: fn(&Db, DepKey, Revision) -> bool,
    /// `name(key)`, for cycle reports.
    describe: fn(&Runtime, DepKey) -> String,
}

struct Interner<K> {
    keys: Vec<K>,
    index: HashMap<K, u32>,
}

impl<K: Clone + Eq + Hash> Interner<K> {
    fn new() -> Self {
        Interner {
            keys: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// The key's index, and whether it was new.
    fn intern(&mut self, key: &K) -> (u32, bool) {
        if let Some(&i) = self.index.get(key) {
            return (i, false);
        }
        let i = u32::try_from(self.keys.len()).expect("fewer than 2^32 keys per query");
        self.keys.push(key.clone());
        self.index.insert(key.clone(), i);
        (i, true)
    }
}

struct InputTable<Q: Query> {
    keys: Interner<Q::Key>,
    slots: Vec<Option<InputSlot<Q::Value>>>,
}

struct InputSlot<V> {
    value: V,
    changed_at: Revision,
}

struct DerivedTable<Q: Query> {
    keys: Interner<Q::Key>,
    memos: Vec<Option<Memo<Q::Value>>>,
}

struct Memo<V> {
    value: Result<V, QueryError>,
    /// The latest revision in which the value is known to be current.
    verified_at: Revision,
    /// The revision in which the value last changed.
    changed_at: Revision,
    deps: Arc<[DepKey]>,
}

impl Runtime {
    fn table_for<T: Any + Send>(
        &mut self,
        name: &'static str,
        new: impl FnOnce() -> T,
        changed_after: fn(&Db, DepKey, Revision) -> bool,
        describe: fn(&Runtime, DepKey) -> String,
    ) -> u32 {
        let type_id = TypeId::of::<T>();
        if let Some(&i) = self.table_index.get(&type_id) {
            return i;
        }
        let i = u32::try_from(self.tables.len()).expect("fewer than 2^32 query types");
        self.tables.push(Table {
            name,
            data: Box::new(new()),
            changed_after,
            describe,
        });
        self.table_index.insert(type_id, i);
        i
    }

    fn data<T: Any>(&self, table: u32) -> &T {
        self.tables[table as usize]
            .data
            .downcast_ref()
            .expect("a table's type matches its index")
    }

    fn data_mut<T: Any>(&mut self, table: u32) -> &mut T {
        self.tables[table as usize]
            .data
            .downcast_mut()
            .expect("a table's type matches its index")
    }

    fn intern_input<Q: Input>(&mut self, key: &Q::Key) -> DepKey {
        let table = self.table_for(
            Q::NAME,
            || InputTable::<Q> {
                keys: Interner::new(),
                slots: Vec::new(),
            },
            input_changed_after::<Q>,
            describe_input::<Q>,
        );
        let data = self.data_mut::<InputTable<Q>>(table);
        let (key, new) = data.keys.intern(key);
        if new {
            data.slots.push(None);
        }
        DepKey { table, key }
    }

    fn intern_derived<Q: Derived>(&mut self, key: &Q::Key) -> DepKey {
        let table = self.table_for(
            Q::NAME,
            || DerivedTable::<Q> {
                keys: Interner::new(),
                memos: Vec::new(),
            },
            derived_changed_after::<Q>,
            describe_derived::<Q>,
        );
        let data = self.data_mut::<DerivedTable<Q>>(table);
        let (key, new) = data.keys.intern(key);
        if new {
            data.memos.push(None);
        }
        DepKey { table, key }
    }

    fn describe(&self, dep: DepKey) -> String {
        (self.tables[dep.table as usize].describe)(self, dep)
    }

    fn record(&mut self, dep: DepKey) {
        if let Some(frame) = self.stack.last_mut().filter(|frame| frame.computing)
            && frame.seen.insert(dep)
        {
            frame.deps.push(dep);
        }
    }
}

fn input_changed_after<Q: Input>(db: &Db, dep: DepKey, since: Revision) -> bool {
    let runtime = db.runtime.borrow();
    let slot = &runtime.data::<InputTable<Q>>(dep.table).slots[dep.key as usize];
    // An input that's still unset hasn't changed; once set, its revision is newer than `since`.
    slot.as_ref().is_some_and(|slot| slot.changed_at > since)
}

fn derived_changed_after<Q: Derived>(db: &Db, dep: DepKey, since: Revision) -> bool {
    if db.runtime.borrow().active.contains(&dep) {
        // It's being computed or verified further up the stack: the dependency graph recorded a
        // cycle. Assume a change; re-running the reader reports the cycle if it's still there.
        return true;
    }
    db.refresh::<Q>(dep);
    let runtime = db.runtime.borrow();
    let memo = runtime.data::<DerivedTable<Q>>(dep.table).memos[dep.key as usize].as_ref();
    memo.is_none_or(|memo| memo.changed_at > since)
}

fn describe_input<Q: Input>(runtime: &Runtime, dep: DepKey) -> String {
    let key = &runtime.data::<InputTable<Q>>(dep.table).keys.keys[dep.key as usize];
    format!("{}({key:?})", Q::NAME)
}

fn describe_derived<Q: Derived>(runtime: &Runtime, dep: DepKey) -> String {
    let key = &runtime.data::<DerivedTable<Q>>(dep.table).keys.keys[dep.key as usize];
    format!("{}({key:?})", Q::NAME)
}

/// Pops the frame it guards, even if the query's `compute` panics, so a caught panic doesn't
/// leave a stale frame that would later look like a cycle.
struct FrameGuard<'a> {
    db: &'a Db,
    popped: bool,
}

impl FrameGuard<'_> {
    fn pop(mut self) -> Frame {
        self.popped = true;
        self.db.pop_frame()
    }
}

impl Drop for FrameGuard<'_> {
    fn drop(&mut self) {
        if !self.popped {
            self.db.pop_frame();
        }
    }
}

impl Db {
    pub fn new() -> Self {
        Db {
            runtime: RefCell::new(Runtime {
                revision: Revision(0),
                tables: Vec::new(),
                table_index: HashMap::new(),
                stack: Vec::new(),
                active: HashSet::new(),
                log: None,
            }),
        }
    }

    pub fn revision(&self) -> Revision {
        self.runtime.borrow().revision
    }

    /// Sets an input. Returns whether it changed: an equal value leaves the revision alone.
    pub fn set<Q: Input>(&mut self, key: &Q::Key, value: Q::Value) -> bool {
        let runtime = self.runtime.get_mut();
        let dep = runtime.intern_input::<Q>(key);
        let next = Revision(runtime.revision.0 + 1);
        let slot = &mut runtime.data_mut::<InputTable<Q>>(dep.table).slots[dep.key as usize];
        if slot.as_ref().is_some_and(|slot| slot.value == value) {
            return false;
        }
        *slot = Some(InputSlot {
            value,
            changed_at: next,
        });
        runtime.revision = next;
        true
    }

    /// Reads an input, recording the dependency.
    pub fn input<Q: Input>(&self, key: &Q::Key) -> Result<Q::Value, QueryError> {
        let mut runtime = self.runtime.borrow_mut();
        let dep = runtime.intern_input::<Q>(key);
        runtime.record(dep);
        match &runtime.data::<InputTable<Q>>(dep.table).slots[dep.key as usize] {
            Some(slot) => Ok(slot.value.clone()),
            None => Err(QueryError::MissingInput {
                query: Q::NAME,
                key: format!("{key:?}"),
            }),
        }
    }

    /// Reads a derived query, computing it if no current memo exists, and records the dependency.
    pub fn get<Q: Derived>(&self, key: &Q::Key) -> Result<Q::Value, QueryError> {
        let dep = {
            let mut runtime = self.runtime.borrow_mut();
            let dep = runtime.intern_derived::<Q>(key);
            runtime.record(dep);
            if runtime.active.contains(&dep) {
                let start = runtime
                    .stack
                    .iter()
                    .position(|frame| frame.query == dep)
                    .unwrap_or(0);
                let participants = runtime.stack[start..]
                    .iter()
                    .map(|frame| runtime.describe(frame.query))
                    .collect();
                return Err(QueryError::Cycle(Cycle::new(participants)));
            }
            dep
        };
        self.refresh::<Q>(dep);
        let runtime = self.runtime.borrow();
        match &runtime.data::<DerivedTable<Q>>(dep.table).memos[dep.key as usize] {
            Some(memo) => memo.value.clone(),
            None => unreachable!("refresh leaves a memo for an inactive query"),
        }
    }

    /// Records every query execution from now on, for tests: `execute name(key)`.
    pub fn enable_log(&mut self) {
        self.runtime.get_mut().log.get_or_insert_with(Vec::new);
    }

    /// Returns and clears the execution log.
    pub fn take_log(&mut self) -> Vec<String> {
        self.runtime
            .get_mut()
            .log
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Makes the memo for `dep` current: reuse it if its dependencies are unchanged, else re-run.
    fn refresh<Q: Derived>(&self, dep: DepKey) {
        let (verified_at, deps) = {
            let runtime = self.runtime.borrow();
            let current = runtime.revision;
            match &runtime.data::<DerivedTable<Q>>(dep.table).memos[dep.key as usize] {
                Some(memo) if memo.verified_at == current => return,
                Some(memo) => (memo.verified_at, Arc::clone(&memo.deps)),
                None => {
                    drop(runtime);
                    return self.execute::<Q>(dep);
                }
            }
        };

        self.push_frame(dep, false);
        let guard = FrameGuard {
            db: self,
            popped: false,
        };
        let unchanged = deps.iter().all(|&d| !self.changed_after(d, verified_at));
        guard.pop();

        if unchanged {
            let mut runtime = self.runtime.borrow_mut();
            let current = runtime.revision;
            if let Some(memo) =
                &mut runtime.data_mut::<DerivedTable<Q>>(dep.table).memos[dep.key as usize]
            {
                memo.verified_at = current;
            }
        } else {
            self.execute::<Q>(dep);
        }
    }

    fn changed_after(&self, dep: DepKey, since: Revision) -> bool {
        let check = self.runtime.borrow().tables[dep.table as usize].changed_after;
        check(self, dep, since)
    }

    fn execute<Q: Derived>(&self, dep: DepKey) {
        let key = {
            let mut runtime = self.runtime.borrow_mut();
            let key =
                runtime.data::<DerivedTable<Q>>(dep.table).keys.keys[dep.key as usize].clone();
            if let Some(log) = &mut runtime.log {
                log.push(format!("execute {}({key:?})", Q::NAME));
            }
            key
        };

        self.push_frame(dep, true);
        let guard = FrameGuard {
            db: self,
            popped: false,
        };
        let value = Q::compute(self, &key);
        let frame = guard.pop();

        let mut runtime = self.runtime.borrow_mut();
        let current = runtime.revision;
        let slot = &mut runtime.data_mut::<DerivedTable<Q>>(dep.table).memos[dep.key as usize];
        let changed_at = match slot {
            Some(old) if old.value == value => old.changed_at,
            _ => current,
        };
        *slot = Some(Memo {
            value,
            verified_at: current,
            changed_at,
            deps: frame.deps.into(),
        });
    }

    fn push_frame(&self, query: DepKey, computing: bool) {
        let mut runtime = self.runtime.borrow_mut();
        runtime.active.insert(query);
        runtime.stack.push(Frame {
            query,
            deps: Vec::new(),
            seen: HashSet::new(),
            computing,
        });
    }

    fn pop_frame(&self) -> Frame {
        let mut runtime = self.runtime.borrow_mut();
        let frame = runtime
            .stack
            .pop()
            .expect("frames are pushed and popped in pairs");
        runtime.active.remove(&frame.query);
        frame
    }
}

#[cfg(test)]
mod tests;
