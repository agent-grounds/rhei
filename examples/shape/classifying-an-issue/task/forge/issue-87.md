# Issue 87: the parser panics on a 64-bit literal

author: mira
labels: none

`let x = 18446744073709551616;` panics the parser instead of giving a
diagnostic.
