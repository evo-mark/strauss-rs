---
title: Install and prefix your first project
description: "Install the Strauss-RS script, create a strauss.json, prefix your Composer dependencies and load the prefixed autoloader in a few steps."
---

# Getting started

Strauss-RS is primarily designed for Wordpress themes and plugins, but can work on any Composer-based project which has a `composer.json` file and a `vendor` folder.

## 1. Install the script

From your project's root folder (the one with `composer.json`):

```sh
mkdir -p bin
curl -fsSL -o bin/strauss-rs.sh https://raw.githubusercontent.com/evo-mark/strauss-rs/main/install.sh
sh bin/strauss-rs.sh
```

The script downloads the right release for your operating system and CPU, verifies its checksum, and keeps the program in `bin/` next to the install script. It also keeps a `bin/.gitignore` up to date so the download is not committed. Commit `bin/strauss-rs.sh` and `bin/.gitignore`.

## 2. Initialise

```sh
sh bin/strauss-rs.sh init
```

This reads your `composer.json` and writes a `strauss.json` next to it. It works out the prefixes for you and asks whether to add the Composer scripts from step 3. Add `--dry-run` to see what it would write without writing anything.

The file looks like this:

```json
{
	"$schema": "./bin/strauss.schema.json",
	"namespace_prefix": "Acme\\Plugin\\Vendor",
	"classmap_prefix": "Acme_Plugin_",
	"include_autoloader": true
}
```

- `namespace_prefix` is put in front of every dependency namespace.
- `classmap_prefix` is put in front of classes that have no namespace.
- `include_autoloader` makes Composer's `vendor/autoload.php` load the prefixed packages as well, so your plugin does not need an extra `require`.

Everything in this file is optional. Every option is listed on the [configuration page](/reference/configuration).

Init will also add necessary scripts to your `composer.json` file to ensure that everything is kept up to date for you.

```json
{
	"scripts": {
		"prefix-namespaces": ["sh bin/strauss-rs.sh prefix"],
		"post-install-cmd": ["@prefix-namespaces"],
		"post-update-cmd": ["@prefix-namespaces"]
	}
}
```

Now `composer install` and `composer update` prefix your dependencies into a `vendor-prefixed/` folder as their last step.

## 4. Run it

```sh
composer install
```

You should see a summary of what was copied and renamed. Your dependencies are now in `vendor-prefixed/`, with names like `Acme\Plugin\Vendor\GuzzleHttp\Client`.

## 5. Use the prefixed names in your code

Your own files still say `use GuzzleHttp\Client;`. Either change them by hand to the prefixed name, or let Strauss-RS do it:

```json
{
	"update_call_sites": true
}
```

This rewrites your source files in place, so commit your work first.

## 6. Delete the originals

Once you're happy that the prefixed output is working, you should delete the original vendor files to prevent both versions registering.

```json
{
	"delete_vendor_packages": true
}
```

::: warning
Be sure to test extensively at this point and make sure no code is relying on un-prefixed versions of your classes and symbols.
:::
