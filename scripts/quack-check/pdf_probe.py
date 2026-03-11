#!/usr/bin/env python3
import json
import re
import sys
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

GARBAGE_RE = re.compile(r"[\uFFFD]")
TOKEN_RE = re.compile(r"\w+", re.UNICODE)
NON_LATIN_RE = re.compile(r"[^\u0000-\u024F\s]", re.UNICODE)


def _normalize_boundary_line(text: str) -> str:
    line = " ".join(text.strip().split())
    line = re.sub(r"\d+", "#", line)
    return line[:160]


def _page_features(text: str, page_index: int) -> dict:
    total_chars = len(text)
    total_ws = sum(1 for c in text if c.isspace())
    total_garbage = len(GARBAGE_RE.findall(text))
    total_punct = sum(1 for c in text if not c.isalnum() and not c.isspace())
    total_digits = sum(1 for c in text if c.isdigit())
    total_non_latin = len(NON_LATIN_RE.findall(text))
    tokens = TOKEN_RE.findall(text)
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    return {
        "page_index": page_index + 1,
        "char_count": total_chars,
        "token_count": len(tokens),
        "line_count": len(lines),
        "whitespace_ratio": float(total_ws / max(1, total_chars)),
        "garbage_ratio": float(total_garbage / max(1, total_chars)),
        "punctuation_ratio": float(total_punct / max(1, total_chars)),
        "digit_ratio": float(total_digits / max(1, total_chars)),
        "non_latin_ratio": float(total_non_latin / max(1, total_chars)),
        "first_line": _normalize_boundary_line(lines[0]) if lines else "",
        "last_line": _normalize_boundary_line(lines[-1]) if lines else "",
    }


def main() -> None:
    req = json.loads(sys.stdin.read().strip() or "{}")
    input_pdf = Path(req["input_pdf"])
    sample_pages = int(req.get("sample_pages", 12))

    reader = None
    doc = None
    if PdfReader is not None:
        try:
            reader = PdfReader(str(input_pdf))
        except Exception as e:
            print(
                json.dumps(
                    {
                        "page_count": 0,
                        "sampled_pages": 0,
                        "avg_chars_per_page": 0,
                        "garbage_ratio": 1.0,
                        "whitespace_ratio": 1.0,
                        "error": f"failed to read pdf: {e}",
                    }
                )
            )
            return
        n_pages = len(reader.pages)
    elif pdfium is not None:
        try:
            doc = pdfium.PdfDocument(str(input_pdf))
        except Exception as e:
            print(
                json.dumps(
                    {
                        "page_count": 0,
                        "sampled_pages": 0,
                        "avg_chars_per_page": 0,
                        "garbage_ratio": 1.0,
                        "whitespace_ratio": 1.0,
                        "error": f"failed to read pdf via pypdfium2: {e}",
                    }
                )
            )
            return
        n_pages = len(doc)
    else:
        print(
            json.dumps(
                {
                    "page_count": 0,
                    "sampled_pages": 0,
                    "avg_chars_per_page": 0,
                    "garbage_ratio": 1.0,
                    "whitespace_ratio": 1.0,
                    "error": "missing pypdf and pypdfium2 imports",
                }
            )
        )
        return
    if n_pages == 0:
        out = dict(
            page_count=0,
            sampled_pages=0,
            avg_chars_per_page=0,
            garbage_ratio=1.0,
            whitespace_ratio=1.0,
        )
        print(json.dumps(out))
        return

    k = min(sample_pages, n_pages)
    idxs = []
    if k == 1:
        idxs = [0]
    else:
        for i in range(k):
            idxs.append(round(i * (n_pages - 1) / (k - 1)))

    total_chars = 0
    total_ws = 0
    total_garbage = 0
    page_stats = []

    for i in idxs:
        if reader is not None:
            txt = reader.pages[i].extract_text() or ""
        else:
            page = doc[i]
            text_page = page.get_textpage()
            txt = text_page.get_text_range() or ""
            text_page.close()
            page.close()
        total_chars += len(txt)
        total_ws += sum(1 for c in txt if c.isspace())
        total_garbage += len(GARBAGE_RE.findall(txt))
        page_stats.append(_page_features(txt, i))

    avg = int(total_chars / max(1, len(idxs)))
    garbage_ratio = float(total_garbage / max(1, total_chars))
    whitespace_ratio = float(total_ws / max(1, total_chars))
    text_pages = sum(1 for page in page_stats if page["char_count"] > 0)
    empty_pages = sum(1 for page in page_stats if page["char_count"] == 0)
    sparse_pages = sum(1 for page in page_stats if 0 < page["char_count"] < 120)
    noisy_pages = sum(1 for page in page_stats if page["garbage_ratio"] >= 0.03)

    first_lines = [page["first_line"] for page in page_stats if page["first_line"]]
    last_lines = [page["last_line"] for page in page_stats if page["last_line"]]
    repeated_headers = len(first_lines) - len(set(first_lines))
    repeated_footers = len(last_lines) - len(set(last_lines))
    denom = max(1, len(page_stats))

    out = dict(
        page_count=n_pages,
        sampled_pages=len(idxs),
        avg_chars_per_page=avg,
        garbage_ratio=garbage_ratio,
        whitespace_ratio=whitespace_ratio,
        text_page_ratio=float(text_pages / denom),
        empty_text_page_ratio=float(empty_pages / denom),
        sparse_text_page_ratio=float(sparse_pages / denom),
        noisy_text_page_ratio=float(noisy_pages / denom),
        repeated_header_ratio=float(repeated_headers / denom),
        repeated_footer_ratio=float(repeated_footers / denom),
        pages=page_stats,
    )
    print(json.dumps(out))
    if doc is not None:
        doc.close()


if __name__ == "__main__":
    main()
