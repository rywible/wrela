//! `wrela index`: every item of a package, with its kind and place, hashes of its signature and
//! body, and a function's effects and calls, sorted by path (for forge).

use crate::common;

use serde_json::Value;
use std::path::PathBuf;

/// The shapes program, with a constant and an alias too, as `edit` changes it.
fn package(tag: &str, edit: impl Fn(String) -> String) -> PathBuf {
    let main = format!("{}\nconst SIDE: f32 = 2.0\n\ntype Side = f32\n", common::SHAPES);
    common::package(
        &format!("index-shapes-{tag}"),
        &[("wrela.toml", &common::manifest("shapes")), ("main.wrela", &edit(main))],
    )
}

/// The package's index, as `wrela index --json` prints it: its items by path.
fn index(dir: &PathBuf) -> Vec<Value> {
    let index = common::answer(common::wrela().arg("index").arg("--json").arg(dir));
    assert_eq!(index["errors"], 0, "{index}");
    index["items"].as_array().cloned().unwrap_or_else(|| panic!("no items: {index}"))
}

fn item<'a>(items: &'a [Value], path: &str) -> &'a Value {
    items.iter().find(|i| i["path"] == path).unwrap_or_else(|| panic!("no `{path}`"))
}

#[test]
fn items_are_listed_with_their_kinds_and_places_sorted_by_path() {
    let items = index(&package("kinds", |m| m));
    let paths: Vec<&str> = items.iter().map(|i| i["path"].as_str().unwrap()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(paths, sorted, "sorted and unique");
    assert!(paths.iter().all(|p| p.starts_with("main::")), "std's items aren't listed: {paths:?}");
    let place = |path: &str| {
        let i = item(&items, path);
        (i["kind"].as_str(), i["file"].as_str(), i["line"].as_u64(), i["end_line"].as_u64())
    };
    assert_eq!(place("main::Grid"), (Some("struct"), Some("main.wrela"), Some(4), Some(6)));
    assert_eq!(place("main::Grid::at"), (Some("method"), Some("main.wrela"), Some(10), Some(12)));
    assert_eq!(place("main::Shape").0, Some("trait"));
    assert_eq!(place("main::Shape::area").0, Some("method"));
    assert_eq!(place("main::Square::area").0, Some("method"));
    assert_eq!(place("main::total").0, Some("function"));
    assert_eq!(place("main::SIDE").0, Some("constant"));
    assert_eq!(place("main::Side").0, Some("type"));
    for i in &items {
        for hash in [&i["signature"], &i["body"]] {
            assert!(hash.as_str().is_some_and(|h| h.len() == 32), "{i}");
        }
    }
    let with_std = common::answer(
        common::wrela().args(["index", "--json", "--std"]).arg(package("std", |m| m)),
    );
    let std = with_std["items"].as_array().unwrap();
    assert!(std.iter().any(|i| i["path"].as_str().is_some_and(|p| p.starts_with("std::"))));
}

#[test]
fn calls_are_the_resolved_callees_and_effects_are_inferred() {
    let items = index(&package("calls", |m| m));
    let calls = |path: &str| item(&items, path)["calls"].clone();
    assert_eq!(calls("main::first"), serde_json::json!(["main::Grid::at"]));
    // A generic parameter's method is the trait's; `frame` calls each function it names.
    assert_eq!(calls("main::total"), serde_json::json!(["main::Shape::area"]));
    let frame = calls("main::frame");
    for callee in ["main::first", "main::grow", "main::total"] {
        assert!(frame.as_array().unwrap().iter().any(|c| c == callee), "{callee}: {frame}");
    }
    assert!(item(&items, "main::grow")["effects"].as_array().unwrap().iter().any(|e| e == "alloc"));
    assert_eq!(item(&items, "main::Square::area")["effects"], serde_json::json!([]));
}

#[test]
fn only_a_change_to_tokens_changes_a_hash() {
    let hashes = |tag: &str, edit: &dyn Fn(String) -> String| {
        let edited = |m: String| {
            let e = edit(m.clone());
            assert!(tag == "before" || e != m, "the edit `{tag}` changes the text");
            e
        };
        let items = index(&package(tag, edited));
        let at = item(&items, "main::Grid::at");
        (at["signature"].clone(), at["body"].clone())
    };
    let (sig, body) = hashes("before", &|m| m);
    let respaced = hashes("respaced", &|m| {
        m.replace("/// The height at `i`.", "/// The height at index `i`, as it was.")
            .replace(
                "pub fn at(self, i: u32) -> f32 {",
                "pub   fn at( self,  i: u32 )  ->  f32  {  // the cell",
            )
            .replace(
                "        self.cells[i]\n",
                "\n        // the cell's height\n        self.cells[ i ]\n\n",
            )
    });
    assert_eq!(respaced, (sig.clone(), body.clone()), "comments and whitespace");
    let new_body = hashes("body", &|m| m.replace("self.cells[i]\n", "self.cells[i] + 1.0\n"));
    assert_eq!(new_body.0, sig, "the signature is the same");
    assert_ne!(new_body.1, body, "the body changed");
    let new_sig = hashes("signature", &|m| m.replace("pub fn at(", "fn at("));
    assert_ne!(new_sig.0, sig, "the signature changed");
    assert_eq!(new_sig.1, body, "the body is the same");
}
