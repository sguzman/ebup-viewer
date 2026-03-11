# PDF Classification Fixture Matrix

This matrix is backed by [pdf-classification-fixtures.toml](/win/linux/Code/projects/lantern-leaf/tests/fixtures/pdf-classification-fixtures.toml) and the classifier regression test in [source_pipeline.rs](/win/linux/Code/projects/lantern-leaf/src/epub_loader/source_pipeline.rs).

Covered fixture families:

- `publisher-clean`: high-quality publisher PDFs
- `malformed-embedded`: malformed embedded-text PDFs
- `sparse-presentation`: sparse-text presentation PDFs
- `academic-layout-hostile`: academic PDFs with figures, tables, and footnotes
- `scanned-book`: scanned books
- `photocopy-image-only`: photocopies / image-only PDFs
- `ocr-overlay`: hidden OCR overlay PDFs
- `hybrid-mixed`: hybrid documents with mixed page classes

The quick summary can be regenerated with:

```bash
python3 scripts/pdf_classification_fixture_report.py
```
