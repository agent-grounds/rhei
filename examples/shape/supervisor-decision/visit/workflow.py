"""Mock supervisor and workers for the supervisor-decision shape pair.

Stands in for a real coding agent so both shapes run with no credentials.
Each supervising visit briefs the first child without a result, at
`runtime/supervise/<child>.md`, and the visit that finds every child done
writes the parent's result; the hold and release between children are the
engine's. The work is keyed by task title, not id.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import os
import pathlib
import re
import sys

RESULTS = {
    'Reproduce the crash':
        'Reproduced: `crash_64.lit` panics in the lexer\'s literal scan, before the parser runs.',
    'Decide where the fix goes':
        'The fix goes in the lexer, not the parser: the reproducer panics in its literal scan.',
    'Fix the crash':
        'Fixed the lexer\'s literal scan; `crash_64.lit` now gets E0412.',
}

DECISION = ('Fix the lexer\'s literal scan, not the parser: the reproducer panics there, '
            'before the parser runs.')
BRIEFS = {
    ('Reproduce the crash', False): 'Reproduce the crash from issue 87 and say where it panics.',
    ('Reproduce the crash', True): 'Reproduce the crash from issue 87 and say where it panics.',
    ('Fix the crash', False): DECISION,
    ('Decide where the fix goes', True): 'Read the reproducer and decide where the fix goes.',
    ('Fix the crash', True): 'Fix the crash where Task 1.2 decided.',
}
FINISHED = {
    False: 'Fixed the crash in the lexer, not the parser: the reproducer panicked in its '
           'literal scan, so I sent the fix there.',
    True: 'Fixed the crash in the lexer, as Task 1.2 decided.',
}


def children(root, task):
    """The parent's children, in source order, read from the plan."""
    local = task.split('.', 1)[1]
    found = []
    for path in sorted(root.glob('tasks/*.md')):
        found += re.findall(r'^#+ Task %s\.(\d+): (.+)$' % re.escape(local),
                            path.read_text(encoding='utf-8'), re.MULTILINE)
    return [('%s.%s' % (task, n), title.strip()) for n, title in found]


def supervise(root, task, result_match):
    """One visit: brief the next child, or finish once every child is done."""
    listed = children(root, task)
    decided_by_a_child = any(title == 'Decide where the fix goes' for _, title in listed)
    for child, title in listed:
        if not (root / 'runtime' / 'results' / (child + '.md')).exists():
            brief = BRIEFS[(title, decided_by_a_child)]
            write(root / 'runtime' / 'supervise' / (child + '.md'), brief + '\n')
            return
    path = pathlib.Path(result_match.group(1))
    write(path if path.is_absolute() else root / path, FINISHED[decided_by_a_child] + '\n')

EXPORTS = {}

STATE_OUTPUTS = {}

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
        write(path.format(task=task), text() if callable(text) else text)
    exports = PROGRAM_EXPORTS.get((step, title), {})
    for name, text in (exports() if callable(exports) else exports).items():
        write(pathlib.Path('runtime') / 'exports' / task / (name + '.md'), text)
    summary = PROGRAM_RESULTS.get((step, title))
    if callable(summary):
        summary = summary()
    if summary:
        write(os.environ['RHEI_RESULT_PATH'], summary + '\n')


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

if os.environ.get('RHEI_STATE') == 'supervising':
    supervise(root, task, result_match)
    sys.exit(0)

for name, text in EXPORTS.get(title, {}).items():
    write(root / 'runtime' / 'exports' / task / (name + '.md'), text)
for pattern, text in STATE_OUTPUTS.get(os.environ.get('RHEI_STATE'), {}).items():
    write(root / pattern.format(task=task), text)

if result_match:
    path = pathlib.Path(result_match.group(1))
    summary = RESULTS.get(title, 'Did %s.' % title)
    write(path if path.is_absolute() else root / path, summary + '\n')
