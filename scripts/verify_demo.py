#!/usr/bin/env python3
"""Regenerate vhs/demo.gif and verify it recorded correctly.

VHS occasionally drops a keypress (go-rod -> xterm.js -> ttyd flakiness), and a
dropped key derails the whole demo. This script:

1. Runs `vhs vhs/demo.tape` (from the repo root).
2. Verifies the tab-bar sequence shows the intended tour:
   Feeds -> Saved -> Settings -> Changelog -> Feeds,
   and that Saved and Settings are revisited later.
3. Repeats until verified (drops are random, so a few attempts usually succeed).

Usage (from repo root):
    python3 scripts/verify_demo.py

Requires: vhs on PATH, ImageMagick (magick), and Pillow.
"""
import glob
import hashlib
import os
import subprocess
import sys
import tempfile

TAPE = 'vhs/demo.tape'
OUT = 'vhs/demo.gif'
MAX_ATTEMPTS = 12


def extract_strips():
    """Extract the tab-bar highlight region of every 12th frame."""
    d = tempfile.mkdtemp(prefix='vhs-verify-')
    subprocess.run(
        ['magick', OUT, '-crop', '400x17+100+17', '+repage', f'{d}/f%04d.png'],
        check=True, capture_output=True,
    )
    return d


def tab_sequence(d):
    """Return the deduped sequence of tab-bar states (as ints)."""
    labels = {}
    order = []
    for f in sorted(glob.glob(os.path.join(d, 'f*.png'))):
        h = hashlib.md5(open(f, 'rb').read()).hexdigest()
        if h not in labels:
            labels[h] = len(labels)
        order.append(labels[h])
    dedup = [order[0]]
    for s in order[1:]:
        if s != dedup[-1]:
            dedup.append(s)
    return dedup


def verify(seq):
    """The tour must be A B C D A, with B and C revisited afterwards."""
    if len(seq) < 7:
        print(f'  too few tab states: {seq}')
        return False
    first5 = seq[:5]
    if len(set(first5)) != 4 or first5[0] != first5[4]:
        print(f'  tab tour broken: {first5}')
        return False
    rest = seq[5:]
    if first5[1] not in rest or first5[2] not in rest:
        print(f'  Saved/Settings not revisited: {rest}')
        return False
    return True


def main() -> int:
    vhs = os.environ.get('VHS', 'vhs')
    for attempt in range(1, MAX_ATTEMPTS + 1):
        print(f'--- attempt {attempt} ---')
        r = subprocess.run([vhs, TAPE], capture_output=True, text=True)
        if r.returncode != 0:
            print('  vhs failed:', r.stderr[-400:])
            continue
        seq = tab_sequence(extract_strips())
        print('  tab-bar sequence:', seq)
        if verify(seq):
            print('  VERIFIED OK')
            return 0
    print('FAILED after', MAX_ATTEMPTS, 'attempts')
    return 1


if __name__ == '__main__':
    sys.exit(main())
