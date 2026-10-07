#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "$root/dist"
rm -f "$root/dist/ap_go2.apworld"
cd "$root/apworld"
zip -qr "$root/dist/ap_go2.apworld" ap_go2 -x '*/__pycache__/*' '*.pyc'
echo "built dist/ap_go2.apworld"
