#!/usr/bin/env sh
# Downloads Apache Tika's OneNote test documents (Apache-2.0 sample DATA files, not code) into the
# gitignored `corpus/` folder at the repository root. The real-file tests in notecraft-one read
# them when the folder exists and skip otherwise.
#
# Only the sample files are fetched; NoteCraft never reads or uses Tika's parser source
# (AGENTS.md, Clean room). Pinned to a Tika commit so the corpus doesn't drift.
set -eu

TIKA_REV="${TIKA_REV:-9c2a853f94ba3e439c1eb4b11e473fa65d1f5dc1}"
BASE="https://raw.githubusercontent.com/apache/tika/${TIKA_REV}/tika-parsers/tika-parsers-standard/tika-parsers-standard-modules/tika-parser-microsoft-module/src/test/resources/test-documents"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${ROOT}/corpus/tika"
mkdir -p "${OUT}"

FILES="
testOneNote.one
testOneNote1.one
testOneNote2.one
testOneNote3.one
testOneNote4.one
testOneNote2016.one
testOneNoteEmbeddedImage.one
testOneNoteEmbeddedWordDoc.one
testOneNoteTablesLists.one
testOneNoteFromOffice365.one
testOneNoteFromOffice365-2.one
testOneNote2007OrEarlier.one
testOneNote-fuzz1.one
testOneNote-fuzz2.one
testOneNote-fuzz3.one
test-tika-4303-Chinese-notes.one
test-tika-3970-dupetext.one
"

for f in ${FILES}; do
  if [ -s "${OUT}/${f}" ]; then
    continue
  fi
  echo "fetching ${f}"
  curl -fsSL --retry 3 -o "${OUT}/${f}.part" "${BASE}/${f}"
  mv "${OUT}/${f}.part" "${OUT}/${f}"
done

cat > "${OUT}/README.txt" <<'TXT'
OneNote sample files from the Apache Tika project (https://github.com/apache/tika),
licensed under the Apache License, Version 2.0. Test inputs only; not redistributed.
TXT
echo "corpus ready in ${OUT}"
