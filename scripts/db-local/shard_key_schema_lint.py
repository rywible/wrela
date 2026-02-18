#!/usr/bin/env python3
"""
Shard-key schema lint gate for WRE-611.

Schema input JSON shape (minimal):
{
  "table": "orders",
  "fields": {
    "tenant_id": {"type": "string"},
    "region": {"type": "enum", "variants": ["us", "eu", "ap"]}
  },
  "shard_key": {
    "fields": ["tenant_id", "order_id"],
    "allow_single_shard_key": {"reason": "..."}
  }
}
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass


COARSE_LOCALITY_NAMES = {
    "region",
    "country",
    "zone",
    "continent",
    "locale",
    "datacenter",
}
TINY_ENUM_MAX_VARIANTS = 8


@dataclass
class LintFinding:
    level: str
    code: str
    message: str


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("schema_json", type=argparse.FileType("r"))
    p.add_argument("--format", choices=("text", "json"), default="text")
    p.add_argument(
        "--strict-low-cardinality",
        action="store_true",
        help="fail on low-cardinality shard key components",
    )
    p.add_argument("--report", type=argparse.FileType("w"))
    return p.parse_args()


def field_type_of(fields: dict, name: str) -> tuple[str | None, dict | None]:
    spec = fields.get(name)
    if spec is None:
        return None, None
    if isinstance(spec, str):
        return spec, {"type": spec}
    if isinstance(spec, dict):
        return spec.get("type"), spec
    return None, None


def lint_schema(payload: dict, strict_low_cardinality: bool) -> tuple[bool, list[LintFinding], dict]:
    findings: list[LintFinding] = []
    meta: dict = {"waiver": None}

    table = payload.get("table") or "<unknown>"
    fields = payload.get("fields") or {}
    shard_key = payload.get("shard_key") or {}
    key_fields = shard_key.get("fields") or []

    if not isinstance(key_fields, list) or not key_fields:
        findings.append(
            LintFinding(
                level="error",
                code="shard_key.missing",
                message="shard_key.fields must be a non-empty list",
            )
        )
        return False, findings, meta

    missing_fields = [name for name in key_fields if name not in fields]
    if missing_fields:
        findings.append(
            LintFinding(
                level="error",
                code="shard_key.unknown_field",
                message=f"shard key references unknown fields: {', '.join(missing_fields)}",
            )
        )

    waiver = shard_key.get("allow_single_shard_key")
    if len(key_fields) == 1:
        if not waiver:
            findings.append(
                LintFinding(
                    level="error",
                    code="shard_key.single_disallowed",
                    message=(
                        "single-field shard key is disallowed by default; "
                        "use composite keys (for example tenant_id + entity_id) or add "
                        "allow_single_shard_key.reason with explicit justification"
                    ),
                )
            )
        else:
            reason = ""
            if isinstance(waiver, dict):
                reason = str(waiver.get("reason") or "").strip()
            if len(reason) < 12:
                findings.append(
                    LintFinding(
                        level="error",
                        code="shard_key.waiver_reason_required",
                        message="allow_single_shard_key.reason must be non-empty and descriptive",
                    )
                )
            else:
                meta["waiver"] = {
                    "table": table,
                    "reason": reason,
                    "fields": key_fields,
                }
                findings.append(
                    LintFinding(
                        level="warning",
                        code="shard_key.waived_single",
                        message=f"single-field shard key waiver in effect: {reason}",
                    )
                )

    for name in key_fields:
        typ, spec = field_type_of(fields, name)
        if typ is None:
            continue

        low_card = False
        why = ""
        t = typ.lower()
        if t in {"bool", "boolean"}:
            low_card = True
            why = "boolean has cardinality 2"
        elif t == "enum":
            variants = []
            if isinstance(spec, dict):
                variants = spec.get("variants") or []
            if isinstance(variants, list) and 0 < len(variants) <= TINY_ENUM_MAX_VARIANTS:
                low_card = True
                why = f"tiny enum has only {len(variants)} variants"

        if name.lower() in COARSE_LOCALITY_NAMES:
            low_card = True
            why = "coarse locality-only key component"

        if low_card:
            level = "error" if strict_low_cardinality else "warning"
            findings.append(
                LintFinding(
                    level=level,
                    code="shard_key.low_cardinality",
                    message=f"{name}: {why}; add a high-cardinality suffix (for example entity_id)",
                )
            )

    passed = not any(f.level == "error" for f in findings)
    return passed, findings, meta


def emit_report(report_file, table: str, passed: bool, findings: list[LintFinding], meta: dict) -> None:
    report_file.write(f"# Shard Key Lint Report: {table}\n\n")
    report_file.write(f"- Status: {'PASS' if passed else 'FAIL'}\n\n")
    if meta.get("waiver"):
        waiver = meta["waiver"]
        report_file.write("## Waiver\n\n")
        report_file.write(f"- Fields: {', '.join(waiver['fields'])}\n")
        report_file.write(f"- Reason: {waiver['reason']}\n\n")

    report_file.write("## Findings\n\n")
    if not findings:
        report_file.write("- none\n")
    else:
        for f in findings:
            report_file.write(f"- [{f.level}] `{f.code}`: {f.message}\n")


def main() -> int:
    args = parse_args()
    payload = json.load(args.schema_json)
    table = payload.get("table") or "<unknown>"

    passed, findings, meta = lint_schema(payload, args.strict_low_cardinality)

    if args.format == "json":
        print(
            json.dumps(
                {
                    "status": "pass" if passed else "fail",
                    "table": table,
                    "findings": [f.__dict__ for f in findings],
                    "waiver": meta.get("waiver"),
                },
                sort_keys=True,
            )
        )
    else:
        prefix = "PASS" if passed else "FAIL"
        print(f"{prefix}: shard-key lint for table {table}")
        for f in findings:
            print(f"  - [{f.level}] {f.code}: {f.message}")

    if args.report:
        emit_report(args.report, table, passed, findings, meta)

    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
