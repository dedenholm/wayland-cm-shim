# Methodology (draft)

Two instances of darktable, one using the monitor .icc in darktable and KDE Plasmas Color profile set to none, the other with KDE plasma color profile set to the monitor .icc. A set of 21 TIFFs with solid colors was opened in sequence in each instance and measured on screen with a Calibrite Display Pro HL colorimeter.

Readings were taken manually with ArgyllCMS `spotread`, writing to its logfile output — one run per pipeline, same patch order in both:

```
spotread [flags] darktable_selfmanaged_kwin_no_icc.log
spotread [flags] darktable_under_cm-shim_kwin_icc.log
```

Each log was converted to a CGATS `.ti3` file by a python script, 'spotlog2ti3.py', that extracts the XYZ columns and assigns SAMPLE_ID 1..N in measurement order, applying no colour transformations.

Per-patch differences were computed with ArgyllCMS `colverify`:

```
colverify -k -v 2 darktable_selfmanaged_kwin_no_icc.log darktable_under_cm-shim_kwin_icc.log 
 
```

`-k` selects CIEDE2000, `-v 2` prints each patch. Patches are matched by SAMPLE_ID, so patch *n* of run 1 is compared against patch *n* of run 2.
