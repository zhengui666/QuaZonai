from __future__ import annotations

from fastapi.testclient import TestClient
from sqlalchemy import Engine

from main import create_app
from settings import Settings


def test_pwa_static_files_have_update_safe_cache_headers(
    engine: Engine,
    settings: Settings,
) -> None:
    assets = settings.frontend_dist / "assets"
    icons = settings.frontend_dist / "icons"
    assets.mkdir(parents=True)
    icons.mkdir()
    (settings.frontend_dist / "index.html").write_text("<main>shell</main>")
    (settings.frontend_dist / "sw.js").write_text("self.skipWaiting()")
    (settings.frontend_dist / "manifest.webmanifest").write_text("{}")
    (assets / "app-AbCd.js").write_text("console.log('app')")
    (icons / "pwa-192.png").write_bytes(b"png")
    client = TestClient(create_app(settings=settings, engine=engine))

    for path in ("/", "/research/123", "/sw.js", "/manifest.webmanifest"):
        response = client.get(path)
        assert response.status_code == 200
        assert response.headers["cache-control"] == "no-cache"
        assert response.headers["content-security-policy"] == "frame-ancestors 'none'"
        assert response.headers["x-frame-options"] == "DENY"

    asset = client.get("/assets/app-AbCd.js")
    assert asset.status_code == 200
    assert asset.headers["cache-control"] == "public, max-age=31536000, immutable"

    icon = client.get("/icons/pwa-192.png")
    assert icon.status_code == 200
    assert "cache-control" not in icon.headers


def test_pwa_static_fallback_never_handles_api_or_path_traversal(
    engine: Engine,
    settings: Settings,
) -> None:
    settings.frontend_dist.mkdir()
    (settings.frontend_dist / "index.html").write_text("<main>shell</main>")
    (settings.frontend_dist.parent / "outside.txt").write_text("outside")
    client = TestClient(create_app(settings=settings, engine=engine))

    api = client.get("/api/not-a-route")
    assert api.status_code == 404
    assert "shell" not in api.text

    # Keep the traversal segment percent-encoded so httpx does not normalize it
    # away before Starlette can exercise the static-serving boundary.
    traversal = client.get("/%2E%2E/outside.txt")
    assert traversal.status_code == 404
    assert "shell" not in traversal.text
