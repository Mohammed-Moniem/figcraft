"""Regression checks for compare_figma_exports. Test fixtures are generated with ReportLab."""

import tempfile
import unittest
from pathlib import Path

from reportlab.pdfgen import canvas

from compare_figma_exports import compare


def draw_pdf(path, pages):
    doc = canvas.Canvas(str(path))
    for width, height, text in pages:
        doc.setPageSize((width, height))
        doc.setFillColorRGB(0, 0, 0)
        doc.rect(8, 10, 12, 14, fill=1)
        # White on white has identical pixels even if a round trip loses the selectable text.
        doc.setFillColorRGB(1, 1, 1)
        doc.drawString(8, 40, text)
        doc.showPage()
    doc.save()


class ExportComparisonTests(unittest.TestCase):
    def measure(self, first, second):
        with tempfile.TemporaryDirectory() as tmp:
            original, reopened = (Path(tmp) / name for name in ("first.pdf", "second.pdf"))
            draw_pdf(original, first)
            draw_pdf(reopened, second)
            return compare(original, reopened)

    def test_retains_text_and_pixels(self):
        pages = [(120, 80, "Title"), (100, 64, "Second")]
        result = self.measure(pages, pages)
        self.assertEqual(2, result["pixel_identical_pages"])
        self.assertEqual(2, result["matching_text_pages"])

    def test_reports_lost_selectable_text_even_if_the_pixels_match(self):
        result = self.measure([(120, 80, "Title")], [(120, 80, "")])
        self.assertEqual(1, result["pixel_identical_pages"])
        self.assertEqual(0, result["matching_text_pages"])
        self.assertEqual(0, result["pages"][0]["reopened_text_length"])

    def test_retains_page_count_and_size_discrepancies(self):
        result = self.measure([(120, 80, "One"), (80, 80, "Two")], [(100, 80, "One")])
        self.assertEqual((2, 1), (result["original_pages"], result["reopened_pages"]))
        self.assertEqual(0, result["same_size_pages"])
        self.assertEqual(0, result["pixels_compared"])


if __name__ == "__main__":
    unittest.main()
