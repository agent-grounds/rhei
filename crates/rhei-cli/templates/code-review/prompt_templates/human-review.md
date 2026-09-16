Use the current state `{state}` and states.yaml to identify this block's
states. Local state names in these instructions mean the state with the
same encoded prefix as the current state; identity wrappers have no prefix.
For completion, use the current state's exact outgoing completion edge.
Resolve every runtime file from its producing state's artifact declaration,
substituting that producer task's actual id and target. Paths written below
as runtime/... describe local artifact roles; the generated declarations
give the actual mounted paths. Keep all artifacts in this scratchpad.

Review the final fix decision for Task {task_id}: {task_title}.

Read `{input.final-decision.path}` and decide whether the accepted fixes
should be applied. Do not transition out of this state autonomously.
A human must use the outgoing approval transition shown in states.yaml
to approve the fix phase, or the cancellation transition to stop it.
