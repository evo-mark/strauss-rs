---
title: Update your own code to use prefixed names
description: "Use update_call_sites to rewrite your plugin's own PHP files to use prefixed namespaces, preview the changes with diff and avoid common pitfalls."
---

# Updating your own code

Your project's own files refer to the packages too. A line like `use Acme\Core\Client;` has to become `use Acme\Vendor\Acme\Core\Client;` once the package is prefixed. Set `update_call_sites` and `prefix` rewrites those files in place:

```json
{
	"update_call_sites": true
}
```

## Which files are changed

- `true` updates the `.php` files under the `autoload` paths in your `composer.json` (`psr-4`, `psr-0`, `classmap` and `files`). `autoload-dev` is not included.
- A list, such as `["src", "includes/loader.php"]`, updates just those files and folders, relative to the project.
- `false`, the default, changes none of your files.
- Your own namespaces are left alone. The vendor and target directories are never touched.
- `exclude_files_from_update` skips files by package, namespace or file pattern.

You can override the setting for one run with `--updateCallSites`, `--updateCallSites=false` or `--updateCallSites=src,includes`, on `prefix` and `diff`.

## Precautions

This changes your source files. Make sure you commit or backup before you run it, and look at the result first:

```sh
sh bin/strauss-rs.sh diff --updateCallSites
```

`prefix --dry-run` also reports how many files would change, without writing them.

## It does not undo itself

A rewrite stays in your files. If a name was renamed that should not have been (for example a function a package declares and PHP also provides), fixing the setting and running `prefix` again will not put the original name back, because the file no longer contains it. Use `git checkout` on those files, then fix the setting, then run `prefix` again.

Additionally, changing any of the prefixes in your config will _not_ update those prefixes in your project files. You will have to do a search and replace for this.

## Names built at run time

Strauss-RS renames names it can read in the code: `use` lines, type hints, `new Foo`, `Foo::bar()`, docblocks, and class names that appear whole inside strings. A name that is put together while the code runs, such as `'Acme\\' . $name`, is not found and is not changed. If your code does that, write the prefixed namespace into the string by hand.

Where a string that is joined to something else with `.`, or has a variable inside it, ends in the start of a renamed namespace (`'Acme\\' . $name` when `Acme\Core` is renamed), `prefix` warns with the file and line. A name built in other ways, such as `$vendor . '\\Core\\Client'`, is not reported.
