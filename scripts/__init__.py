"""A package only so unittest discovery can name the test modules.

`python -m unittest discover -s scripts/tests -t .` imports them as
`scripts.tests.*` from the repository root (§FS-rhei-distribution.6). Nothing
imports this package; every script here is still run by path.
"""
