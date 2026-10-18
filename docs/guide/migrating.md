---
title: Migrate from Strauss PHP
description: "Move your extra.strauss settings from composer.json into strauss.json with the migrate command, and learn what behaves differently from Strauss PHP."
---

# Moving from Strauss PHP

If you already use Strauss PHP, the `migrate` command moves your settings across for you.

```sh
sh bin/strauss-rs.sh migrate --dry-run   # show the strauss.json it would write
sh bin/strauss-rs.sh migrate
```

It moves `extra.strauss` out of `composer.json` into a new `strauss.json` and removes it from `composer.json`. Other entries under `extra` stay where they are. Nothing is written if a setting is one that Strauss-RS does not support. Use `--force` to replace an existing `strauss.json`.

It also warns about Composer scripts that still run `vendor/bin/strauss`, but it does not edit them. Change those to call `sh bin/strauss-rs.sh prefix`.

## Doing it by hand

Strauss-RS does not read `extra.strauss` in `composer.json`. Anything left there is ignored, so without the move you get the inferred defaults instead of your settings, and your files are prefixed differently from before.

Take the contents of the `extra.strauss` object and make them the whole of `strauss.json`, without the `extra` and `strauss` wrapper. The option names and values are the same. From this:

```json
{
	"name": "acme/plugin",
	"extra": {
		"strauss": {
			"target_directory": "vendor-prefixed",
			"namespace_prefix": "Acme\\Plugin\\Vendor",
			"classmap_prefix": "Acme_Plugin_"
		}
	}
}
```

to a `strauss.json` containing:

```json
{
	"target_directory": "vendor-prefixed",
	"namespace_prefix": "Acme\\Plugin\\Vendor",
	"classmap_prefix": "Acme_Plugin_"
}
```

Then delete `extra.strauss` from `composer.json` so there is one place for the settings.

## Differences to know about

- Unknown keys are an error, so a typo or an unsupported option is reported instead of ignored.
- Mozart's old keys (`dep_directory`, `dep_namespace`, `exclude_packages` and so on) and the `extra.mozart` fallback are not supported. Use the current option names.
- If you leave out `classmap_prefix`, the one worked out from your package name ends in an underscore (`Acme_My_Plugin_`). Strauss PHP leaves the underscore off. Set `classmap_prefix` yourself if your existing class names must stay the same. `migrate` does this for you when the two would differ.
- Strauss-RS renames global functions with `functions_prefix`. If a function is declared by a polyfill and also provided by PHP itself, it is left alone. See [leaving things alone](./exclusions).
- Packages that only use `files` or `classmap` autoloading have all their namespaces renamed together, so a package is never left half prefixed.
- After a run with `delete_vendor_packages`, the next run needs `composer install` first. Hooking `prefix` onto the Composer scripts does this for you.

## Things to verify

Strauss PHP projects often list the Symfony polyfill packages (`symfony/polyfill-mbstring` and similar) under `exclude_from_prefix`, because they declare constants and functions that PHP already provides. Strauss-RS has the `php-constants` preset for this. See [leaving things alone](./exclusions#php-constants).
