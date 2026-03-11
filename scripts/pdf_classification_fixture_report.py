#!/usr/bin/env python3
import argparse
import sys
from pathlib import Path


def load_fixture_data(path: Path) -> dict:
    if sys.version_info >= (3, 11):
        import tomllib
    else:
        raise SystemExit("python 3.11+ is required for tomllib")
    with path.open("rb") as handle:
        return tomllib.load(handle)


def main() -> None:
    parser = argparse.ArgumentParser(description="Render a markdown summary for PDF classification fixtures.")
    parser.add_argument(
        "fixture_file",
        nargs="?",
        default="tests/fixtures/pdf-classification-fixtures.toml",
        help="Path to the fixture TOML file",
    )
    args = parser.parse_args()
    fixture_path = Path(args.fixture_file)
    data = load_fixture_data(fixture_path)
    fixtures = data.get("fixtures", [])

    print("# PDF Classification Fixture Matrix")
    print()
    print(f"Source: `{fixture_path}`")
    print()
    print("| Fixture | Label | Document Class | OCR | Highlight | Search |")
    print("| --- | --- | --- | --- | --- | --- |")
    for fixture in fixtures:
        print(
            f"| `{fixture['id']}` | {fixture['label']} | `{fixture['document_class']}` | "
            f"`{fixture['ocr_recommendation']}` | `{fixture['sentence_highlight_policy']}` | "
            f"`{fixture['search_policy']}` |"
        )


if __name__ == "__main__":
    main()
