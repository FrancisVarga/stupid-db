#!/usr/bin/env python3
"""
OCR fallback for scanned PDFs using EasyOCR + PyMuPDF.
Called by Rust when pdf-extract returns empty text.

Usage:
    python ocr_pdf.py <pdf_path>

Output:
    JSON to stdout: {"pages": [{"page_number": 1, "text": "..."}]}
"""
import sys
import json
import fitz  # PyMuPDF
import easyocr
from pathlib import Path


def ocr_pdf(pdf_path: str) -> dict:
    """
    Extract text from scanned PDF using OCR.

    Args:
        pdf_path: Path to PDF file

    Returns:
        Dict with structure: {"pages": [{"page_number": int, "text": str}]}
    """
    # Initialize EasyOCR reader (English by default, add more languages as needed)
    # Languages: ['en'] for English, ['en', 'de'] for English+German, etc.
    reader = easyocr.Reader(['en'], gpu=False, verbose=False)

    # Open PDF
    doc = fitz.open(pdf_path)
    pages = []

    for page_num in range(len(doc)):
        page = doc.load_page(page_num)

        # Convert page to image (pixmap)
        # zoom=2 gives 144 DPI (better OCR quality), use zoom=1.5 for 108 DPI
        mat = fitz.Matrix(2, 2)
        pix = page.get_pixmap(matrix=mat)

        # Convert pixmap to PIL Image bytes
        img_bytes = pix.tobytes("png")

        # Run OCR
        # readtext returns list of (bbox, text, confidence)
        results = reader.readtext(img_bytes)

        # Extract text from results, join with spaces
        page_text = " ".join([text for (_, text, _) in results])

        pages.append({
            "page_number": page_num + 1,
            "text": page_text.strip()
        })

    doc.close()

    return {"pages": pages}


def main():
    if len(sys.argv) != 2:
        print(json.dumps({"error": "Usage: ocr_pdf.py <pdf_path>"}), file=sys.stderr)
        sys.exit(1)

    pdf_path = sys.argv[1]

    if not Path(pdf_path).exists():
        print(json.dumps({"error": f"File not found: {pdf_path}"}), file=sys.stderr)
        sys.exit(1)

    try:
        result = ocr_pdf(pdf_path)
        print(json.dumps(result))
    except Exception as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
