#!/usr/bin/env bash
set -euo pipefail
# Use the package indexes already refreshed by Playwright --with-deps.
# The official distribution package is test-only; no font enters the web/PWA.
if ! dpkg-query -W -f='${Status}' fonts-noto-cjk 2>/dev/null | grep -qx 'install ok installed'; then
  timeout --signal=TERM --kill-after=5s 60s sudo -n env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    -o Acquire::Retries=0 -o Acquire::http::Timeout=10 -o Acquire::https::Timeout=10 \
    -o DPkg::Lock::Timeout=15 fonts-noto-cjk
fi
dpkg-query -W -f='${Package} ${Version} ${Status}\n' fonts-noto-cjk
fc-match -f '%{family}\n' 'Noto Sans CJK SC:lang=zh-cn' | grep -Fx 'Noto Sans CJK SC'
