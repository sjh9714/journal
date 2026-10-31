#!/usr/bin/env bash
# Korean sample day for native QA of capture and memory, dated today in KST.
# Usage: scripts/qa/fixtures.sh "$QA_DIR", then qa_launch. Needs Pillow and target/debug/dayfile.
set -euo pipefail
Q="$1"
[[ ! -e "$Q/Journal" ]] || { echo "$Q already has a journal; run qa_reset first" >&2; exit 1; }
REPO="$(git rev-parse --show-toplevel)"
export TZ=Asia/Seoul DAYFILE_INDEX_DIR="$Q/idx" DAYFILE_CONFIG="$Q/config.json"
DAY=$(date +%F)
mkdir -p "$Q/code/shop-api" "$Q/photos"

(
  cd "$Q/code/shop-api"
  gc() { GIT_AUTHOR_DATE="$1" GIT_COMMITTER_DATE="$1" git -c user.name=QA -c user.email=qa@example.com -c commit.gpgsign=false "${@:2}"; }
  git init -q
  for t in "10:12 주문 API 추가" "14:05 환불 금액 반올림 버그 수정" "16:40 환불 테스트 보강"; do
    echo "$t" > f.txt
    gc "${DAY}T${t:0:5}:00+09:00" add .
    gc "${DAY}T${t:0:5}:00+09:00" commit -qm "${t:6}"
  done
)

D=${DAY//-/}
printf 'BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:q1\nSUMMARY:스탠드업\nDTSTART;TZID=Asia/Seoul:%sT093000\nDTEND;TZID=Asia/Seoul:%sT094500\nEND:VEVENT\nBEGIN:VEVENT\nUID:q2\nSUMMARY:알고리즘 스터디 (민지)\nDTSTART;TZID=Asia/Seoul:%sT190000\nDTEND;TZID=Asia/Seoul:%sT210000\nEND:VEVENT\nEND:VCALENDAR\n' "$D" "$D" "$D" "$D" > "$Q/cal.ics"

python3 - "$Q/photos/IMG_0001.jpg" "${DAY//-/:} 18:20:00" <<'EOF'
import sys, struct, io
from PIL import Image
buf = io.BytesIO(); Image.new("RGB", (480, 320), (240, 170, 80)).save(buf, "JPEG"); jpg = buf.getvalue()
tiff = b"II*\0\x08\0\0\0" + bytes([1,0,0x69,0x87,4,0,1,0,0,0,26,0,0,0,0,0,0,0]) + bytes([1,0,0x03,0x90,2,0,20,0,0,0,44,0,0,0,0,0,0,0]) + sys.argv[2].encode() + b"\0"
app1 = b"\xff\xe1" + struct.pack(">H", 2 + 6 + len(tiff)) + b"Exif\0\0" + tiff
open(sys.argv[1], "wb").write(jpg[:2] + app1 + jpg[2:])
EOF

python3 -c "import json,sys; q=sys.argv[1]; json.dump({'capture': {'git': [q+'/code'], 'git_author': 'qa@example.com', 'calendars': [q+'/cal.ics'], 'photos': [q+'/photos']}}, open(q+'/config.json','w'))" "$Q"

J=("$REPO/target/debug/dayfile" --root "$Q/Journal")
"${J[@]}" init >/dev/null
"${J[@]}" add "어제는 비가 왔다" --at "$(date -v-1d +%F)T21:00:00+09:00" >/dev/null
src=$("${J[@]}" list | awk '{print $2}' | head -1)
old=$("${J[@]}" remember 집 "부산에 산다" --since 2019-01-01 | awk '{print $1}')
"${J[@]}" remember 집 "서울 마포구에 산다" --since 2024-03-01 --replaces "$old" --source "$src" >/dev/null
"${J[@]}" remember 회사 "커머스 스타트업 백엔드 인턴" --since 2026-07-01 >/dev/null
"${J[@]}" remember 사람 "민지는 알고리즘 스터디 파트너" >/dev/null
