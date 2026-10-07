# Portable replay of rhei.53's incident, with a future date and private files.
# No provider is contacted. The OS-local conversion is independent of Rhei.
import datetime
import json

root = pathlib.Path(env('RHEI_ROOT'))
append(root / 'runtime' / 'starts.txt', env('RHEI_ATTEMPT') + '\n')
expected_path = root / 'runtime' / 'expected.json'
if not expected_path.exists():
    if hasattr(time, 'tzset'):
        time.tzset()
    if TRANSPORT == 'control':
        signal = "You've hit your session limit · resets 11:19pm (Etc/GMT-2)"
        now = datetime.datetime.now(datetime.timezone.utc)
        named_local = (now + datetime.timedelta(hours=2)).replace(tzinfo=None)
        boundary = named_local.replace(hour=23, minute=20, second=0, microsecond=0)
        if boundary <= named_local:
            boundary += datetime.timedelta(days=1)
        utc = (boundary - datetime.timedelta(hours=2)).replace(tzinfo=datetime.timezone.utc)
    else:
        local = (datetime.datetime.now() + datetime.timedelta(days=2)).replace(
            hour=23, minute=19, second=0, microsecond=0)
        day = local.day
        ordinal = 'th' if 11 <= day <= 13 else {1: 'st', 2: 'nd', 3: 'rd'}.get(day % 10, 'th')
        month = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
                 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][local.month - 1]
        when = f'{month} {day}{ordinal}, {local.year:04d} 11:19 PM'
        signal = ("You've hit your usage limit. Visit https://chatgpt.com/codex/settings/usage "
                  f'to purchase more credits or try again at {when}.')
        boundary = local + datetime.timedelta(minutes=1)
        utc = datetime.datetime.fromtimestamp(time.mktime(boundary.timetuple()), datetime.timezone.utc)
    write(expected_path, json.dumps({'signal': signal, 'deadline': utc.strftime('%Y-%m-%dT%H:%M:%SZ')}))
signal = json.loads(expected_path.read_text())['signal']
if TRANSPORT == 'json':
    assert '--json' in sys.argv, sys.argv
    for event in [
        {'type': 'thread.started', 'thread_id': 'fixture-thread'},
        {'type': 'turn.started'},
        {'type': 'error', 'message': signal},
        {'type': 'turn.failed', 'error': {'message': signal}},
    ]:
        print(json.dumps(event), flush=True)
else:
    print(signal, file=sys.stderr if TRANSPORT == 'stderr' else sys.stdout, flush=True)
raise SystemExit(1)
