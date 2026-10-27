#!/usr/bin/env bash
# WSL 上のソースを Windows 側のビルド用ディレクトリへ同期する。
# Windows のビルドツール（cargo / MSVC / WebView2）は UNC パス上では遅く不安定なため。
#   使い方: scripts/sync-to-windows.sh [Windows側のディレクトリ（WSLのパス）]
set -euo pipefail
SRC="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${1:-/mnt/c/dev/fr_sch_widget}"
mkdir -p "$DEST"
# -c で中身を比べ、--no-times で更新日時をコピーしない（cargo は更新日時で再ビルドを判断するため、
# 古い日時に戻すと変更が無視されることがある）
rsync -rlc --no-times --delete \
  --exclude node_modules --exclude src-tauri/target --exclude src-tauri/gen \
  --exclude .git \
  "$SRC/" "$DEST/"
echo "synced: $SRC -> $DEST"
