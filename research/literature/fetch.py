"""Download every source in `sources.json` and convert it to Markdown with docling.

Each PDF goes to `cache/<id>.pdf`, its Markdown to `cache/<id>.md` (one `<!-- page N -->`
marker per page) and the same pages to `cache/<id>.pages.json`, which `verify.py` reads.
`cache/` is git-ignored: papers are not redistributed, only identified by URL and checksum.

Only https is fetched, a download is capped in size and lands under its final name only
after its checksum is checked, and a file that no longer matches is deleted. A source with
no `sha256` gets one recorded on its first download — review it before committing.
"""

import hashlib
import json
import sys
import time
import urllib.request
from pathlib import Path

from verify import check_source

HERE = Path(__file__).parent
CACHE = HERE / "cache"
SOURCES = HERE / "sources.json"

# arXiv asks automated clients for at most one request every three seconds.
POLITE_DELAY = 3.0
MAX_BYTES = 100 * 2**20


def download(url: str, opener) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "lotml-research/1.0"})
    with opener(request, timeout=120) as response:
        data = response.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise OSError(f"larger than {MAX_BYTES} bytes")
    return data


def fetch_one(source: dict, opener=urllib.request.urlopen) -> str | None:
    """Put the source's checked PDF in the cache; the reason it failed, or None."""
    check_source(source)
    pdf = CACHE / f"{source['id']}.pdf"
    if pdf.exists():
        data = pdf.read_bytes()
    else:
        print(f"downloading {source['id']}", flush=True)
        try:
            data = download(source["url"], opener)
        except OSError as error:
            return f"{source['id']}: {error}"
    digest = hashlib.sha256(data).hexdigest()
    if "sha256" not in source:
        print(f"recorded the checksum of {source['id']}: review it", flush=True)
        source["sha256"] = digest
    if source["sha256"] != digest:
        pdf.unlink(missing_ok=True)
        return f"{source['id']}: checksum changed"
    if not pdf.exists():
        part = pdf.with_suffix(".part")
        part.write_bytes(data)
        part.replace(pdf)
    return None


def converter():
    """A PDF converter without OCR: every source is born-digital."""
    from docling.datamodel.base_models import InputFormat
    from docling.datamodel.pipeline_options import PdfPipelineOptions
    from docling.document_converter import DocumentConverter, PdfFormatOption

    options = PdfPipelineOptions(do_ocr=False, do_table_structure=True)
    return DocumentConverter(
        format_options={InputFormat.PDF: PdfFormatOption(pipeline_options=options)}
    )


def markdown_pages(convert, path: Path) -> list[str]:
    document = convert.convert(path).document
    return [
        document.export_to_markdown(page_no=number)
        for number in range(1, document.num_pages() + 1)
    ]


def main() -> int:
    sources = json.loads(SOURCES.read_text("utf-8"))
    CACHE.mkdir(exist_ok=True)
    convert = None
    failed = []
    for source in sources:
        downloaded = not (CACHE / f"{source['id']}.pdf").exists()
        error = fetch_one(source)
        if downloaded:
            time.sleep(POLITE_DELAY)
        if error is not None:
            failed.append(error)
            continue
        pages_path = CACHE / f"{source['id']}.pages.json"
        if pages_path.exists():
            continue
        print(f"converting {source['id']}", flush=True)
        convert = convert or converter()
        pages = markdown_pages(convert, CACHE / f"{source['id']}.pdf")
        pages_path.write_text(json.dumps(pages, ensure_ascii=False), "utf-8")
        (CACHE / f"{source['id']}.md").write_text(
            "\n\n".join(
                f"<!-- page {number} -->\n\n{page}"
                for number, page in enumerate(pages, 1)
            ),
            "utf-8",
        )
    SOURCES.write_text(
        json.dumps(sources, indent=2, ensure_ascii=False) + "\n", "utf-8"
    )
    for line in failed:
        print(f"failed: {line}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
