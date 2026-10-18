---
title: Troubleshooting prefixing problems
description: "Fix class not found errors, undefined constants and functions, stale source files and objects that stop matching between plugins after prefixing."
---

# Common problems

## Class "…\Vendor\…" not found, very early in a request

The prefixed autoloader has not been loaded yet when the code runs. This happens in code that runs before plugins, such as an `object-cache.php` drop-in or a firewall started from `wp-config.php`. Turn on [`include_autoloader`](./autoloader#loading-it) so anything that loads Composer's `vendor/autoload.php` gets the prefixed packages too, or require `vendor-prefixed/autoload.php` from that code.

## Undefined constant such as `ACME_MB_CASE_TITLE`

A polyfill declares a constant that PHP already defines, and the polyfill's copy was renamed. Turn on the [`php-constants` preset](./exclusions#php-constants).

## A function is undefined after prefixing

Some functions exist in PHP only on certain setups, and libraries ship a polyfill for the others. Strauss-RS leaves the common ones alone (`getallheaders()`, the `apache_*` functions and `fastcgi_finish_request()`, for example). For any other function that your own code or PHP expects to find under its original name, use [`exclude_from_prefix.functions`](./exclusions#one-function).

Run `explain` with the function name to see which setting decided it.

## I fixed a setting and my source files still have the wrong name

If `update_call_sites` already rewrote your files, the old name is gone from them and a new run cannot restore it. Put the files back with `git checkout`, fix the setting, and run `prefix` again. See [updating your own code](./updating-your-code#it-does-not-undo-itself).

## A theme passes a Collection and my plugin rejects it

After prefixing, `Illuminate\Support\Collection` in a theme and `Acme\Plugin\Vendor\Illuminate\Support\Collection` in your plugin are two different classes. Strauss-RS cannot make them interchangeable. Where your plugin takes objects from outside, accept `iterable`, `Traversable` or `array` and wrap the value in your own prefixed class inside your code. Do the same for values you hand back.

A `class_alias()` from the original name to the prefixed class works only if nothing else has declared the original name, so it is not safe for a library that other plugins or themes might also bundle.

## The summary says files were skipped or could not be analysed

Files that Strauss-RS cannot read are reported on stderr. Check that the file is valid PHP. `sh bin/strauss-rs.sh analyse path/to/file.php` shows what it sees in one file.
