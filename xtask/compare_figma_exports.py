#!/usr/bin/env python3
"""Compare two Figma frame PDFs in page order at 72 dpi. Requires Poppler and Pillow."""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageChops


def poppler(diagnostics, tool, *args):
    run = subprocess.run([tool, *args], capture_output=True, env={**os.environ, "LC_ALL": "C"})
    if run.stderr:
        message = run.stderr.decode("utf-8", errors="replace").strip()
        if run.returncode:
            raise ValueError(f"{tool} exited with {run.returncode}: {message[:400]}")
        entry = diagnostics.setdefault(tool, {"calls_with_warnings": 0, "first_warning": message[:200]})
        entry["calls_with_warnings"] += 1
    run.check_returncode()
    return run.stdout


def page_count(pdf, diagnostics):
    info = poppler(diagnostics, "pdfinfo", pdf).decode("utf-8", errors="replace")
    match = re.search(r"^Pages:\s+(\d+)\s*$", info, re.MULTILINE)
    if not match:
        raise ValueError(f"Cannot read the page count for {pdf}")
    return int(match.group(1))


def extract_pages(pdf, count, diagnostics):
    text = poppler(diagnostics, "pdftotext", "-enc", "UTF-8", pdf, "-").decode("utf-8")
    pages = text.split("\f")
    if pages[-1] == "":
        pages.pop()
    if len(pages) != count:
        raise ValueError(f"Text extraction did not return {count} pages for {pdf}")
    return [" ".join(page.split()) for page in pages]


def render_page(pdf, number, prefix, diagnostics):
    poppler(diagnostics, "pdftoppm", "-r", "72", "-f", str(number), "-l", str(number),
            "-singlefile", "-png", pdf, prefix)
    with Image.open(prefix.with_suffix(".png")) as image:
        return image.convert("RGB")


def compare(original, reopened):
    diagnostics = {"original": {}, "reopened": {}}
    counts = [page_count(original, diagnostics["original"]), page_count(reopened, diagnostics["reopened"])]
    texts = [extract_pages(original, counts[0], diagnostics["original"]),
             extract_pages(reopened, counts[1], diagnostics["reopened"])]
    result = {"original_pages": counts[0], "reopened_pages": counts[1], "dpi": 72,
              "same_size_pages": 0, "pixel_identical_pages": 0, "matching_text_pages": 0,
              "pixels_compared": 0, "different_pixels": 0, "diagnostics": diagnostics, "pages": []}
    with tempfile.TemporaryDirectory() as tmp:
        for idx in range(min(counts)):
            page = idx + 1
            first = render_page(original, page, Path(tmp) / "original", diagnostics["original"])
            second = render_page(reopened, page, Path(tmp) / "reopened", diagnostics["reopened"])
            a, b = (t[idx] for t in texts)
            row = {"page": page, "original_size": first.size, "reopened_size": second.size,
                   "original_text_length": len(a), "reopened_text_length": len(b),
                   "matching_text": a == b}
            result["matching_text_pages"] += a == b
            if first.size == second.size:
                red, green, blue = ImageChops.difference(first, second).split()
                delta = ImageChops.lighter(ImageChops.lighter(red, green), blue)
                pixels = first.width * first.height
                changed = pixels - delta.histogram()[0]
                row["different_pixels"] = changed
                result["same_size_pages"] += 1
                result["pixel_identical_pages"] += changed == 0
                result["pixels_compared"] += pixels
                result["different_pixels"] += changed
            result["pages"].append(row)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("original", type=Path)
    parser.add_argument("reopened", type=Path)
    args = parser.parse_args()
    try:
        result = compare(args.original.resolve(), args.reopened.resolve())
    except (OSError, ValueError, subprocess.CalledProcessError, Image.DecompressionBombError) as exc:
        parser.exit(2, f"Could not compare PDFs: {exc}\n")
    print(json.dumps(result, indent=2))
    return int(not (result["pixel_identical_pages"] == result["matching_text_pages"]
                    == result["original_pages"] == result["reopened_pages"]))


if __name__ == "__main__":
    sys.exit(main())
