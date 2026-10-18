---
title: Install and update with strauss-rs.sh
description: "How the bin/strauss-rs.sh script downloads the right Strauss-RS release for your system, verifies its checksum, updates it and runs it."
---

# Installer script

`bin/strauss-rs.sh` downloads the right release for your operating system and CPU, checks its checksum, and runs it. The program is kept next to the script, and a `.gitignore` in the same folder keeps it out of your repository. Commit the script and the `.gitignore`.

```sh
sh bin/strauss-rs.sh prefix   # install or update if needed, then run prefix
sh bin/strauss-rs.sh          # only install or update
```

Without `STRAUSS_RS_VERSION`, the latest release is looked for once a day. If it cannot be reached, the version already installed is used.

## Settings

Set these as environment variables when running the script:

| Variable               | What it does                                                          |
| ---------------------- | --------------------------------------------------------------------- |
| `STRAUSS_RS_VERSION`   | Use this release tag, such as `v0.4.0`, instead of the latest.        |
| `STRAUSS_RS_DIR`       | Where the program is kept. Defaults to the folder the script is in.   |
| `STRAUSS_RS_OFFLINE`   | Set to `1` to never look for an update.                               |
| `STRAUSS_RS_GITIGNORE` | Set to `0` to leave `.gitignore` alone.                               |
| `STRAUSS_RS_REPO`      | The GitHub repository to download from. Defaults to `craigrileyuk/strauss-rs`. |
| `STRAUSS_RS_BASE_URL`  | Where releases are downloaded from.                                   |
| `STRAUSS_RS_COMPOSER`  | The command that runs Composer, when `prefix` needs to rebuild Composer's autoloader and `composer` is not on your `PATH`. |

To pin a version for everyone who works on the project, set it in your Composer script:

```json
{
	"scripts": {
		"prefix-namespaces": ["STRAUSS_RS_VERSION=v0.4.0 sh bin/strauss-rs.sh prefix"]
	}
}
```
