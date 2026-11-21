#!/usr/bin/env python3
"""Calculate the two headline estimates from the published Figma Design rubric."""

import argparse
from decimal import Decimal
from pathlib import Path
import re


def table_rows(markdown, header):
    lines = markdown.splitlines()
    start = lines.index(header)
    for line in lines[start + 2:]:
        if not line.startswith("|"):
            break
        yield [cell.strip() for cell in line.strip("|").split("|")]


def weighted_score(rows, expected_weight):
    numerator = total_weight = 0
    names = set()
    for name, weight_text, percent_text in rows:
        if name in names or not weight_text.isdecimal():
            raise ValueError(f"Invalid or duplicate rubric row {name!r}")
        names.add(name)
        match = re.fullmatch(r"~?(\d+(?:\.\d+)?)%", percent_text)
        if match is None or Decimal(match[1]) > 100:
            raise ValueError(f"Invalid score for {name!r}")
        weight = int(weight_text)
        numerator += weight * Decimal(match[1])
        total_weight += weight
    if total_weight != expected_weight:
        raise ValueError(f"Expected total weight {expected_weight}, found {total_weight}")
    return numerator / total_weight


def calculate(markdown):
    features = table_rows(markdown, "| # | Area | Weight | Breadth | Ready | Remaining | Hours |")
    dimensions = table_rows(markdown, "| Dimension | Weight | Ready | Hours | Evidence | Doc |")
    breadth = weighted_score(((r[0], r[2], r[3]) for r in features if r[0]), 113)
    readiness = weighted_score(((r[0], r[1], r[2]) for r in dimensions if r[0] != "**Ready for real work**"), 100)
    return breadth, readiness


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rubric", nargs="?", type=Path,
                        default=Path(__file__).resolve().parents[1] / "docs/target-app-parity.md")
    args = parser.parse_args()
    try:
        breadth, readiness = calculate(args.rubric.read_text(encoding="utf-8"))
    except (OSError, ValueError, IndexError) as error:
        parser.exit(1, f"Cannot calculate parity scores: {error}\n")
    print(f"Weighted readiness: {readiness:.2f}%")
    print(f"Weighted feature breadth: {breadth:.2f}%")
