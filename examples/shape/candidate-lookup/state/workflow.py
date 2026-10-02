"""Mock agent and lookup program for the candidate-lookup shape pair.

Stands in for a real agent, and for the program that searches an issue
tracker, so both shapes run with no credentials and touch no forge. The lookup
reads `forge/issues.md`, the stand-in tracker, and lists every issue that
shares a word with the report; the verdict is the agent's. The work is keyed
by task title, not id.

The first line of each result is the one-line summary every later Plan History
shows for the task, so it is written as a sentence a later reader can use.
"""

import os
import pathlib
import re
import sys

REPORT = {'integer', 'literal', 'overflow', 'panics'}

VERDICT = 'Issue 87 is new: #52 is about float literals and #61 was a lexer crash fixed in 0.4.'

RESULTS = {
    'Triage issue 87': VERDICT,
    'Judge whether issue 87 is a duplicate': VERDICT,
    'Fix the parser': 'The parser rejects the literal with E0412 instead of panicking.',
}


def candidates():
    """Every issue in the stand-in tracker that shares a word with the report."""
    tracker = pathlib.Path('forge') / 'issues.md'
    found = [line.strip() for line in tracker.read_text(encoding='utf-8').splitlines()
             if line.startswith('#') and REPORT & set(re.findall(r'[a-z]+', line.lower()))]
    return ''.join(line + '\n' for line in found)


def found():
    listed = [line.split(' ')[0] for line in candidates().splitlines()]
    return 'Found %d candidates for issue 87: %s.' % (len(listed), ' and '.join(listed))


# The state shape's handoff, which the verdict state reads.
PROGRAM_WRITES = {'lookup': {'runtime/candidates/{task}.md': candidates}}
# The task shape's export and result, which the verdict task and Plan History read.
PROGRAM_EXPORTS = {
    ('lookup', 'Search for duplicate candidates'): lambda: {'candidates': candidates()},
}
PROGRAM_RESULTS = {('lookup', 'Search for duplicate candidates'): found}

EXPORTS = {}

STATE_OUTPUTS = {}


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
