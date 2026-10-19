---
title: How it works
description: "How Strauss-RS prefixes a Composer project: the stages of a run, the PHP parser and lexer it reads code with, and the Rust crates used for parallelism, globbing, patterns and output."
---

# How it works

This page is for people who want to know what happens between `strauss-rs prefix` and the files that come out, and which Rust crates do the work. You don't need any of it to use Strauss-RS.

The short version: every PHP file is parsed into a syntax tree, the names it declares and uses are collected, a plan says what each name becomes, and the plan is applied as exact byte-range edits to the original text. The source is never printed back from a tree, so formatting and comments survive untouched.

## The stages

`prefix` runs these in order. Each stage reads what the one before produced and none of them writes anything until the last.

| Stage         | Module     | What it does                                                                                              |
| :------------ | :--------- | :-------------------------------------------------------------------------------------------------------- |
| Configuration | `config`   | Reads `strauss.json` and `composer.json`, fills in defaults, infers prefixes and validates every setting. |
| Packages      | `packages` | Finds the packages through `installed.json` and lists their files.                                        |
| Symbol scan   | `symbols`  | Parses every PHP file and collects the symbols each package declares.                                     |
| Rename plan   | `rename`   | Decides which symbols are renamed, and to what.                                                           |
| Edit plan     | `edits`    | Works out every change needed in the files so the renamed symbols still resolve.                          |
| Output        | `output`   | Copies the files with their edits made, writes the autoloader, updates your own files and cleans up.      |

`plan`, `diff`, `symbols`, `files`, `packages` and `config` stop partway and print what they have. That is why they are safe to run at any time: they share the stages above and only `prefix` reaches the output stage. `--dry-run` runs the output stage without writing.

## Reading PHP

PHP can't be prefixed with search and replace. `Client` might be a class in your namespace, an imported name, a string in a docblock or a method called `Client`. Strauss-RS parses the code so it knows which.

### `php-rs-parser`, `php-ast` and `php-lexer`

These three crates are the PHP front end.

- **`php-rs-parser`** turns source into a syntax tree. A file that doesn't parse is reported and skipped; the other files carry on.
- **`php-ast`** defines that tree, including the `owned` form the analyser works with, and a `NameResolver`. PHP names are resolved against the surrounding `namespace` and `use` statements, so `new Client` becomes `GuzzleHttp\Client`, and an unqualified function call is treated as the namespaced function or the global one, as PHP does.
- **`php-lexer`** produces the token stream.

Why both a tree and tokens? The tree has the structure but not every position. It doesn't give the position of the name in `class Foo`, and for an item inside a group `use A\{B, C}` the position it gives runs from the start of the statement. The analyser (`analyser/spans.rs`) finds those exact locations from the tokens. Strings and comments also come from the tokens (`analyser/regions.rs`), because a string like `'Acme\\Lib\\Client'` or a `@param` in a docblock can name a symbol without being code the parser resolves.

The result for each file is a list of declarations (classes, interfaces, traits, enums, functions and constants) and references, each with a byte span.

## Doing it in parallel

### `rayon`

Parsing is the expensive part, and it is independent for each file, so it is spread over a `rayon` thread pool (`parallel.rs`). Rayon hands files to workers as they become free rather than splitting the list up front, so one huge file doesn't leave the other workers idle. The same pool is used to list package files, plan edits and write output.

Things worth knowing:

- Workers are threads of one process, unlike PHP Strauss where each worker is a process. The default `parallel` setting uses every CPU.
- The analyser recurses into nested code, so each worker gets a 16 MiB stack, and even a single worker runs inside the pool rather than on the smaller main thread stack.
- Results come back in input order, so the output does not depend on the number of workers. This is covered by tests that run with different worker counts.
- Analysis runs inside `catch_unwind`. A bug that a particular file triggers is reported against that file and doesn't stop the run.

## Finding packages and files

### `serde` and `serde_json`

`installed.json`, `composer.json` and `strauss.json` are read with `serde`. Settings are first read into a raw, permissive shape (`config/raw.rs`) and then validated into the `Config` the rest of the program uses, so every error message can name the setting at fault. `serde_json` is built with `preserve_order`, so JSON that Strauss-RS writes, such as `init` and `migrate` output, keeps the order of keys the user already had.

The `--json` output of `config`, `packages`, `files`, `symbols`, `plan` and `prefix` is these same types serialised.

### `ignore`

Package directories and your own source are walked with `ignore::WalkBuilder`. It is configured to read only what's asked of it: hidden files are included, parent and global ignore rules are off, and links aren't followed. For a package that opts in, `.gitignore` rules are honoured. The same walker finds your own files for `update_call_sites`.

### `globset`

A package's `.gitattributes` marks files it leaves out of its release archive with `export-ignore`: tests, CI configuration and so on. Strauss-RS reads those lines, compiles each pattern with `globset` and leaves the matching files out of the copy. A pattern without a slash matches a name in any directory, as it does in git; one with a slash matches from the package root.

### `fancy-regex`

Patterns in `strauss.json`, such as `file_patterns` and `namespace_replacement_patterns`, were written for PHP's `preg_match()` and `preg_replace()`, so they have to keep working. `fancy-regex` is used instead of `regex` because PCRE patterns commonly use lookarounds and backreferences, which `regex` doesn't support. `php_pattern.rs` wraps it to add what PHP does around it: optional delimiters (`~foo~`), literal-or-regex keys, and `preg_replace` replacement syntax (`$1`, `${1}`, `\1`).

## Deciding what to rename

The rename stage (`rename`) is plain Rust over the symbol table. It follows Strauss PHP's order:

1. Add the namespaces from each package's autoload rules.
2. Find the symbols that are autoloaded.
3. Mark symbols for renaming, minus exclusions: packages, namespaces, files, built-ins and the Wordpress, WooCommerce, WP-CLI and PHP constant presets.
4. Work out the new names: the namespace prefix on namespaced symbols, `classmap_prefix` on global classes, and the function and constant prefixes.

Symbols PHP provides itself are left out of the table to begin with, so a package that declares a polyfill can't take over a built-in name.

## Making the edits

The edit stage turns the plan into a list of `Edit { start, end, replacement }` values for each file, in terms of byte offsets into the original source. This is what keeps the tool precise:

- All edits refer to the original text, so none moves another, and they can be made in any order.
- Edits that overlap, fall outside the file or land in the middle of a UTF-8 character are an error, and the file is left unchanged.
- A file that isn't valid UTF-8 is copied as it is, with a warning, since offsets in it can't be trusted.

Applying them is a single pass over the original text (`edits/apply.rs`). Nothing is re-parsed and nothing is reformatted.

### `similar`

The `diff` command applies the edits in memory and shows them as a unified diff with `similar`, without touching any file. It is the same edit list `prefix` writes, so what you see is what you get.

## Writing the result

The output stage copies each file, makes its edits, adds the modification note to the header and writes it, keeping the file's permissions so an executable stays executable. It then writes the autoloader, applies edits to your own files if `update_call_sites` is on and finally deletes the originals if you asked for that. Nothing is deleted unless everything before it was written.

Copying is parallel too. `--dry-run` goes through the same code with the writes skipped, so its report matches a real run.

### `indicatif` and `clap`

- **`indicatif`** draws the spinners and progress bars. They are drawn only if stderr is a terminal and `--quiet` isn't given. When output is piped or logged you get plain lines, and the library code and its tests never draw anything.
- **`clap`** defines the command line with derive macros (`cli/mod.rs`), including the `--json`, `--dry-run`, `--parallel` and `--quiet` options.

## The crates at a glance

| Crate                                   | Used for                                                                                       |
| :-------------------------------------- | :--------------------------------------------------------------------------------------------- |
| `php-rs-parser`, `php-ast`, `php-lexer` | Parsing PHP, resolving names and locating exact positions.                                     |
| `rayon`                                 | Analysing, listing, planning and writing files in parallel.                                    |
| `serde`, `serde_json`                   | Reading configuration and `installed.json`, and all JSON output.                               |
| `ignore`                                | Walking package directories and your own source.                                               |
| `globset`                               | Matching `export-ignore` patterns from `.gitattributes`.                                       |
| `fancy-regex`                           | PHP-style patterns with lookarounds and backreferences.                                        |
| `similar`                               | The unified diff in `diff`.                                                                    |
| `indicatif`                             | Progress on a terminal.                                                                        |
| `clap`                                  | The command line.                                                                              |
| `libc`                                  | Measuring memory in the `benchmark` command, which is only built with the `benchmark` feature. |

There is no PHP runtime involved at any point, which is why Strauss-RS is a single binary with nothing else to install.
