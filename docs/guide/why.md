---
title: Why prefix Composer dependencies in Wordpress
description: "Two Wordpress plugins that bundle different versions of the same library can break each other. See how namespace prefixing stops the clash."
---

# Why prefix?

Say two plugins bundle different versions of the same library. Wordpress loads both into one PHP process, so whichever autoloader registers first wins and the other plugin gets the wrong version.

```
wp-content/plugins/
├── plugin-a/
│   ├── plugin-a.php
│   └── vendor/
│       └── guzzlehttp/guzzle/     # v6.5, declares GuzzleHttp\Client
└── plugin-b/
    ├── plugin-b.php
    └── vendor/
        └── guzzlehttp/guzzle/     # v7.8, also declares GuzzleHttp\Client
```

Plugin B can end up calling v6.5 code it was never tested with, or PHP can stop with a "cannot redeclare class" error. Which one you get depends on plugin load order.

Strauss-RS copies each dependency into a folder of its own and renames its namespaces and global symbols with a prefix that belongs to your plugin. After running it in each plugin, every copy lives under its own name:

```
wp-content/plugins/
├── plugin-a/
│   ├── plugin-a.php
│   └── vendor-prefixed/
│       └── guzzlehttp/guzzle/     # Acme\PluginA\Vendor\GuzzleHttp\Client (v6.5)
└── plugin-b/
    ├── plugin-b.php
    └── vendor-prefixed/
        └── guzzlehttp/guzzle/     # Acme\PluginB\Vendor\GuzzleHttp\Client (v7.8)
```

The two copies no longer share a name, so they no longer clash.

## What gets renamed

- Namespaces: `GuzzleHttp\Client` becomes `Acme\PluginA\Vendor\GuzzleHttp\Client`.
- Classes, interfaces, traits and enums in the global namespace: `Parsedown` becomes `Acme_PluginA_Parsedown`.
- Global functions: `mb_helper()` becomes `acme_plugina_mb_helper()`.
- Constants, if you set a `constant_prefix`.

Every place that refers to one of those names is updated too: `use` statements, type hints, docblocks, and class names written inside strings.

## Where it comes from

Strauss-RS is a port of [Strauss](https://github.com/BrianHenryIE/strauss) by Brian Henry, which is itself a fork of Mozart. It is written in Rust and runs as a single binary, so it needs no PHP to run. It reads the same kinds of settings Strauss PHP does, with a few differences listed on the [migration page](./migrating).
