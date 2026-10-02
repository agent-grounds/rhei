# Panta: Decide the merge policy, then apply it

The merge queue needs a policy: squash, merge commits, or something between.
The project holds two rheis. `discussion` is the discussion, on its own
machine: two participants argue it over rounds until a judge rules. `ticket`
is the work that applies the ruling, on the project's default machine, and
its task consumes the ruling across rheis.
