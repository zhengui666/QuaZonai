# Reuse the browser's installed CJK font

PR161's [data job](https://github.com/zhengui666/QuaZonai/actions/runs/36925004265/job/110580052926)
installed `fonts-wqy-zenhei` successfully as part of Playwright's dependencies.
The subsequent extra `fonts-noto-cjk` download was 61.2 MB and reached the
60-second bound with exit 124 before the actual data/browser acceptance ran.
The source was `be35dbb59f872b4d661736dc04cbfa96fa75c078`.

The pinned Playwright 1.63.0 Ubuntu dependency recipe includes
`fonts-wqy-zenhei`. Reuse this existing official distribution package and verify
its installed state and exact fontconfig family, with no extra installation,
network call or elevated command. Missing dependencies and serif/unknown
fallbacks remain failures. The application CSS and Ant Design font stack both
prefer that sans family on Linux; Apple/Windows families and Noto fallback remain.

The actual native Chromium CDP check requires `WenQuanYi Zen Hei` on Linux and
records the observed glyph-font metadata. It does not accept any family merely
because a CSS string names it. All existing layout, theme, accessibility,
database, Worker/OCI and recovery assertions remain. No font asset is shipped
in the web/PWA or application image. This removes the observed additional
61.2 MB transaction; it does not claim Playwright's own dependency installation
or the complete PR takes five minutes.

Four isolated shell regressions cover the installed prerequisite, missing
package without a download, serif substitution and similar-name rejection.
The required final-head hosted font rendering, browser layouts and data
acceptance remain unrun until this correction is published and checked.
