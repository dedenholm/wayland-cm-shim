#!/usr/bin/env python3
"""
spotlog2ti3.py - convert an ArgyllCMS `spotread` logfile into a CGATS/.ti3 file
that `colverify` can read.

spotread writes its logfile as a header row plus one tab-separated row per
reading. This script finds the XYZ columns (or L*a*b* if XYZ is absent),
numbers the patches 1..N as SAMPLE_ID, and writes a minimal .ti3.

Usage:
    spotread -e -y l run1.log          # measure 20 patches, q to quit
    python3 spotlog2ti3.py run1.log -o run1.ti3 --expect 20
    python3 spotlog2ti3.py run2.log -o run2.ti3 --expect 20
    colverify -k -v 2 run1.ti3 run2.ti3

Patch order must be identical in both runs - matching is by position.
"""

import argparse
import os
import re
import sys
import time

# Header names spotread might use, matched case-insensitively against the
# whole cell. Covers "X"/"Y"/"Z", "XYZ_X", "L*"/"a*"/"b*", "LAB_L", etc.
PATTERNS = {
    "X": r"(XYZ[ _]?)?X",
    "Y": r"(XYZ[ _]?)?Y",
    "Z": r"(XYZ[ _]?)?Z",
    "L": r"(LAB[ _]?)?L\*?",
    "A": r"(LAB[ _]?)?A\*?",
    "B": r"(LAB[ _]?)?B\*?",
}


def split_row(line):
    """Split on tabs; fall back to runs of whitespace if there are none."""
    return line.split("\t") if "\t" in line else line.split()


def clean(cell):
    return cell.strip().strip('"').strip()


def find_columns(header):
    """Return {name: index} for every pattern that matches exactly one column."""
    cells = [clean(c).upper() for c in header]
    found = {}
    for name, pat in PATTERNS.items():
        rx = re.compile(r"^" + pat + r"$", re.IGNORECASE)
        hits = [i for i, c in enumerate(cells) if rx.match(c)]
        if len(hits) == 1:
            found[name] = hits[0]
    return found


def read_log(path):
    """Return (header_cells, [data_rows])."""
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        lines = [ln.rstrip("\r\n") for ln in fh if ln.strip()]
    if not lines:
        sys.exit(f"{path}: file is empty")
    return split_row(lines[0]), [split_row(ln) for ln in lines[1:]]


def extract(rows, cols, keys):
    """Pull the named columns out of each row, skipping rows that aren't numeric."""
    idx = [cols[k] for k in keys]
    out, skipped = [], 0
    for row in rows:
        if max(idx) >= len(row):
            skipped += 1
            continue
        try:
            out.append([float(clean(row[i])) for i in idx])
        except ValueError:
            skipped += 1
    return out, skipped


def write_ti3(path, values, fields, color_rep, device_class, source, ids=None):
    n = len(values)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("CTI3\n\n")
        fh.write('DESCRIPTOR "Spot measurements from spotread logfile"\n')
        fh.write('ORIGINATOR "spotlog2ti3"\n')
        fh.write(f'CREATED "{time.strftime("%a %b %d %H:%M:%S %Y")}"\n')
        fh.write(f'KEYWORD "SOURCE_LOG"\nSOURCE_LOG "{os.path.basename(source)}"\n')
        fh.write(f'KEYWORD "DEVICE_CLASS"\nDEVICE_CLASS "{device_class}"\n')
        fh.write(f'KEYWORD "COLOR_REP"\nCOLOR_REP "{color_rep}"\n\n')
        fh.write(f"NUMBER_OF_FIELDS {len(fields) + 1}\n")
        fh.write("BEGIN_DATA_FORMAT\n")
        fh.write("SAMPLE_ID " + " ".join(fields) + "\n")
        fh.write("END_DATA_FORMAT\n\n")
        fh.write(f"NUMBER_OF_SETS {n}\n")
        fh.write("BEGIN_DATA\n")
        for i, vals in enumerate(values):
            sid = ids[i] if ids else str(i + 1)
            fh.write(sid + " " + " ".join(f"{v:.6f}" for v in vals) + "\n")
        fh.write("END_DATA\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("logfile", help="logfile written by spotread")
    ap.add_argument("-o", "--out", help="output .ti3 (default: logfile with .ti3)")
    ap.add_argument("--expect", type=int, metavar="N",
                    help="fail unless exactly N readings are found")
    ap.add_argument("--ids", metavar="FILE",
                    help="file of patch names, one per line, used as SAMPLE_ID")
    ap.add_argument("--device-class", default="DISPLAY",
                    choices=["DISPLAY", "OUTPUT", "INPUT"],
                    help="written to the .ti3 header (default: DISPLAY)")
    args = ap.parse_args()

    out = args.out or os.path.splitext(args.logfile)[0] + ".ti3"
    header, rows = read_log(args.logfile)
    cols = find_columns(header)

    if all(k in cols for k in ("X", "Y", "Z")):
        keys, fields, rep = ["X", "Y", "Z"], ["XYZ_X", "XYZ_Y", "XYZ_Z"], "XYZ"
    elif all(k in cols for k in ("L", "A", "B")):
        keys, fields, rep = ["L", "A", "B"], ["LAB_L", "LAB_A", "LAB_B"], "LAB"
    else:
        sys.exit(
            "Could not find XYZ or L*a*b* columns.\n"
            f"Header was: {[clean(c) for c in header]}\n"
            "Re-run spotread without -x / -h / -u so it logs XYZ and Lab."
        )

    values, skipped = extract(rows, cols, keys)
    if skipped:
        print(f"note: skipped {skipped} non-numeric row(s)", file=sys.stderr)
    if not values:
        sys.exit("No readings found.")
    if args.expect and len(values) != args.expect:
        sys.exit(f"Found {len(values)} readings, expected {args.expect}.")

    ids = None
    if args.ids:
        with open(args.ids, encoding="utf-8") as fh:
            ids = [ln.strip().replace(" ", "_") for ln in fh if ln.strip()]
        if len(ids) != len(values):
            sys.exit(f"{args.ids} has {len(ids)} names but there are {len(values)} readings.")

    write_ti3(out, values, fields, rep, args.device_class, args.logfile, ids)
    print(f"{out}: {len(values)} patches, {rep}")


if __name__ == "__main__":
    main()
