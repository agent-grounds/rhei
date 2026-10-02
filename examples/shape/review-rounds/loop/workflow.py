"""Mock agent for the review-rounds shape pair.

Stands in for a real reviewer and a real coding agent so both shapes run with
no credentials. The work is keyed by task title, not id. The loop shape's
review task writes one set of notes per review pass to its `review-notes`
handoff, which only the next fix pass reads, and its result once the second
fix pass is done. The tasks shape writes each round's finding as that round's
result.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import os
import pathlib
import re
import sys

RESULTS = {
    'Implement the streaming export':
        'The export streams its rows; a first cut, ready for review.',
    'Review and fix the export':
        'Reviewed and fixed in two rounds: the writer flushes every 64 KB and writes the header once.',
    'Review round 1':
        'Round 1: the writer flushes a whole page, not every 64 KB.',
    'Fix round 1':
        'Fixed round 1: the writer flushes every 64 KB.',
    'Review round 2':
        'Round 2: the header is written again after every flush.',
    'Fix round 2':
        'Fixed round 2: the header is written once.',
    'Ship the export':
        'Shipped: the streaming export is merged.',
}

# The loop's handoff: each review pass's notes, read by the next fix pass alone.
STATE_OUTPUTS = {
    'review': {'runtime/reviews/task-{task}-review.md':
               'The writer flushes a whole page; the header repeats after a flush.\n'},
}

EXPORTS = {}

PROGRAM_WRITES = {}

PROGRAM_EXPORTS = {}

PROGRAM_RESULTS = {}


def write(path, text):
    target = pathlib.Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open('w', encoding='utf-8', newline='') as handle:
        handle.write(text)


def title_of(local):
    """A program has no prompt: its task's title is read from the plan."""
    plan = pathlib.Path(os.environ['RHEI_PLAN_PATH'])
    root = plan if plan.is_dir() else plan.parent
    for path in sorted(root.glob('tasks/*.md')):
        found = re.search(r'^#+ Task %s: (.+)$' % re.escape(local),
                          path.read_text(encoding='utf-8'), re.MULTILINE)
        if found:
            return found.group(1).strip()
    return local


def program(step):
    """A program state: it writes what its step makes, keyed by the task."""
    task = os.environ['RHEI_TASK_ID']
    title = title_of(os.environ['RHEI_TASK_ID_LOCAL'])
    for path, text in PROGRAM_WRITES.get(step, {}).items():
        write(path, text)
    for name, text in PROGRAM_EXPORTS.get((step, title), {}).items():
        write(pathlib.Path('runtime') / 'exports' / task / (name + '.md'), text)
    if (step, title) in PROGRAM_RESULTS:
        write(os.environ['RHEI_RESULT_PATH'], PROGRAM_RESULTS[(step, title)] + '\n')


def prompt_arg():
    """The authoritative autonomous context delivered by `prompt_flag`."""
    args = sys.argv[1:]
    for index, arg in enumerate(args[:-1]):
        if arg == '--prompt':
            return args[index + 1]
    return ''


if sys.argv[1:2] == ['--program']:
    program(sys.argv[2])
    sys.exit(0)

prompt = prompt_arg()
root_match = re.search(r'^- This rhei: `([^`]+)`', prompt, re.MULTILINE)
task_match = re.search(r'^#+ Task ([^:]+): (.+)$', prompt, re.MULTILINE)
if not root_match or not task_match:
    sys.exit('the agent prompt is missing its Rhei execution root or task')

root = pathlib.Path(root_match.group(1))
task, title = task_match.group(1), task_match.group(2).strip()
result_section = prompt.split('\n## Result\n', 1)
result_match = None
if len(result_section) == 2:
    result_match = re.search(r'^- `([^`]+)`$', result_section[1], re.MULTILINE)

for name, text in EXPORTS.get(title, {}).items():
    write(root / 'runtime' / 'exports' / task / (name + '.md'), text)
for pattern, text in STATE_OUTPUTS.get(os.environ.get('RHEI_STATE'), {}).items():
    write(root / pattern.format(task=task), text)

if result_match:
    path = pathlib.Path(result_match.group(1))
    summary = RESULTS.get(title, 'Did %s.' % title)
    write(path if path.is_absolute() else root / path, summary + '\n')
