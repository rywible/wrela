//! `wrela diff-behaviour <old-package-dir> <new-package-dir> <item> [--samples n] [--seed s]
//! [--within e] [--json]`: where two versions of a package's free function behave differently
//! on the same seeded samples (`wrela_driver::behaviour`). Exit status 0 if no sample differs,
//! 1 if some do, 2 on a usage or build error.

use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::ExitCode;
use wrela_driver::behaviour::{self, Difference, Ending, Options, Outcome, Report};

pub fn run(args: &[String]) -> ExitCode {
    let mut options = Options::default();
    let mut json = false;
    let mut words = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |what: &str| {
            let v = it.next().map(String::as_str);
            if v.is_none() {
                eprintln!("error: `{a}` needs {what}");
            }
            v
        };
        match a.as_str() {
            "--json" => json = true,
            "--samples" => match value("a number of samples").map(str::parse) {
                Some(Ok(n)) => options.samples = n,
                _ => return crate::usage(),
            },
            "--seed" => match value("a seed").map(str::parse) {
                Some(Ok(s)) => options.seed = s,
                _ => return crate::usage(),
            },
            "--within" => match value("a tolerance").map(str::parse::<f64>) {
                Some(Ok(e)) if e >= 0.0 => options.within = Some(e),
                _ => return crate::usage(),
            },
            _ if !a.starts_with('-') && words.len() < 3 => words.push(a.clone()),
            _ => return crate::unknown(a),
        }
    }
    let [old, new, item] = words.as_slice() else { return crate::usage() };
    let (old, new) = (PathBuf::from(old), PathBuf::from(new));
    if let Err(status) = crate::package_dir(&old).and_then(|()| crate::package_dir(&new)) {
        return status;
    }
    let report = match behaviour::diff(&old, &new, item, options) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    if json {
        let mut v = to_json(&report);
        v["old"] = json!(old.display().to_string());
        v["new"] = json!(new.display().to_string());
        println!("{v}");
    } else {
        print!("{}", text(&report));
    }
    if report.differing == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

/// The samples' domain: how many, from which seed, and how floats were compared.
fn domain(r: &Report) -> String {
    let floats = match r.within {
        None => "floats compared bit for bit".to_string(),
        Some(e) => format!("floats within {e:?}"),
    };
    format!(
        "seed {}: {} of boundary values, {} seeded; {floats}",
        r.seed,
        crate::count(r.boundary, "sample", "samples"),
        r.seeded
    )
}

/// The report for people.
pub fn text(r: &Report) -> String {
    if r.differing == 0 {
        return format!(
            "`{}`: no differences on {} ({})\n",
            r.item,
            crate::count(r.samples(), "sample", "samples"),
            domain(r)
        );
    }
    let mut s = format!(
        "`{}`: {} of {} differ ({})\n",
        r.item,
        r.differing,
        crate::count(r.samples(), "sample", "samples"),
        domain(r)
    );
    for d in &r.differences {
        s.push_str(&difference(d));
    }
    if r.differing > r.differences.len() {
        s.push_str(&format!("\nand {} more\n", r.differing - r.differences.len()));
    }
    s
}

fn difference(d: &Difference) -> String {
    let inputs: Vec<String> = d.inputs.iter().map(|(n, v)| format!("{n} = {v}")).collect();
    let kind = if d.boundary { "boundary values" } else { "seeded" };
    format!(
        "\nsample {} ({kind}): {}\n  old: {}\n  new: {}\n",
        d.sample,
        if inputs.is_empty() { "no arguments".to_string() } else { inputs.join(", ") },
        outcome(&d.old),
        outcome(&d.new)
    )
}

fn outcome(o: &Outcome) -> String {
    let mut s = match &o.ending {
        Ending::Returned => match &o.result {
            Some(r) => format!("returned {r}"),
            None => "returned".to_string(),
        },
        Ending::Panicked(m) => format!("panicked: {m}"),
        Ending::Trapped(why) => format!("trapped: {why}"),
        Ending::OutOfFuel => "ran out of fuel".to_string(),
    };
    for (n, v) in &o.muts {
        s.push_str(&format!("; then {n} = {v}"));
    }
    s
}

/// The report for tools: the same, with each value as Wrela writes it.
pub fn to_json(r: &Report) -> Value {
    let named = |pairs: &[(String, String)]| -> Vec<Value> {
        pairs.iter().map(|(n, v)| json!({ "name": n, "value": v })).collect()
    };
    let outcome = |o: &Outcome| {
        let (ending, message) = match &o.ending {
            Ending::Returned => ("returned", None),
            Ending::Panicked(m) => ("panicked", Some(m)),
            Ending::Trapped(m) => ("trapped", Some(m)),
            Ending::OutOfFuel => ("out_of_fuel", None),
        };
        json!({ "ending": ending, "message": message, "result": o.result, "muts": named(&o.muts) })
    };
    let differences: Vec<Value> = r
        .differences
        .iter()
        .map(|d| {
            json!({
                "sample": d.sample,
                "boundary": d.boundary,
                "inputs": named(&d.inputs),
                "old": outcome(&d.old),
                "new": outcome(&d.new),
            })
        })
        .collect();
    json!({
        "item": r.item,
        "seed": r.seed,
        "samples": r.samples(),
        "boundary": r.boundary,
        "seeded": r.seeded,
        "floats": match r.within { None => json!("bits"), Some(e) => json!({ "within": e }) },
        "differing": r.differing,
        "differences": differences,
    })
}
