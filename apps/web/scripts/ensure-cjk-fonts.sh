#!/usr/bin/env bash
set -euo pipefail
# Playwright's pinned Ubuntu dependency recipe already installs this official
# CJK sans package. Verify it; do not add another package/network transaction.
# No font is bundled into the application or its PWA assets.
dpkg-query -W -f='${Status}' fonts-wqy-zenhei | grep -qx 'install ok installed'
dpkg-query -W -f='${Package} ${Version} ${Status}\n' fonts-wqy-zenhei
fc-match -f '%{family}\n' 'WenQuanYi Zen Hei:lang=zh-cn' | grep -E '^WenQuanYi Zen Hei(,|$)'
