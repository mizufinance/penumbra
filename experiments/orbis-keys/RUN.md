# Reproduce the key experiments

These are synthetic fixtures, not operational ring state. Use a fresh MP-SPDZ
checkout: the initialization program overwrites its experimental master shares.
Run one heavy command at a time. Check memory, swap, disk, and other workloads.

The paths below use `experiment_dir` for this directory and `mpc_dir` for a
separate dependency checkout. Set them to actual absolute paths.

```sh
experiment_dir=/absolute/path/shieldd-key-experiments/experiments/orbis-keys
mpc_dir=/absolute/path/orbis-experiment-deps/MP-SPDZ
export CARGO_BUILD_JOBS=2 RAYON_NUM_THREADS=2 GOMAXPROCS=2
```

`run_bounded.py` stops its own process group at the RSS limit, timeout, or a
64 MiB increase in swap. Its defaults are 12 GiB and 20 minutes. Do not interpret
its one-second RSS samples as precise peak-memory measurements for short jobs.

## Native vetKeys and PRSS

```sh
cd "$experiment_dir/native"
python3 ../run_bounded.py --log ../results/reproduce-native-build.log -- cargo build --release
python3 ../run_bounded.py --log ../results/reproduce-vetkeys.log -- target/release/orbis-key-experiments
python3 ../run_bounded.py --log ../results/reproduce-prss.log -- target/release/prss
```

The vetKeys fixture uses a local Shamir dealer and official client verification.
The PRSS fixture uses synthetic subset seeds. Neither invokes a production key
service. `Cargo.lock` pins dependencies, including the existing Orbis crypto
implementation used by `lakey_pre`.

## Real component proofs

```sh
cd "$experiment_dir/../../tools/gnark"
python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-pairing.log" -- go run -p 2 ./cmd/audit-key-experiment -prove
python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-registry.log" -- go run -p 2 ./cmd/audit-registry-experiment
```

Both generate development setup in memory. They do not modify production
artifacts. The pairing test is deliberately a component, not full IBE. The
membership test is deliberately a component, not full Transfer.

## LaKey backend

On the tested macOS system, GMP, Boost, libsodium, and OpenSSL came from Homebrew.
The supplied configuration uses `/opt/homebrew`. Adapt dependency paths for a
different platform. The MPIR build is unnecessary for this working adaptation.

```sh
git clone --branch lattice-prf https://github.com/MetaMask/MP-SPDZ.git "$mpc_dir"
cd "$mpc_dir"
git checkout 7a9efcadf134263a94cd0548456e60011a7d4492
git submodule update --init deps/simde
git apply "$experiment_dir/lakey-macos.patch"
cp "$experiment_dir/CONFIG.mine.macos" CONFIG.mine
python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-mpc-build.log" -- make -j2 malicious-shamir-party.x
bash Scripts/setup-ssl.sh 5
chmod 600 Player-Data/*.key
```

`NO_MIXED_CIRCUITS` disables the unused binary backend, which cannot initialize
at the requested 128-bit setting in this old fork. Arithmetic Shamir execution,
live preprocessing, and malicious checks remain enabled. Unsupported binary
operations fail; they are not simulated. No `Fake-Offline` or `-F` option is used.

## REG32, five nodes, real preprocessing

Use a fresh `Persistence` directory. The driver compiles fixed test identities;
production must instead implement a fixed, authenticated derivation kernel.

```sh
cd "$mpc_dir"
umask 077
scalar_prime=2111115437357092606062206234695386632838870926408408195193685246394721360383
python3 "$experiment_dir/prepare_lakey.py" . --log2q 32 --stat 128 --mode init
python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-init-compile.log" -- ./compile.py -O -g 256 audit_lakey_32_128_init
PLAYERS=5 THRESHOLD=2 python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-init.log" -- bash Scripts/mal-shamir.sh audit_lakey_32_128_init --prime "$scalar_prime" -S 128

python3 "$experiment_dir/prepare_lakey.py" . --log2q 32 --stat 128 --mode derive
python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-derive-compile.log" -- ./compile.py -O -g 256 audit_lakey_32_128_derive
PLAYERS=5 THRESHOLD=2 python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-derive.log" -- bash Scripts/mal-shamir.sh audit_lakey_32_128_derive --prime "$scalar_prime" -S 128

"$experiment_dir/native/target/release/lakey_pre" "$mpc_dir/Persistence" 5 3 "$experiment_dir/results/reproduce-public-reference.json" 32 128
```

MP-SPDZ's `THRESHOLD=2` means degree/corruption bound two, corresponding to
three shares for reconstruction. It does not mean a 2-of-5 PRE threshold.

The compile-time `-g 256` is the upstream compiler option used in these runs;
the runtime `--prime` supplies the exact Decaf377 scalar field. Avoid `-P` for
this bounded-integer program: that selects full-field conversion and was much
more expensive. Both routes were checked against the same independent reference.
Keep all stated integer and statistical bounds below the runtime field capacity.

The final test reads synthetic fixtures from all nodes, checks an independent
clear LaKey reference, and exercises existing Orbis PRE. It removes the four
transient derived-share slots, retaining the 512 master shares per node. It is
not a production share-transport design. Its reference file contains only
public keys; use a new reference filename when initializing a new master.

## Restart and refresh

Repeat the derive execution and `lakey_pre` with the same public reference to
test process restart. Then run:

```sh
cd "$mpc_dir"
python3 "$experiment_dir/prepare_lakey.py" . --log2q 32 --stat 128 --mode refresh
python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-refresh-compile.log" -- ./compile.py -O -g 256 audit_lakey_32_128_refresh
PLAYERS=5 THRESHOLD=2 python3 "$experiment_dir/run_bounded.py" --log "$experiment_dir/results/reproduce-refresh.log" -- bash Scripts/mal-shamir.sh audit_lakey_32_128_refresh --prime "$scalar_prime" -S 128
"$experiment_dir/native/target/release/lakey_pre" "$mpc_dir/Persistence" 5 3 "$experiment_dir/results/reproduce-public-reference.json" 32 128
```

Use `--log2q 12 --stat 40`, program names `audit_lakey_12_40_*`, `PLAYERS=3`,
`THRESHOLD=1`, and PRE arguments `3 2 ... 12 40` for the smaller configuration.
Do not mix old persistent state or public references with a newly initialized
master. Store no synthetic private fixture files in Git.
