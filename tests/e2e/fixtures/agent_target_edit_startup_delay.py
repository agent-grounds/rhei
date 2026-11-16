# Prepended through the real scenario's scheduler-prefix seam.
# Six seconds models a healthy startup stall, not a timeout value.
# §FS-rhei-agent-visit-pairing.5.1
import os as _delay_os
import time as _delay_time

if _delay_os.environ.get('RHEI_TARGET') == 'mock:mock:d':
    print('rhei-739: d interpreter entered; delaying scheduler entry by 6 seconds', flush=True)
    _delay_time.sleep(6)
    print('rhei-739: d scheduler released', flush=True)
