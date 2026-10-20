---
title: strauss.json configuration reference
description: "Every strauss.json setting with its type and default: prefixes, target directory, packages, autoloader options, exclusions and call site updates."
---

# Configuration

Settings go in a `strauss.json` next to your `composer.json`. Every setting is optional. Unknown keys are an error, so a typo is reported instead of ignored.

Add the schema to get completion in your editor:

```json
{
	"$schema": "./bin/strauss.schema.json"
}
```

The installer script keeps a copy of the schema in `bin/`. The schema is also [published with this site](/strauss.schema.json).

To see every setting with its default or inferred value filled in, run:

```sh
sh bin/strauss-rs.sh config
```

A complete example:

```json
{
	"$schema": "./bin/strauss.schema.json",
	"target_directory": "vendor-prefixed",
	"namespace_prefix": "Acme\\Plugin\\Vendor",
	"classmap_prefix": "Acme_Plugin_",
	"constant_prefix": "ACME_PLUGIN_",
	"packages": ["guzzlehttp/guzzle", "monolog/monolog"],
	"update_call_sites": true,
	"include_autoloader": true,
	"delete_vendor_packages": true,
	"exclude_presets": ["wordpress", "php-constants"],
	"exclude_from_prefix": {
		"packages": ["psr/log"],
		"functions": ["dd"]
	}
}
```

## Where things go

### target_directory

|         |                   |
| ------- | ----------------- |
| Type    | string            |
| Default | `"vendor-prefixed"` |

The folder, relative to the project, that the prefixed files are copied into. Leading and trailing slashes are ignored. It must not contain `..`.

### packages

|         |                                  |
| ------- | -------------------------------- |
| Type    | array of package names           |
| Default | the `require` list in `composer.json` |

The packages to process, for example `["guzzlehttp/guzzle"]`. Everything those packages require is processed as well. If you leave it out or leave it empty, every package in the `require` section of your `composer.json` is used. Packages in `require-dev` are not.

Packages that are listed but not installed are skipped with a warning. Run `composer install` first.

### packages_includes_dev

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `false` |

When `packages` is left out or empty, also process every package in the `require-dev` section of your `composer.json`, in addition to `require`. It has no effect if you list `packages` yourself.

## Prefixes

### namespace_prefix

|         |        |
| ------- | ------ |
| Type    | string |
| Default | worked out from `composer.json` |

The text put in front of every namespace, for example `"Acme\\Plugin\\Vendor"`, which turns `GuzzleHttp\Client` into `Acme\Plugin\Vendor\GuzzleHttp\Client`. Remember to double the backslashes in JSON.

If you leave it out, it comes from the first `psr-4` namespace in your `composer.json` (then `psr-0`, then the package name). The `init` command adds `\Vendor` to that, so your dependencies cannot meet your own classes. When the setting is left out entirely, the inferred value has no `\Vendor`, so set it explicitly if you want one.

### classmap_prefix

|         |        |
| ------- | ------ |
| Type    | string |
| Default | worked out from `composer.json` |

The text put in front of classes, interfaces, traits and enums that have no namespace, for example `"Acme_Plugin_"`, which turns `Parsedown` into `Acme_Plugin_Parsedown`. When it is inferred, it ends in an underscore.

### constant_prefix

|         |        |
| ------- | ------ |
| Type    | string |
| Default | none   |

The text put in front of constants in the global namespace, for example `"ACME_PLUGIN_"`. If you leave it out or leave it empty, constants are not renamed.

### functions_prefix

|         |                            |
| ------- | -------------------------- |
| Type    | string or `false`          |
| Default | `classmap_prefix` in lower case |

The text put in front of functions in the global namespace. Set it to `false` or an empty string to leave functions alone. `function_prefix` is an older name for the same setting and still works, but is deprecated.

### namespace_replacement_patterns

|         |                    |
| ------- | ------------------ |
| Type    | object             |
| Default | `{}`               |

Rules to use instead of `namespace_prefix` for some namespaces. They are tried in order and the first one that changes a namespace wins.

A key is plain text, unless it starts and ends with the same delimiter character (for example `~^Acme\\(.*)~`), in which case it is a regular expression. A value can refer to capture groups with `$1`.

```json
{
	"namespace_replacement_patterns": {
		"~^Acme\\Lib\\(.*)~": "Custom\\Lib\\$1"
	}
}
```

## Your own code

### update_call_sites

|         |                              |
| ------- | ---------------------------- |
| Type    | boolean or array of paths    |
| Default | `false`                      |

Rewrites your own PHP files where they use prefixed names. `true` uses the files and folders in your project's `autoload` section. An array lists the files and folders yourself, relative to the project. `false` changes nothing. It changes your files in place, so read [updating your own code](/guide/updating-your-code) first.

Can be overridden for one run with `--updateCallSites`.

### include_root_autoload

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `false` |

Include your project's own `autoload` section when the prefixed autoloader is written. Only the `files` entries are included.

### exclude_files_from_update

|         |                                              |
| ------- | -------------------------------------------- |
| Type    | object with `packages`, `namespaces`, `file_patterns` |
| Default | empty                                        |

Files matching these are not changed when `update_call_sites` is on.

## The autoloader

### optimize_autoloader

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `true`  |

Write a classmap-authoritative autoloader. Set it to `false` to fall back to each package's PSR-4 folders for classes that are not in the classmap. See [the autoloader](/guide/autoloader#classmap-or-fallback).

### classmap_output

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | worked out |

Write `autoload-classmap.php` and `autoload.php` into the target directory. If you leave it out, it is `false` when the target directory is already part of your project's `autoload`, and `true` otherwise.

### include_autoloader

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `false` |

After prefixing, add a line to `vendor/autoload.php` that loads the prefixed packages' autoloader, the same as the `include-autoloader` command. It needs the autoloader to be written (`classmap_output`). Composer rewrites `vendor/autoload.php` on install and update, so run `prefix` after those.

### override_autoload

|         |                                    |
| ------- | ---------------------------------- |
| Type    | object, keyed by package name      |
| Default | `{}`                               |

Autoload rules that replace the ones in a package's own `composer.json`. Each value is an `autoload` object with `psr-4`, `psr-0`, `classmap` and `files`. Use it when a package does not declare its autoloading properly.

```json
{
	"override_autoload": {
		"acme/library": {
			"psr-4": { "Acme\\Library\\": "lib/" }
		}
	}
}
```

## What is left alone

These are explained, with examples, on the page about [leaving things alone](/guide/exclusions).

### exclude_from_copy

|         |                                              |
| ------- | -------------------------------------------- |
| Type    | object with `packages`, `namespaces`, `file_patterns` |
| Default | empty                                        |

Packages, namespaces and files that are not copied into the target directory.

### exclude_from_prefix

|         |                                              |
| ------- | -------------------------------------------- |
| Type    | object with `packages`, `namespaces`, `file_patterns`, `functions` |
| Default | empty                                        |

Symbols declared in these packages, namespaces and files are not renamed. Where they are used is still updated.

- `packages`: package names, such as `league/flysystem`. Matched without regard to case.
- `namespaces`: matched exactly, with everything inside them. Leading and trailing backslashes are ignored.
- `file_patterns`: regular expressions matched against paths relative to `vendor/`. Wrapped in `#` unless they start and end with the same delimiter character.
- `functions`: function names that are never renamed, wherever they are declared. Matched without regard to case. A function in a namespace is written in full. Only this setting takes `functions`.

### exclude_constants

|         |                                                              |
| ------- | ------------------------------------------------------------ |
| Type    | object with `packages`, `namespaces`, `file_patterns`, `constants` |
| Default | empty                                                        |

Like `exclude_from_prefix`, but for constants only. `constants` is a list of constant names that are never renamed, such as `WP_PLUGIN_DIR`. Constant names are matched exactly, including case.

### exclude_presets

|         |                              |
| ------- | ---------------------------- |
| Type    | array                        |
| Default | `["wordpress"]`              |

Sets of symbols that something other than your dependencies provides, which are never renamed. The choices are `wordpress`, `woocommerce`, `wp-cli` and `php-constants`. Setting this replaces the default, so list `wordpress` again if you want it. `[]` turns every preset off.

### exclude_from_string_rename

|         |                          |
| ------- | ------------------------ |
| Type    | object with `functions`  |
| Default | empty                    |

Function names that are renamed where they are used in code but left unchanged where the name appears inside a string, such as the array key in `$array['value']`.

## Clean up

### delete_vendor_packages

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `false` |

Delete the original packages from `vendor/` after they have been copied. Composer's records are updated and its autoloader rebuilt. See [the autoloader](/guide/autoloader#deleting-the-originals).

### delete_vendor_files

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `false` |

Deprecated. Use `delete_vendor_packages`. This deletes only the files that were copied, and leaves any that were not.

### exclude_git_files

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `true`  |

Skip files that would not be in the package's distributed archive: the `.git` folder, files matched by `.gitignore`, and files marked `export-ignore`.

## Notes added to changed files

Each file that Strauss-RS changes gets a line in its header comment, such as `Modified by Jane Doe on 17-October-2026 using {@see https://github.com/evo-mark/strauss-rs}.`

### include_modified_date

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `true`  |

Put the date in that line.

### include_author

|         |         |
| ------- | ------- |
| Type    | boolean |
| Default | `true`  |

Put the author in that line. The author is the first name in the `authors` list of your `composer.json`. If there is none, the author is left out.

## Speed

### parallel

|         |                                  |
| ------- | -------------------------------- |
| Type    | `"auto"`, boolean or whole number |
| Default | `"auto"`                         |

How many workers read the PHP files. `"auto"`, `true` or `0` uses one per CPU. `false` or `1` uses one. A larger number uses at most that many. Can be overridden for one run with `--parallel`.
