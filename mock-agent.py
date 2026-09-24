import pathlib, os, sys
def env(k): return os.environ[k]
def append(p, t):
    p.parent.mkdir(parents=True, exist_ok=True)
    open(p,'a').write(t)
def write(p, t):
    p.parent.mkdir(parents=True, exist_ok=True)
    open(p,'w').write(t)
root = pathlib.Path(env('RHEI_ROOT'))
log = root / 'runtime' / 'spawn-count.log'
seen = len(log.read_text(encoding='utf-8').splitlines()) if log.exists() else 0
append(log, '{}\n'.format(env('RHEI_STATE')))
if seen >= 25:
    raise SystemExit(17)
write(root / 'runtime' / 'work.md', 'work\n')
write(root / 'runtime' / 'review.md', 'review\n')
