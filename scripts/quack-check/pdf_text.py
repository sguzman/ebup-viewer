#!/usr/bin/env python3
import json
import re
import sys
import unicodedata
from pathlib import Path

PdfReader = None
try:
    from pypdf import PdfReader
except Exception:
    PdfReader = None

try:
    import pypdfium2 as pdfium
except Exception:
    pdfium = None

HYPHEN_RE = re.compile(r"(\w)-\n(\w)")
SPACE_RE = re.compile(r"[\t\f\r ]+")


def normalize_text(text: str, cfg: dict) -> str:
    if cfg.get("normalize_unicode", False):
        text = unicodedata.normalize("NFKC", text)
    if cfg.get("fix_hyphenation", False):
        text = HYPHEN_RE.sub(r"\1\2", text)
    if cfg.get("collapse_whitespace", False):
        lines = []
        for line in text.splitlines():
            line = SPACE_RE.sub(" ", line).strip()
            lines.append(line)
        text = "\n".join(lines)
    return text


def convert(req: dict, cfg: dict) -> None:
    input_pdf = Path(req["input_pdf"])
    start_page = int(req.get("start_page", 1))
    end_page = int(req.get("end_page", 1))

    native_cfg = cfg.get("native_text", {})
    light_md = bool(native_cfg.get("light_markdown", False))

    reader = None
    doc = None
    if PdfReader is not None:
        try:
            reader = PdfReader(str(input_pdf))
        except Exception as e:
            print(json.dumps({"ok": False, "warnings": [f"failed to read pdf: {e}"], "markdown": "", "meta": {}}))
            return
        n_pages = len(reader.pages)
    elif pdfium is not None:
        try:
            doc = pdfium.PdfDocument(str(input_pdf))
        except Exception as e:
            print(json.dumps({"ok": False, "warnings": [f"failed to read pdf via pypdfium2: {e}"], "markdown": "", "meta": {}}))
            return
        n_pages = len(doc)
    else:
        print(json.dumps({"ok": False, "warnings": ["missing pypdf and pypdfium2 imports"], "markdown": "", "meta": {}}))
        return
    warnings = []
    if start_page < 1 or end_page < start_page or end_page > n_pages:
        # If we were given original page numbers but are operating on a split chunk,
        # fall back to the full chunk range.
        if n_pages > 0 and start_page > 1:
            warnings.append(
                f"invalid page range {start_page}-{end_page} for split chunk (pages={n_pages}); falling back to 1-{n_pages}"
            )
            start_page = 1
            end_page = n_pages
        else:
            print(
                json.dumps(
                    {
                        "ok": False,
                        "warnings": [
                            f"invalid page range: {start_page}-{end_page} (pages={n_pages})"
                        ],
                        "markdown": "",
                        "meta": {},
                    }
                )
            )
            return

    parts = []
    for page_index in range(start_page - 1, end_page):
        if reader is not None:
            text = reader.pages[page_index].extract_text() or ""
        else:
            page = doc[page_index]
            text_page = page.get_textpage()
            text = text_page.get_text_range() or ""
            text_page.close()
            page.close()
        text = normalize_text(text, native_cfg)
        if light_md:
            parts.append(f"## Page {page_index + 1}\n\n{text}")
        else:
            parts.append(text)

    markdown = "\n\n".join(parts)
    out = {
        "ok": True,
        "markdown": markdown,
        "warnings": warnings,
        "meta": {"start_page": start_page, "end_page": end_page, "engine": "native_text"},
    }
    print(json.dumps(out))
    if doc is not None:
        doc.close()


def extract_page_texts(input_pdf: Path, cfg: dict) -> None:
    native_cfg = cfg.get("native_text", {})

    reader = None
    doc = None
    if PdfReader is not None:
        try:
            reader = PdfReader(str(input_pdf))
        except Exception as e:
            print(json.dumps({"ok": False, "page_texts": [], "warnings": [f"failed to read pdf: {e}"], "meta": {}, "error": str(e)}))
            return
        n_pages = len(reader.pages)
    elif pdfium is not None:
        try:
            doc = pdfium.PdfDocument(str(input_pdf))
        except Exception as e:
            print(json.dumps({"ok": False, "page_texts": [], "warnings": [f"failed to read pdf via pypdfium2: {e}"], "meta": {}, "error": str(e)}))
            return
        n_pages = len(doc)
    else:
        print(json.dumps({"ok": False, "page_texts": [], "warnings": ["missing pypdf and pypdfium2 imports"], "meta": {}, "error": "missing pdf backends"}))
        return

    page_texts = []
    for page_index in range(n_pages):
        if reader is not None:
            text = reader.pages[page_index].extract_text() or ""
        else:
            page = doc[page_index]
            text_page = page.get_textpage()
            text = text_page.get_text_range() or ""
            text_page.close()
            page.close()
        page_texts.append(normalize_text(text, native_cfg))

    print(json.dumps({
        "ok": True,
        "page_texts": page_texts,
        "warnings": [],
        "meta": {"page_count": n_pages, "engine": "native_text"}
    }))
    if doc is not None:
        doc.close()


def main() -> None:
    payload = json.loads(sys.stdin.read().strip() or "{}")
    cmd = payload.get("cmd")
    if cmd == "convert":
        convert(payload.get("req", {}), payload.get("cfg", {}))
        return
    if cmd == "extract_page_texts":
        input_pdf = Path(payload.get("input_pdf", ""))
        extract_page_texts(input_pdf, payload.get("cfg", {}))
        return
    print(json.dumps({"ok": False, "warnings": [f"unknown cmd: {cmd}"], "markdown": "", "meta": {}, "error": f"unknown cmd: {cmd}"}))


if __name__ == "__main__":
    main()
