# Real scheduler recovery after observed admission, never a timed guess.
# §FS-rhei-run.3.4
import json
import subprocess

root = pathlib.Path(__file__).parent
action = os.environ['ISSUE671_ACTION']
deadline = time.monotonic() + 60
while time.monotonic() < deadline:
    path = root / 'runtime/events.jsonl'
    raw = path.read_text(encoding='utf-8') if path.exists() else ''
    events = [json.loads(line) for line in raw.splitlines(keepends=True)
              if line.endswith('\n')]
    if action == 'arrival':
        observed = 'no such reservation' in raw
    else:
        refusals = [e for e in events if e.get('event') == 'budget_halt'
                    and e.get('task') == 'evidence.1']
        observed = len(refusals) >= (2 if action == 'fanout' else 1)
    if observed:
        break
    time.sleep(.01)
else:
    raise RuntimeError('never observed the real admission refusal')


def cli(args):
    out = subprocess.run([os.environ['ISSUE671_BINARY']] + args,
                         capture_output=True, text=True, encoding='utf-8', timeout=60)
    print(out.stdout, out.stderr, flush=True)
    assert out.returncode == 0, out


if action == 'arrival':
    for before, after in [('write', 'pause'), ('pause', 'write')]:
        cli(['--state-machine', str(root / 'states.yaml'), 'transition', str(root),
             '--task', '1', '--from', before, '--to', after, '--no-callbacks'])
    task = root / 'tasks/1.md'
    write(task, task.read_text(encoding='utf-8') + '**Assignee:** fixture-holder\n')
else:
    cli(['budget', 'adjust', str(root), '--invocations',
         '2' if action == 'fanout' else '10', '--reason', 'fixture recovery'])
    if action in ['replace', 'allocation']:
        task = root / 'tasks/1.md'
        write(task, task.read_text(encoding='utf-8').replace('**Model:** old', '**Model:** new'))
write(root / 'repaired', action)
