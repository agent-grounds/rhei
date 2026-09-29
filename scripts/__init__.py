# `scripts/` is a package only so that `python -m unittest discover -s
# scripts/tests -t .` can name its test modules from the repository root
# (§FS-rhei-distribution.6). Nothing imports it; every script here is still run
# by path.
