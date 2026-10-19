---
title: Benchmarks vs Strauss PHP
description: "Strauss-RS is 24x to 28x faster than Strauss PHP on one worker and up to 139x faster on 12, with far less memory. Includes the method and how to reproduce."
image: /social/benchmarks.jpg
imageAlt: Benchmark results comparing Strauss-RS with Strauss PHP
---

# Benchmarks

These are timings of `prefix` for Strauss PHP and Strauss-RS on four projects of different sizes. They were taken on one machine, so treat them as a guide to the size of the difference. Your results will vary with your hardware, your project and your disk.

## Results

| Project | Workers | Strauss PHP | Strauss-RS | Speedup | PHP peak memory | RS peak memory |
| :------ | ------: | ----------: | ---------: | ------: | --------------: | -------------: |
| small   |       1 |    625.4 ms |    22.8 ms |   27.4x |          70 MiB |        9.5 MiB |
| small   |      12 |    831.3 ms |     9.1 ms |   91.0x |         411 MiB |       11.7 MiB |
| medium  |       1 |      1.11 s |    46.5 ms |   24.0x |          83 MiB |       13.1 MiB |
| medium  |      12 |      1.38 s |    16.2 ms |   85.5x |         599 MiB |       19.5 MiB |
| large   |       1 |     18.72 s |   761.3 ms |   24.6x |         259 MiB |       81.6 MiB |
| large   |      12 |     16.84 s |   188.7 ms |   89.3x |       1,291 MiB |       97.7 MiB |
| xlarge  |       1 |     42.26 s |     1.50 s |   28.2x |         990 MiB |      336.2 MiB |
| xlarge  |      12 |     55.05 s |   395.6 ms |  139.1x |       5,556 MiB |      405.4 MiB |

- Small: monolog
- Medium: guzzle + flysystem + finder
- Large: laravel/framework
- X-Large: aws-sdk-php

## How they were run

- Wall time for `prefix` command
- Median of 5 runs after 1 warm-up, from a clean output directory each time.
- Both tools were given the worker count explicitly (1 against 1, 12 against 12).
- Memory is peak total across all processes, sampled on Linux.
- Strauss PHP 0.31.1 on PHP 8.4.23 (static CLI build, no OPcache, no memory limit).
- Strauss-RS release build.
- Intel i5-10600 (12 threads), 32 GB RAM.
- All projects with locked dependencies.

## What to keep in mind

- These measure the `prefix` step. A Composer install and your own build steps take the same time with either tool.

## How to reproduce benchmarks

Clone the repo from GitHub, then run the following commands

```sh
cargo run --release --features benchmark -- benchmark setup
cargo run --release --features benchmark -- benchmark run
```

Setup downloads the pinned Strauss PHP 0.31.1 phar and runs composer install for each project. run has these options:

| Option             | Description                                                                                              |
| ------------------ | -------------------------------------------------------------------------------------------------------- |
| `--project <NAME>` | Only run this project. Can be given more than once. By default every project in `benchmark.json` is run. |
| `--tool <rs\|php>` | Only run this tool. Can be given more than once. By default both are run.                                |
| `--runs <N>`       | How many timed runs of each. The default is 5.                                                           |
| `--warmup <N>`     | How many runs to throw away first. The default is 1.                                                     |
| `--workers <N>`    | How many workers in the runs with many. Must be 2 or more. The default is one per CPU.                   |
| `--php <PATH>`     | The PHP binary that runs Strauss PHP and checks the output. The default is `php` from your `PATH`.       |
| `--json <FILE>`    | Write the results, and what they were measured on, to this JSON file.                                    |
| `--keep`           | Leave the last output in place, instead of deleting it.                                                  |
| `--no-check`       | Don't parse the output with PHP.                                                                         |
| `--no-tree-memory` | Don't take the extra run that adds up the memory of every process.                                       |
| `--allow-debug`    | Run even though this is a debug build. The times mean nothing in a debug build.                          |
