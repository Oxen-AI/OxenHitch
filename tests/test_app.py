"""Verify packaged Windows frontend URL contracts.

| Stage | Mechanism | Tag | Ref |
| --- | --- | --- | --- |
| INPUT | Create representative frontend paths | [mandatory] | R1 |
| URI | Build and inspect file-safe Python desktop URLs | [mandatory] | R1 |
| ASSERT | Confirm paths survive URI parsing unchanged | [mandatory] | R1 |

EARS guards: R1 WHEN a packaged frontend URL is built, THEN its marker is a fragment and never part of the local path.
CLOSED: query-string desktop markers are rejected for file URLs.
Preconditions: Python and hitch.app dependencies are installed.
Failure modes: an absent frontend raises FileNotFoundError.
"""

from pathlib import Path
from urllib.parse import unquote, urlsplit

import pytest

from hitch.app import frontend_url


@pytest.mark.parametrize("name", ["index.html", "folder with spaces/index.html", "café/index.html"])
def test_frontend_url_keeps_desktop_marker_out_of_local_path(tmp_path: Path, name: str) -> None:
    """Keep file URI path resolution independent of the desktop marker.

    Require: tmp_path can hold a frontend file at name.
    Guarantee: URL uses the file path and a separate desktop fragment.
    Maintain: spaces and Unicode path components remain encoded in the URI.
    Assert: no query component is created.
    """
    frontend = tmp_path.joinpath(*name.split("/"))
    frontend.parent.mkdir(parents=True, exist_ok=True)
    frontend.write_text("<!doctype html>", encoding="utf-8")

    parsed = urlsplit(frontend_url(frontend))
    assert parsed.scheme == "file"
    assert unquote(parsed.path).endswith(name)
    assert parsed.query == ""
    assert parsed.fragment == "desktop=python"


def test_frontend_url_rejects_missing_file(tmp_path: Path) -> None:
    """Fail before creating a WebView navigation to an absent frontend.

    Require: tmp_path has no index.html.
    Guarantee: FileNotFoundError identifies the missing path.
    Maintain: no file is created.
    Assert: invalid package contents cannot produce a launch URL.
    """
    with pytest.raises(FileNotFoundError):
        frontend_url(tmp_path / "index.html")
