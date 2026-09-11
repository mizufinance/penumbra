"""Run one experiment, stopping its process group on memory/swap pressure."""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time


def swap_mb():
    s = subprocess.check_output(['sysctl', 'vm.swapusage'], text=True)
    m = re.search(r'used = ([0-9.]+)([MG])', s)
    return float(m[1]) * (1024 if m[2] == 'G' else 1)


def rss_kb(root):
    lines = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,rss='], text=True).splitlines()
    rows = [tuple(map(int, line.split())) for line in lines]
    family = {root}
    while True:
        expanded = family | {pid for pid, parent, _ in rows if parent in family}
        if expanded == family:
            return sum(rss for pid, _, rss in rows if pid in family)
        family = expanded


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--log', required=True)
    p.add_argument('--memory-gb', type=float, default=12)
    p.add_argument('--seconds', type=int, default=1200)
    p.add_argument('command', nargs=argparse.REMAINDER)
    a = p.parse_args()
    command = a.command[1:] if a.command[0] == '--' else a.command
    start_swap = swap_mb()
    started = time.monotonic()
    peak = 0
    reason = None
    with open(a.log, 'w') as log:
        proc = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        while proc.poll() is None:
            peak = max(peak, rss_kb(proc.pid))
            if peak > a.memory_gb * 1024 ** 2:
                reason = 'memory_limit'
            elif swap_mb() > start_swap + 64:
                reason = 'swap_pressure'
            elif time.monotonic() - started > a.seconds:
                reason = 'time_limit'
            if reason:
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
                break
            time.sleep(1)
    result = dict(command=command, exit_code=proc.returncode, stopped_for=reason,
                  elapsed_seconds=time.monotonic()-started, sampled_peak_rss_kb=peak,
                  swap_start_mb=start_swap, swap_end_mb=swap_mb())
    Path(a.log + '.metrics.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
    raise SystemExit(proc.returncode or (1 if reason else 0))


if __name__ == '__main__':
    main()
