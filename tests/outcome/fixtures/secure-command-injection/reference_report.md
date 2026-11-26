# Security review

## 1. OS command injection in word counting - HIGH

`fileinfo/words.py:7`, function `count_words` (CWE-78). The path is interpolated into a command
string that runs with `shell=True`, so a value such as `x; rm -rf ~` or `$(...)` runs arbitrary
commands as the service user. Fix: pass an argument list, `["wc", "-w", path]`, without a shell.

## Checked and fine

`count_lines` in `fileinfo/lines.py` passes an argument list and no shell, so the path is never
parsed as a command.
