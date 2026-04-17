//! Owns the query-contract catalog command surface and its human/JSON
//! projection.
//! Does not own CLI parsing or contract catalog construction.
//!
//! Key invariants:
//! - the handler receives only typed catalog args, so command legality stays at
//!   parse time.
//! - human and machine-readable catalog output must project the same snapshot.

use super::*;

pub(crate) fn execute_query_contracts_command(args: CatalogCommandArgs) {
    let catalog = query_contract_catalog_snapshot();
    if matches!(args.output_format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| "{}".to_string())
        );
        return;
    }
    println!("query contract catalog schema v{}", catalog.schema_version);
    for contract in &catalog.contracts {
        let backends = if contract.backends.is_empty() {
            "none".to_string()
        } else {
            contract.backends.join(",")
        };
        println!(
            "{} v{}  call={}  target={}  cardinality={}  surface={}  capture={}  item={}  result={}  backends={}  legacy={}",
            contract.contract_id,
            contract.contract_version,
            contract.call,
            contract.target,
            contract.cardinality,
            contract.surface,
            contract.capture_kind,
            contract.item_kind,
            contract.result_kind,
            backends,
            contract.legacy_builtin,
        );
    }
    if !catalog.aliases.is_empty() {
        println!("aliases:");
        for alias in &catalog.aliases {
            println!(
                "{} -> {}  ({})",
                alias.alias_id, alias.canonical_id, alias.reason
            );
        }
    }
}
