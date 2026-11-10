# Bench notes

The gallery worker on the server is `banksidecms-imageoptimiser-worker.service`. It runs:

```text
/home/banksidecms/bin/php artisan queue:work database --queue=imageoptimiser --sleep=1 --tries=3 --timeout=300
```

The working directory is `/home/banksidecms/public_html/app`. The user is `banksidecms`. A job may run for 300 seconds, then it is stopped.

## Limits on that worker

The 50–75 second PHP and GD time is under these caps. The unit comment says this live worker is lighter than the UAT gallery units, which stay at 15% CPU and 3M of disk.

- `Nice=16`. The worker yields to other processes.
- `CPUQuota=10%`. About one tenth of one core.
- `CPUWeight=10`.
- `IOSchedulingClass=idle` and `IOWeight=8`. Disk waits behind other work.
- Read cap on `/home`: 2 MB/s and 40 reads per second.
- Write cap on `/home`: 1 MB/s and 20 writes per second.
- `MemoryHigh=128M`. `MemoryMax=192M`.
- `TasksMax=32`.

A 25 MB read at 2 MB/s is about 13 seconds of the cap before the pixels are decoded. The 23 output files then share a 1 MB/s write cap. Smashbin on this Mac has no such cap. A Mac clock and the server clock are different tests until smashbin is run on that server under the same unit, or the caps are written next to both numbers.

## The file in this checkout

`output/bench.png` is the server original. It is a PNG, 3543 by 3543, 25,427,716 bytes. `.gitignore` ignores `/output/` and `/output/bench.png`, so the file stays on this machine.

```bash
./target/release/smashbin batch --verbose --jobs fixtures/gallery.yaml --out output/variants output/bench.png
```

stderr ends with `23 files in` and the seconds. Do not treat a Mac clock as the server result unless the command ran under the caps above.

## This Mac, 10 Nov 2026

Release build, 8 CPU threads, no systemd cap. The file is the server PNG, 25,427,716 bytes, 3543 by 3543. It lives in `output/`, which git ignores.

```text
read: 0.026s
decode: 0.374s
encode: 2.747s
write: 0.014s
encode threads: 23
output bytes: 5535548
23 files in 3.169s
```

The square photo’s `preview_820x860` contain file is 820 by 820. Contain keeps the whole picture inside the box, so the short side of that box wins. This Mac run is not under the server’s 10% CPU cap.
