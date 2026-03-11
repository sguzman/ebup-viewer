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


def _safe_float(value, default: float = 0.0) -> float:
    try:
        return float(value)
    except Exception:
        return default


def _count_pdf_images(page) -> int:
    try:
        resources = page.get("/Resources")
        if resources is None:
            return 0
        resources = resources.get_object()
        xobjects = resources.get("/XObject")
        if xobjects is None:
            return 0
        xobjects = xobjects.get_object()
        count = 0
        for _, obj in xobjects.items():
            try:
                resolved = obj.get_object()
                if resolved.get("/Subtype") == "/Image":
                    count += 1
            except Exception:
                continue
        return count
    except Exception:
        return 0


def _estimate_render_coverage(page) -> float:
    if pdfium is None:
        return 0.0
    try:
        bitmap = page.render(scale=0.2)
        pil = bitmap.to_pil().convert("L")
        pixels = list(pil.getdata())
        if not pixels:
            return 0.0
        non_white = sum(1 for px in pixels if px < 245)
        return float(non_white / len(pixels))
    except Exception:
        return 0.0


def _page_quality_features(text: str, image_object_count: int, image_coverage_ratio: float) -> dict:
    total_chars = len(text)
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    normalized_lines = [" ".join(line.split()) for line in lines]
    unique_lines = len(set(normalized_lines))
    repeated_lines = len(normalized_lines) - unique_lines
    duplicate_text_ratio = float(repeated_lines / max(1, len(lines)))
    avg_line_length = float(sum(len(line) for line in lines) / max(1, len(lines)))
    line_length_variance = float(
        sum((len(line) - avg_line_length) ** 2 for line in lines) / max(1, len(lines))
    )
    block_coherence = max(
        0.0,
        min(
            1.0,
            1.0
            - min(0.55, line_length_variance / 5000.0)
            - min(0.3, duplicate_text_ratio * 0.8),
        ),
    )
    coordinate_sanity = max(
        0.0,
        min(
            1.0,
            1.0
            - min(0.4, image_coverage_ratio * 0.5)
            - min(0.35, duplicate_text_ratio * 0.9)
            - (0.15 if total_chars > 0 and avg_line_length <= 18 else 0.0),
        ),
    )
    reading_order_stability = max(
        0.0,
        min(
            1.0,
            1.0
            - min(0.4, duplicate_text_ratio * 0.9)
            - min(0.25, image_coverage_ratio * 0.3)
            - (0.2 if total_chars > 0 and avg_line_length <= 18 else 0.0),
        ),
    )
    hidden_text_layer_suspected = (
        total_chars > 0
        and total_chars <= 120
        and image_coverage_ratio >= 0.70
        and image_object_count >= 1
    )
    duplicate_text_suspected = duplicate_text_ratio >= 0.22
    mixed_text_image_suspected = (
        total_chars >= 150
        and image_object_count >= 1
        and image_coverage_ratio >= 0.18
        and image_coverage_ratio <= 0.88
    )
    full_page_raster_suspected = (
        image_object_count >= 1 and image_coverage_ratio >= 0.82 and total_chars <= 240
    )
    return {
        "image_object_count": int(image_object_count),
        "image_coverage_ratio": float(image_coverage_ratio),
        "duplicate_text_ratio": duplicate_text_ratio,
        "block_coherence": block_coherence,
        "coordinate_sanity": coordinate_sanity,
        "reading_order_stability": reading_order_stability,
        "hidden_text_layer_suspected": hidden_text_layer_suspected,
        "duplicate_text_suspected": duplicate_text_suspected,
        "mixed_text_image_suspected": mixed_text_image_suspected,
        "full_page_raster_suspected": full_page_raster_suspected,
    }


def _page_features(text: str, page_index: int, image_object_count: int, image_coverage_ratio: float) -> dict:
    total_chars = len(text)
    total_ws = sum(1 for c in text if c.isspace())
    total_garbage = len(GARBAGE_RE.findall(text))
    total_punct = sum(1 for c in text if not c.isalnum() and not c.isspace())
    total_digits = sum(1 for c in text if c.isdigit())
    total_non_latin = len(NON_LATIN_RE.findall(text))
    tokens = TOKEN_RE.findall(text)
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    alpha_tokens = sum(1 for token in tokens if any(ch.isalpha() for ch in token))
    alpha_chars = sum(1 for c in text if c.isalpha())
    upper_chars = sum(1 for c in text if c.isupper())
    short_lines = sum(1 for line in lines if len(line) <= 24)
    hyphenated_lines = sum(1 for line in lines if line.endswith("-"))
    quality = _page_quality_features(text, image_object_count, image_coverage_ratio)
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
        "alpha_char_ratio": float(alpha_chars / max(1, total_chars)),
        "uppercase_char_ratio": float(upper_chars / max(1, total_chars)),
        "alpha_token_ratio": float(alpha_tokens / max(1, len(tokens))),
        "avg_token_length": float(sum(len(token) for token in tokens) / max(1, len(tokens))),
        "short_line_ratio": float(short_lines / max(1, len(lines))),
        "repeated_line_ratio": quality["duplicate_text_ratio"],
        "hyphenated_line_ratio": float(hyphenated_lines / max(1, len(lines))),
        "image_object_count": quality["image_object_count"],
        "image_coverage_ratio": quality["image_coverage_ratio"],
        "duplicate_text_ratio": quality["duplicate_text_ratio"],
        "block_coherence": quality["block_coherence"],
        "coordinate_sanity": quality["coordinate_sanity"],
        "reading_order_stability": quality["reading_order_stability"],
        "hidden_text_layer_suspected": quality["hidden_text_layer_suspected"],
        "duplicate_text_suspected": quality["duplicate_text_suspected"],
        "mixed_text_image_suspected": quality["mixed_text_image_suspected"],
        "full_page_raster_suspected": quality["full_page_raster_suspected"],
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
            pdf_page = reader.pages[i]
            txt = pdf_page.extract_text() or ""
            image_object_count = _count_pdf_images(pdf_page)
        else:
            page = doc[i]
            text_page = page.get_textpage()
            txt = text_page.get_text_range() or ""
            text_page.close()
            image_object_count = 0
        image_coverage_ratio = 0.0
        if doc is not None:
            page = doc[i]
            image_coverage_ratio = _safe_float(_estimate_render_coverage(page), 0.0)
            page.close()
        total_chars += len(txt)
        total_ws += sum(1 for c in txt if c.isspace())
        total_garbage += len(GARBAGE_RE.findall(txt))
        page_stats.append(_page_features(txt, i, image_object_count, image_coverage_ratio))

    avg = int(total_chars / max(1, len(idxs)))
    garbage_ratio = float(total_garbage / max(1, total_chars))
    whitespace_ratio = float(total_ws / max(1, total_chars))
    text_pages = sum(1 for page in page_stats if page["char_count"] > 0)
    empty_pages = sum(1 for page in page_stats if page["char_count"] == 0)
    sparse_pages = sum(1 for page in page_stats if 0 < page["char_count"] < 120)
    noisy_pages = sum(1 for page in page_stats if page["garbage_ratio"] >= 0.03)
    image_pages = sum(
        1
        for page in page_stats
        if page["image_object_count"] > 0 or page["image_coverage_ratio"] >= 0.15
    )
    mixed_text_image_pages = sum(1 for page in page_stats if page["mixed_text_image_suspected"])
    full_page_raster_pages = sum(1 for page in page_stats if page["full_page_raster_suspected"])
    hidden_text_layer_pages = sum(1 for page in page_stats if page["hidden_text_layer_suspected"])
    duplicate_text_pages = sum(1 for page in page_stats if page["duplicate_text_suspected"])

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
        image_page_ratio=float(image_pages / denom),
        mixed_text_image_page_ratio=float(mixed_text_image_pages / denom),
        full_page_raster_page_ratio=float(full_page_raster_pages / denom),
        hidden_text_layer_page_ratio=float(hidden_text_layer_pages / denom),
        duplicate_text_page_ratio=float(duplicate_text_pages / denom),
        pages=page_stats,
    )
    print(json.dumps(out))
    if doc is not None:
        doc.close()


if __name__ == "__main__":
    main()
