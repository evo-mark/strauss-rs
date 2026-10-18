---
title: Test coverage
description: "How Strauss-RS is tested: 723 automated tests that cover 92% of its lines, with coverage by area, how the tests are organised, and how to run them or measure coverage."
---

# Test coverage

Strauss-RS rewrites other people's code, so most of its tests check exactly what comes out. This page details what is tested, how the tests are organised and how to run them yourself.

## What is tested

There are 723 automated tests, and all of them pass. 615 are unit tests inside the program. The other 108 run the built `strauss-rs` program on a copy of a small PHP project and check the files and messages it produces.

| Area                             | Unit tests | Lines covered | What they check                                                                                         |
| :------------------------------- | ---------: | ------------: | :------------------------------------------------------------------------------------------------------ |
| Editing (`edits`)                |        152 |           95% | Rewriting namespaces, `use` statements, strings, docblocks and your own files in `update_call_sites`.   |
| Renaming (`rename`)              |         89 |           96% | Which symbols are renamed and to what, including exclusions, autoload kinds and already-prefixed names. |
| PHP analysis (`analyser`)        |         86 |           86% | Finding declarations and references in PHP source, with their exact positions.                          |
| Configuration (`config`)         |         78 |           97% | Reading `strauss.json` and `composer.json`, defaults, inferred prefixes and every error message.        |
| Packages (`packages`)            |         57 |           96% | Reading `installed.json`, finding each package's files, `.gitattributes`, and platform packages.        |
| Writing output (`output`)        |         53 |           91% | Copying, in-place changes, the autoloader, file headers and deleting the originals.                     |
| Symbol table (`symbols`)         |         50 |           98% | Collecting symbols from every file, built-in and preset constants, and running in parallel.             |
| Command line (`cli`, `commands`) |         33 |           88% | Options, error messages, `init`, `migrate` and `explain`.                                               |
| Patterns and workers             |         17 |           98% | PHP-style regular expressions, and results not depending on the number of workers.                      |

Lines covered counts the lines of each area's code that run in any test, unit or program. The figures are from `cargo llvm-cov` and include the program tests, so an area can be covered by tests that are not listed against it. The command line and commands share a row because they are tested together; the tiny `main.rs` is at 100% and left out of the table.

The 108 program tests are in seven files: `prefix` (23), `commands` (25), `packages` (17), `cli` (13), `rename` (11), `edits` (10) and `symbols` (9).

## Overall

| Measure   | Covered |
| :-------- | ------: |
| Lines     |   92.4% |
| Functions |   93.3% |
| Regions   |   91.1% |

That is 6,519 of 7,057 lines. Branch coverage is not reported.

## Kinds of test

- **Small cases.** Most unit tests give one piece of PHP to the analyser or editor and check the result, such as a `use` statement with an alias, a name inside a heredoc or a class named in an attribute.
- **Whole-project fixtures.** Folders under `test/` hold small projects with packages that exercise each rule. The tests copy a fixture to a scratch folder, run the pipeline or the program, and check the files that come out.
- **Projects built by the test.** Where a fixture would hide the point, the test writes its project itself. For example, the in-place tests prefix a project, restore one package's files as `composer update` would, prefix again, and check that the reinstalled package is prefixed and the untouched one is not prefixed twice.

## Run them

```sh
cargo test
```

To run one area, give part of a test's name:

```sh
cargo test rename
cargo test --test prefix
```

## Reproducing coverage results

Test coverage results provided by [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov):

```sh
cargo install cargo-llvm-cov
cargo llvm-cov --html
```

The report is written to `target/llvm-cov/html/index.html`. For a summary in the terminal, run `cargo llvm-cov` on its own.
