# `changeset-review`'s shipped surface, captured before the union replaced the compiler

`list-inputs.txt` is `rhei instantiate changeset-review --list-inputs`, byte for
byte, as the block compiler produced it. `ticket-ids.txt` and `artifact-paths.txt`
are what one instantiation with default inputs produced.

These three files are the non-breaking promise of
[§FS-rhei-library.1](../../../../docs/functional-spec/rhei-library.spec.md#1-composition-by-graph-union):
re-authoring `changeset-review` as a template that includes two others changes
what its machine looks like, and must change none of this. They were captured
while the compiler was still live, because "today's bytes" reconstructed out of
git after the fact is a worse thing to argue from.
