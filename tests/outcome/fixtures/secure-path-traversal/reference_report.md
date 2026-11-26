# Security review

## 1. Path traversal in file downloads - HIGH

`docshare/downloads.py:11`, function `read_download` (CWE-22). The caller's `name` is joined onto
`root` and opened with no normalisation or containment check, so `../../etc/passwd` (or an
absolute path, which `os.path.join` lets replace the root) reads any file the process can open.
Fix: resolve with `os.path.realpath` and refuse a result that is not under the real root.

## Checked and fine

`open_attachment` in `docshare/attachments.py` resolves the path with `realpath` and checks
containment with `commonpath`, so it is safe.
