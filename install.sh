#!/bin/sh
# Downloads the strauss-rs release for this machine, updates it when there is a
# newer one, and runs it with the arguments given.
#
#   sh strauss-rs.sh prefix            # install or update if needed, then run
#   sh strauss-rs.sh                   # only install or update
#
# In composer.json:
#   "scripts": { "prefix-namespaces": ["sh bin/strauss-rs.sh prefix"] }
#
# Settings, as environment variables:
#   STRAUSS_RS_DIR       where the binary is kept (default: the directory of this script)
#   STRAUSS_RS_VERSION   a release tag to use, such as v0.4.0, instead of the latest
#   STRAUSS_RS_REPO      the GitHub repository (default: evo-mark/strauss-rs)
#   STRAUSS_RS_BASE_URL  where releases are downloaded from (default: https://github.com/$STRAUSS_RS_REPO/releases)
#   STRAUSS_RS_OFFLINE   set to 1 to never look for an update
#   STRAUSS_RS_CHECK     set to 1 to look for an update now, even if one was looked for today
#   STRAUSS_RS_GITIGNORE set to 0 to leave .gitignore alone
#
# The files it downloads are added to a .gitignore in the directory the binary is
# kept in (created if there isn't one), so they aren't committed. This script is.
#
# Without STRAUSS_RS_VERSION, the latest release is looked for once a day. If it
# can't be reached, the binary already installed is used. STRAUSS_RS_CHECK=1
# looks straight away (STRAUSS_RS_OFFLINE=1 still wins).

set -eu

REPO="${STRAUSS_RS_REPO:-evo-mark/strauss-rs}"
BASE_URL="${STRAUSS_RS_BASE_URL:-https://github.com/$REPO/releases}"
SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
DIR="${STRAUSS_RS_DIR:-$SCRIPT_DIR}"
CHECK_EVERY_MINUTES=1440

say() { printf 'strauss-rs: %s\n' "$*" >&2; }
fail() { say "$*"; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# ---- which build ----

detect_target() {
    os="$(uname -s)"
    machine="$(uname -m)"

    case "$os" in
        Linux)
            case "$machine" in
                x86_64 | amd64) arch=x86_64 ;;
                aarch64 | arm64) arch=aarch64 ;;
                *) fail "no build for Linux on $machine" ;;
            esac

            # The musl builds are static, so they also run where glibc is missing.
            if [ -f /etc/alpine-release ] || { have ldd && ldd --version 2>&1 | grep -qi musl; }; then
                libc=musl
            else
                libc=gnu
            fi

            TARGET="$arch-unknown-linux-$libc"
            EXT=tar.gz
            BINARY=strauss-rs
            ;;
        Darwin)
            case "$machine" in
                arm64 | aarch64) arch=aarch64 ;;
                x86_64) arch=x86_64 ;;
                *) fail "no build for macOS on $machine" ;;
            esac

            # A shell running under Rosetta reports x86_64 on an Apple Silicon Mac.
            if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
                arch=aarch64
            fi

            TARGET="$arch-apple-darwin"
            EXT=tar.gz
            BINARY=strauss-rs
            ;;
        MINGW* | MSYS* | CYGWIN*)
            case "${PROCESSOR_ARCHITEW6432:-${PROCESSOR_ARCHITECTURE:-$machine}}" in
                ARM64 | aarch64 | arm64) arch=aarch64 ;;
                *) arch=x86_64 ;;
            esac

            TARGET="$arch-pc-windows-msvc"
            EXT=zip
            BINARY=strauss-rs.exe
            ;;
        *)
            fail "no build for $os"
            ;;
    esac
}

# ---- downloading ----

download() { # url, destination
    if have curl; then
        curl -fsSL --retry 2 -o "$2" "$1"
    elif have wget; then
        wget -q -O "$2" "$1"
    else
        fail "curl or wget is needed to download"
    fi
}

# The tag of the latest release, from where GitHub redirects /releases/latest.
latest_tag() {
    if have curl; then
        url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$BASE_URL/latest")" || return 1
    elif have wget; then
        url="$(wget -q --max-redirect=0 --server-response -O /dev/null "$BASE_URL/latest" 2>&1 |
            sed -n 's/^ *[Ll]ocation: *//p' | tr -d '\r' | tail -n 1)" || return 1
    else
        return 1
    fi

    tag="${url##*/}"

    case "$tag" in
        v[0-9]*) printf '%s\n' "$tag" ;;
        *) return 1 ;;
    esac
}

sha256_of() {
    if have sha256sum; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif have shasum; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        return 1
    fi
}

extract() { # archive, directory
    case "$1" in
        *.zip)
            if have unzip; then unzip -q -o "$1" -d "$2"; else tar -xf "$1" -C "$2"; fi
            ;;
        *) tar -xzf "$1" -C "$2" ;;
    esac
}

install_release() { # tag
    tag="$1"
    name="strauss-rs-$tag-$TARGET"
    work="$(mktemp -d "${TMPDIR:-/tmp}/strauss-rs.XXXXXX")"
    trap 'rm -rf "$work"' EXIT

    say "downloading $tag for $TARGET"

    download "$BASE_URL/download/$tag/$name.$EXT" "$work/$name.$EXT" ||
        fail "could not download $BASE_URL/download/$tag/$name.$EXT"
    download "$BASE_URL/download/$tag/$name.$EXT.sha256" "$work/$name.sha256" ||
        fail "could not download the checksum for $name.$EXT"

    expected="$(cut -d ' ' -f 1 "$work/$name.sha256")"
    actual="$(sha256_of "$work/$name.$EXT")" || fail "sha256sum or shasum is needed to check the download"

    [ "$expected" = "$actual" ] || fail "the download does not match its checksum; not installing"

    mkdir "$work/unpacked"
    extract "$work/$name.$EXT" "$work/unpacked"

    [ -f "$work/unpacked/$name/$BINARY" ] || fail "$name.$EXT does not contain $BINARY"

    mkdir -p "$DIR"

    # Moved into place from beside it, so a run that is using it is not cut off.
    cp "$work/unpacked/$name/$BINARY" "$DIR/$BINARY.new"
    chmod +x "$DIR/$BINARY.new"
    mv -f "$DIR/$BINARY.new" "$DIR/$BINARY"

    if [ -f "$work/unpacked/$name/strauss.schema.json" ]; then
        cp "$work/unpacked/$name/strauss.schema.json" "$DIR/strauss.schema.json"
    fi

    rm -rf "$work"
    trap - EXIT
}

# "v0.4.0" for the installed binary, or nothing.
installed_tag() {
    [ -x "$DIR/$BINARY" ] || return 0
    version="$("$DIR/$BINARY" --version 2>/dev/null | sed -n 's/^strauss-rs //p')"
    [ -n "$version" ] && printf 'v%s\n' "$version"
    return 0
}

# The schema is in each release's archive, but a binary installed before that was
# so (or from an archive without it) would never get one, because the archive is
# only unpacked when the binary changes. So fetch it by itself when it is missing.
ensure_schema() { # tag
    [ -f "$DIR/strauss.schema.json" ] && return 0

    mkdir -p "$DIR"

    if download "$BASE_URL/download/$1/strauss.schema.json" "$DIR/strauss.schema.json.new" 2>/dev/null; then
        mv -f "$DIR/strauss.schema.json.new" "$DIR/strauss.schema.json"
    else
        rm -f "$DIR/strauss.schema.json.new"
        say "could not download strauss.schema.json for $1; \"\$schema\" in strauss.json won't resolve"
    fi
}

# Adds what this script puts in $DIR to $DIR/.gitignore, if it isn't there yet.
update_gitignore() {
    [ "${STRAUSS_RS_GITIGNORE:-1}" = 0 ] && return 0
    [ -d "$DIR" ] || return 0

    ignore="$DIR/.gitignore"

    for entry in "/$BINARY" "/.strauss-rs-checked" "/strauss.schema.json"; do
        if [ -f "$ignore" ] && grep -qxF -- "$entry" "$ignore"; then
            continue
        fi

        # A file that doesn't end in a newline would join the entry to its last line.
        if [ -s "$ignore" ] && [ -n "$(tail -c 1 "$ignore")" ]; then
            printf '\n' >>"$ignore"
        fi

        printf '%s\n' "$entry" >>"$ignore" || {
            say "could not update $ignore"
            return 0
        }
    done
}

checked_recently() {
    stamp="$DIR/.strauss-rs-checked"
    [ -f "$stamp" ] && [ -n "$(find "$stamp" -mmin "-$CHECK_EVERY_MINUTES" 2>/dev/null)" ]
}

main() {
    detect_target

    current="$(installed_tag)"
    wanted="${STRAUSS_RS_VERSION:-}"

    if [ -n "$wanted" ]; then
        case "$wanted" in v*) ;; *) wanted="v$wanted" ;; esac
    elif [ -n "$current" ] && { [ "${STRAUSS_RS_OFFLINE:-0}" = 1 ] || { [ "${STRAUSS_RS_CHECK:-0}" != 1 ] && checked_recently; }; }; then
        wanted="$current"
    else
        if wanted="$(latest_tag)"; then
            mkdir -p "$DIR" && : >"$DIR/.strauss-rs-checked"
        elif [ -n "$current" ]; then
            say "could not look for a newer release; using $current"
            wanted="$current"
        else
            fail "could not find the latest release at $BASE_URL"
        fi
    fi

    if [ "$current" != "$wanted" ]; then
        install_release "$wanted"
        say "installed $wanted in $DIR"
    fi

    ensure_schema "$wanted"
    update_gitignore

    if [ "$#" -gt 0 ]; then
        exec "$DIR/$BINARY" "$@"
    fi
}

main "$@"
