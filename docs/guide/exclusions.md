---
title: Exclude packages and symbols from prefixing
description: "Leave packages, namespaces, functions, constants and files unprefixed, use the Wordpress, WooCommerce and WP-CLI presets, and see why with explain."
---

# Leaving things alone

Most of the time you want everything renamed. Sometimes you do not, and Strauss-RS has a few ways to say so. Each one is listed in full on the [configuration page](/reference/configuration). This page explains when to use which.

To find out why a symbol was or was not renamed, run `explain`. It names the setting that decided it:

```sh
sh bin/strauss-rs.sh explain dd
```

## A whole package

Use `exclude_from_prefix.packages` to copy a package without renaming anything in it:

```json
{
	"exclude_from_prefix": {
		"packages": ["symfony/polyfill-mbstring"]
	}
}
```

To leave a package out of the copy entirely, use `exclude_from_copy.packages`. This also works for namespaces and file patterns.

## A namespace or some files

```json
{
	"exclude_from_prefix": {
		"namespaces": ["Psr\\Log"],
		"file_patterns": ["~/Legacy/~"]
	}
}
```

Namespaces are matched exactly, along with everything inside them. File patterns are regular expressions matched against paths relative to `vendor/`.

## One function

```json
{
	"exclude_from_prefix": {
		"functions": ["dd"]
	}
}
```

The function is not renamed where it is declared, where it is called, or inside strings such as `function_exists('dd')`. Names are matched without regard to case, and a leading backslash is ignored. A function in a namespace is written in full, for example `Acme\Lib\helper`.

## One constant

```json
{
	"exclude_constants": {
		"constants": ["WP_PLUGIN_DIR", "ABSPATH"]
	}
}
```

`exclude_constants` also takes `packages`, `namespaces` and `file_patterns`, and they apply to constants only.

## A function name inside a string

Some functions are renamed where they are called, but their name also appears in a string for another reason, for example as an array key. `exclude_from_string_rename` leaves the string alone:

```json
{
	"exclude_from_string_rename": {
		"functions": ["value", "when"]
	}
}
```

## Presets

Some libraries declare symbols that the platform they run on already has, usually in a shim guarded by `function_exists()` or `class_exists()`. If that shim were renamed, the platform would never see it and the shim would stop doing its job. `exclude_presets` lists sets of symbols that are never renamed:

```json
{
	"exclude_presets": ["wordpress", "php-constants"]
}
```

| Preset          | Covers                                                                                                   | On by default |
| --------------- | -------------------------------------------------------------------------------------------------------- | ------------- |
| `wordpress`     | Wordpress core's global classes (`WP_Error`, `wpdb`), functions (`add_action`) and constants (`ABSPATH`) | Yes           |
| `woocommerce`   | WooCommerce's global classes (`WC_Order`), functions (`wc_get_order`) and constants (`WC_ABSPATH`)        | No            |
| `wp-cli`        | WP-CLI's global classes (`WP_CLI`, `WP_CLI_Command`, `Plugin_Command`) and constants (`WP_CLI`)           | No            |
| `php-constants` | The constants PHP and its extensions define (`MB_CASE_TITLE`, `JSON_THROW_ON_ERROR`, `E_ALL`)            | No            |

- Setting `exclude_presets` replaces the default, so list `wordpress` too if you want it alongside the others. `"exclude_presets": []` renames those symbols like any others.
- Namespaced symbols are not included, so a bundled copy of something like `WpOrg\Requests` or `Automattic\WooCommerce\Client` is still prefixed.
- PHP's own classes and functions are always left alone, whatever you set here. This includes functions that only some PHP setups provide, such as `getallheaders()`.

### php-constants

PHP's constants are not left alone by default, because a package may mean to define a constant of its own. This matters for polyfills. `symfony/polyfill-mbstring` declares `MB_CASE_TITLE` for a PHP without the mbstring extension. Without this preset, the constant is renamed to something like `ACME_MB_CASE_TITLE`, which PHP never defines, and code that uses it fails with an undefined constant error. Turn on `php-constants` if you bundle any polyfills.

The list covers the extensions of the PHP it was built with, plus a short list of extras. A constant from an extension it did not know about is still renamed, and `exclude_constants.constants` can cover it.

### woocommerce and wp-cli

Turn on `woocommerce` or `wp-cli` if your plugin extends them, so a library's own shim for their classes is not renamed.

## Files in your own project

`exclude_files_from_update` stops Strauss-RS changing files in your own code when `update_call_sites` is on. It takes `packages`, `namespaces` and `file_patterns`.
