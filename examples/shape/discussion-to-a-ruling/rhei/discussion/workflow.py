"""Mock participants and judge for the discussion member of the
discussion-to-a-ruling shape pair, after `examples/agent-discussion`.

Stands in for real participant and judge agents so the example runs with no
credentials. `write-position` writes each participant's position for the
round, `judge-round` writes the round's digest and redirects, and the judge
agent writes the ruling once the round converges. The mock converges in
round 2.
"""

import os
import pathlib
import re
import sys

PARTICIPANTS = ('claude', 'codex')
CAP = 3

POSITIONS = {
    1: {
        'claude': 'Squash every pull request: one commit per change keeps history readable.\n',
        'codex': 'Merge commits only: a squash loses the commits a bisect needs.\n',
    },
    2: {
        'claude': 'codex is right about bisect: keep the commits when each one builds.\n',
        'codex': 'Agreed with claude: squash the ones whose commits do not build on their own.\n',
    },
}

RULING = ('Rebase-merge a pull request whose every commit builds; squash one whose '
          'commits do not.')


def discussion():
    plan = pathlib.Path(os.environ.get('RHEI_PLAN_PATH', '.'))
    return (plan if plan.is_dir() else plan.parent) / 'runtime' / 'discussion'


def current_round():
    """The round the positions on disk have reached."""
    return len([d for d in discussion().glob('round-*') if d.is_dir()])


def write_position():
    """A round opens after every digest, so the first position of a round starts it."""
    model = os.environ.get('RHEI_MODEL', 'none')
    number = len(list((discussion() / 'digest').glob('round-*.md'))) + 1
    write(discussion() / ('round-%d' % number) / (model + '.md'), POSITIONS[number][model])


def judge_round():
    number = current_round()
    parts = ['# Digest, round %d\n\n' % number]
    for model in PARTICIPANTS:
        position = discussion() / ('round-%d' % number) / (model + '.md')
        parts.append('- %s: %s' % (model, position.read_text(encoding='utf-8')))
    write(discussion() / 'digest' / ('round-%d.md' % number), ''.join(parts))
    if number >= 2:
        print('{"success": true, "nextState": "converged"}')
    elif number >= CAP:
        print('{"success": true, "nextState": "escalated"}')
    else:
        print('{"success": true}')

RESULTS = {}

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


if sys.argv[1:2] == ['write-position']:
    write_position()
    sys.exit(0)
if sys.argv[1:2] == ['judge-round']:
    judge_round()
    sys.exit(0)
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

if os.environ.get('RHEI_STATE') == 'judge':
    os.environ.setdefault('RHEI_PLAN_PATH', str(root))
    if current_round() >= 2 and result_match:
        write(root / 'runtime' / 'exports' / task / 'ruling.md', RULING + '\n')
        path = pathlib.Path(result_match.group(1))
        write(path if path.is_absolute() else root / path,
              'Ruled in round 2: ' + RULING[0].lower() + RULING[1:] + '\n')
    sys.exit(0)
if os.environ.get('RHEI_STATE') == 'collect':
    sys.exit(0)

for name, text in EXPORTS.get(title, {}).items():
    write(root / 'runtime' / 'exports' / task / (name + '.md'), text)
for pattern, text in STATE_OUTPUTS.get(os.environ.get('RHEI_STATE'), {}).items():
    write(root / pattern.format(task=task), text)

if result_match:
    path = pathlib.Path(result_match.group(1))
    summary = RESULTS.get(title, 'Did %s.' % title)
    write(path if path.is_absolute() else root / path, summary + '\n')
