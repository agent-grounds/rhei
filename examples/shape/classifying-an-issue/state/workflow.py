"""Mock agent and labelling program for the classifying-an-issue shape pair.

Stands in for a real coding agent, and for the program that labels an issue,
so both shapes run with no credentials and touch no forge: labelling writes
the label into `forge/issue-87.md`, the stand-in forge, and the kind into
its task's `kind` export. The work is keyed by task title, not id.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import os
import pathlib
import re
import sys

RESULTS = {
    'Triage issue 87':
        'Issue 87 is real: the parser panics on a valid 64-bit literal.',
    'Route the issue':
        'Routed issue 87 to the parser\'s bug queue.',
}

STATE_OUTPUTS = {
    'classify': {'runtime/classify/{task}.md': 'bug\n'},
}


def kind():
    task = os.environ['RHEI_TASK_ID']
    return (pathlib.Path('runtime') / 'classify' / (task + '.md')).read_text(
        encoding='utf-8').strip()


def labelled():
    """Put the kind on the stand-in forge's issue as its label."""
    issue = pathlib.Path('forge') / 'issue-87.md'
    text = issue.read_text(encoding='utf-8').replace('labels: none', 'labels: ' + kind())
    write(issue, text)
    return {'kind': kind() + '\n'}


PROGRAM_EXPORTS = {
    ('labelling', 'Triage issue 87'): labelled,
    ('labelling', 'Classify the issue'): labelled,
}
PROGRAM_RESULTS = {
    ('labelling', 'Triage issue 87'):
        lambda: 'Issue 87 is a real %s: the parser panics on a valid 64-bit literal; labelled `%s`.'
        % (kind(), kind()),
    ('labelling', 'Classify the issue'): lambda: 'Labelled issue 87 `%s`.' % kind(),
}

EXPORTS = {}

PROGRAM_WRITES = {}


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

for name, text in EXPORTS.get(title, {}).items():
    write(root / 'runtime' / 'exports' / task / (name + '.md'), text)
for pattern, text in STATE_OUTPUTS.get(os.environ.get('RHEI_STATE'), {}).items():
    write(root / pattern.format(task=task), text)

if result_match:
    path = pathlib.Path(result_match.group(1))
    summary = RESULTS.get(title, 'Did %s.' % title)
    write(path if path.is_absolute() else root / path, summary + '\n')
