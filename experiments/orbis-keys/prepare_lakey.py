#!/usr/bin/env python3
"""Create bounded-state fixtures from the pinned LaKey REG implementation."""
import argparse
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument("upstream", type=Path)
p.add_argument("--log2q", type=int, choices=[12, 32], default=12)
p.add_argument("--stat", type=int, choices=[40, 128], default=40)
p.add_argument("--mode", choices=["init", "derive", "refresh"], required=True)
args = p.parse_args()
source = (args.upstream / "Programs/Source/lattice_prf.mpc").read_text()
source = source.split("def main():")[0]
source = source.replace("stat = 40 # statistical distance", f"stat = {args.stat} # experiment statistical distance")
prefix = f'''import os
program.set_security({args.stat})
os.environ.update(BASE_RING="GFP", KEYGEN="1", COMPOSE="1", REVEAL="0",
                  BITS="251", LOG2Q="{args.log2q}", LOG2P="{8 if args.log2q == 12 else 24}",
                  DIM="512", OPTIMIZE="0", DOUBLE="0")
'''
body = '''
from hashlib import shake_256
from Compiler.library import start_timer, stop_timer

# The request supplies an identity, never an arbitrary matrix.
def identity_matrix(identity):
    data = shake_256(b"shieldd-lakey-experiment-v1\\0" + identity).digest(l*m*4)
    a = Matrix(l, m, cint)
    for i in range(l):
        for j in range(m):
            off = (i*m+j)*4
            a[i][j] = int.from_bytes(data[off:off+4], "little") & ((1<<log2q)-1)
    return a
'''
if args.mode == "init":
    body += '''
start_timer(1)
k = Array(m, sint)
for i in range(m):
    k[i] = srand()
k.write_to_file(position=0)
stop_timer(1)
print_ln("Master shares initialized; no clear key output")
'''
else:
    body += '''
k = Array(m, sint)
k.read_from_file(0)
'''
    if args.mode == "refresh":
        body += '''
start_timer(2)
for i in range(m):
    z = sint.get_random()
    k[i] = k[i] + z - z.reveal()
k.write_to_file(position=0)
stop_timer(2)
print_ln("Experimental same-committee zero-share refresh; not membership migration")
'''
    body += '''
start_timer(3)
identities = [b"chain1/ring1/epoch1/named/Alice/amount",
              b"chain1/ring1/epoch1/named/Bob/amount",
              b"chain1/ring1/epoch1/named/Alice/sender",
              b"chain1/ring1/epoch1/general/amount"]
derived = [eval(identity_matrix(identity), k) for identity in identities]
stop_timer(3)
# Fixed four-slot synthetic handoff, removed by the driver after local PRE.
# This is not production persistent storage of derived shares.
sint.write_to_file(derived, position=m)
print_ln("Four transient derived shares written per node; no clear key output")
'''
name = f"audit_lakey_{args.log2q}_{args.stat}_{args.mode}"
path = args.upstream / "Programs/Source" / (name + ".mpc")
path.write_text(prefix + source + body)
print(name)
