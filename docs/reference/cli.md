---
title: Command line reference
description: "All Strauss-RS commands and options: prefix, diff, explain, init, migrate, include-autoloader, plan, config, packages, files, symbols and analyse."
---

# Command line

Run the commands through the installer script, which passes everything after its name to the program:

```sh
sh bin/strauss-rs.sh <command> [options]
```

If you have installed the binary yourself, use `strauss-rs <command>` instead. Run `strauss-rs <command> --help` for the same information in your terminal.

## Options for every command

| Option          | What it does                                                   |
| --------------- | -------------------------------------------------------------- |
| `-q`, `--quiet` | Show no progress and no summary, only warnings and errors.     |
| `-h`, `--help`  | Print help.                                                    |
| `-V`, `--version` | Print the version (on the main command only).                |

Most commands also take:

| Option                     | What it does                                                                  |
| -------------------------- | ----------------------------------------------------------------------------- |
| `-C`, `--project-dir <DIR>` | The plugin or theme root, which holds `strauss.json` and `composer.json`. Defaults to the current folder. |

## Commands

| Command              | What it does                                                           |
| -------------------- | ---------------------------------------------------------------------- |
| [`prefix`](#prefix)  | Copy packages to the target directory, rename symbols, write the autoloader. |
| [`diff`](#diff)      | Show the changes `prefix` would make.                                  |
| [`explain`](#explain) | Explain what happens to one symbol, and why.                          |
| [`init`](#init)      | Write a starting `strauss.json`.                                       |
| [`migrate`](#migrate) | Move `extra.strauss` from `composer.json` into `strauss.json`.        |
| [`include-autoloader`](#include-autoloader) | Make `vendor/autoload.php` load the prefixed packages. |
| [`plan`](#plan)      | Show which symbols would be renamed, and why others would not.         |
| [`config`](#config)  | Print the resolved configuration as JSON.                              |
| [`packages`](#packages) | List the packages to process.                                       |
| [`files`](#files)    | List the files of those packages.                                      |
| [`symbols`](#symbols) | List the symbols each package declares.                               |
| [`analyse`](#analyse) | Analyse one PHP file.                                                 |

### prefix

Copies the packages to `target_directory` with their namespaces and global symbols renamed, and writes the autoloader. `vendor` is left as it is unless `delete_vendor_files` or `delete_vendor_packages` is set. A summary and any warnings go to stderr.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--dry-run`                           | Work out what would be written, and write nothing. |
| `--json`                              | Print what was written as JSON, instead of the summary. |
| `--parallel[=<auto\|false\|N>]`        | How many workers read the files. Overrides `parallel`. `auto` (or `true`) is one per CPU, `false` or `1` is one worker, a number caps the workers. |
| `--updateCallSites[=<true\|false\|PATHS>]` | Which of your files to update where they use prefixed names. Overrides `update_call_sites`. `true` is the files in your `autoload`, `false` is none, or give a comma-separated list of files and folders. |

### diff

Shows the changes `prefix` would make, as a diff. Nothing is written. It runs the same analysis as `plan`, then works out every place that has to change so that the renamed symbols still work.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--package <NAME>`                    | Only show the files of this package. Can be given more than once. |
| `--json`                              | Show the edits as JSON, with their positions, instead of a diff. |
| `--parallel[=<auto\|false\|N>]`        | As for `prefix`. |
| `--updateCallSites[=<true\|false\|PATHS>]` | As for `prefix`. |

### explain

```sh
sh bin/strauss-rs.sh explain 'GuzzleHttp\Client'
```

Shows whether a class, function, constant or namespace is renamed, what to, and which setting decided it. Symbols that PHP or a preset provides are explained too, even when no package declares them. It exits with an error if nothing matches, and lists similar names.

The name is the full name of the symbol, such as `GuzzleHttp\Client`, `my_function` or `ABSPATH`. A leading backslash is ignored. Names are matched without regard to case, except constants.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--kind <KIND>`                       | Only consider one kind of symbol, for a name that is more than one thing. One of `namespace`, `class`, `interface`, `trait`, `enum`, `function`, `constant`. |
| `--json`                              | Print the explanation as JSON. |
| `--parallel[=<auto\|false\|N>]`        | As for `prefix`. |

### init

Writes a starting `strauss.json`, with the prefixes worked out from `composer.json`. Dependencies go under `<your namespace>\Vendor` so they cannot meet your own classes. It refuses to replace an existing `strauss.json` unless you pass `--force`.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--force`                             | Replace `strauss.json` if there is one. |
| `--dry-run`                           | Show what would be written, and write nothing. |
| `--namespace-prefix <PREFIX>`         | The `namespace_prefix` to write, instead of the one worked out. |
| `--classmap-prefix <PREFIX>`          | The `classmap_prefix` to write, instead of the one worked out. |
| `--target-directory <DIR>`            | The `target_directory` to write, if not `vendor-prefixed`. |
| `--composer-scripts`                  | Add scripts to `composer.json` that run `prefix` after `composer install` and `composer update`, without asking. |
| `--no-composer-scripts`               | Do not add the Composer scripts, and do not ask. |

Your `composer.json` keeps its key order and indentation when scripts are added.

### migrate

Moves Strauss PHP's `extra.strauss` from `composer.json` into `strauss.json`. It checks the options are ones Strauss-RS knows before writing anything, then removes `extra.strauss`. It sets `classmap_prefix` where Strauss PHP would have worked it out differently, so your class names stay the same. See [moving from Strauss PHP](/guide/migrating).

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--force`                             | Replace `strauss.json` if there is one. |
| `--dry-run`                           | Show what would be written, and write nothing. |

### include-autoloader

Adds a `require_once` of the target directory's `autoload.php` to `vendor/autoload.php`, before Composer's loader is returned. It is safe to run again. Run `prefix` first so there is an autoloader to include. This is what the `include_autoloader` setting does after `prefix`.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--dry-run`                           | Work out what would change, and write nothing. |

### plan

Shows which symbols would be renamed, to what, and why not the others, as JSON. Without `--package` this is every symbol, which for a large project is a lot. The `summary` part has the counts.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--package <NAME>`                    | Only show the symbols this package declares. Can be given more than once. |
| `--parallel[=<auto\|false\|N>]`        | As for `prefix`. |

### config

Prints the resolved configuration as JSON, with every setting shown with its default or inferred value, in the form `strauss.json` takes. It applies any options given here first.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--updateCallSites[=<true\|false\|PATHS>]` | As for `prefix`. |
| `--parallel[=<auto\|false\|N>]`        | As for `prefix`. |

### packages

Lists the packages to process as JSON: those in `packages`, and everything they require. It reads `vendor/composer/installed.json`, so run `composer install` first. Warnings go to stderr.

### files

Lists the files of those packages as JSON, and which would be copied. Every file in a package is listed, except those git would leave out of an archive: `.git`, files matched by `.gitignore`, and `export-ignore` files.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--parallel[=<auto\|false\|N>]`        | As for `prefix`. |

### symbols

Lists the symbols each package declares as JSON: namespaces, classes, interfaces, traits, enums, functions and constants. It reads every `.php` file of every package, so it can take a moment. PHP's own symbols, which polyfills redeclare, are listed separately as `built_ins`. Files that cannot be read are reported on stderr.

| Option                                | What it does |
| ------------------------------------- | ------------ |
| `--parallel[=<auto\|false\|N>]`        | As for `prefix`. |

### analyse

```sh
sh bin/strauss-rs.sh analyse path/to/file.php
```

Reads one PHP file and prints the symbols it declares and refers to. Useful when you want to see how Strauss-RS reads a file. This command takes the file as its only argument, plus `-q`.
