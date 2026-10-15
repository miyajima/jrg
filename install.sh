#!/bin/sh
# Install locally without sudo. Build natively if no usable binary is bundled.
set -eu

project_root=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
bin_dir=${JRG_INSTALL_DIR:-${HOME:?HOME is not set}/.local/bin}
binary=
force_build=false

usage() {
    cat <<'EOF'
Usage: sh install.sh [--bin-dir DIR] [--binary FILE | --build]

Mac / Linux installer for jrg and jrg-ai.
Default destination: $HOME/.local/bin (or JRG_INSTALL_DIR).
Use --build to compile for this machine (requires Rust 1.88+).
An incompatible bundled binary automatically falls back to a native build.
No shell profile, rg alias, or remote environment is modified.
EOF
}

die() { printf '%s\n' "jrg install: $*" >&2; exit 1; }

while [ "$#" -gt 0 ]; do
    case "$1" in
        --bin-dir)
            [ "$#" -ge 2 ] || die '--bin-dir requires a directory'
            bin_dir=$2
            shift 2
            ;;
        --binary)
            [ "$#" -ge 2 ] || die '--binary requires a file'
            binary=$2
            shift 2
            ;;
        --build) force_build=true; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; die "unknown argument: $1" ;;
    esac
done

[ -n "$bin_dir" ] || die 'installation directory is empty'
if [ -n "$binary" ] && [ "$force_build" = true ]; then
    die '--binary and --build cannot be combined'
fi
case "$(uname -s)" in
    Darwin|Linux) ;;
    *) die 'supported systems: macOS and Linux; use cargo install on other systems' ;;
esac

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$project_root/Cargo.toml" | head -n 1)
[ -n "$version" ] || die 'cannot determine package version'

usable() {
    [ -f "$1" ] && [ -x "$1" ] || return 1
    observed=$("$1" --version 2>/dev/null) || return 1
    [ "$observed" = "jrg $version" ] || return 1
    probe=$(printf 'ガッコウ\nｶﾞｯｺｳ\n' | "$1" --ai --max-tokens 128 がっこう 2>/dev/null) || return 1
    case "$probe" in
        *'matched=2 shown=2 omitted=0 clipped=0') return 0 ;;
        *) return 1 ;;
    esac
}

if [ -n "$binary" ]; then
    binary=$(CDPATH= cd -P "$(dirname "$binary")" && printf '%s/%s\n' "$(pwd)" "$(basename "$binary")")
    usable "$binary" || die 'explicit binary failed version / Japanese AI search checks'
elif [ "$force_build" = false ]; then
    for candidate in "$project_root/dist/jrg" "$project_root/target/release/jrg"; do
        if usable "$candidate"; then binary=$candidate; break; fi
    done
fi

if [ -z "$binary" ]; then
    command -v cargo >/dev/null 2>&1 || die 'native build needs Rust 1.88+; install Rust and rerun this installer'
    printf '%s\n' 'Building jrg natively for this machine...' >&2
    build_target_dir=${CARGO_TARGET_DIR:-$project_root/target}
    case "$build_target_dir" in
        /*) ;;
        *) build_target_dir=$(pwd)/$build_target_dir ;;
    esac
    # An explicit target-dir keeps the output location independent of Cargo config.
    cargo build --release --locked --manifest-path "$project_root/Cargo.toml" \
        --target-dir "$build_target_dir" --bin jrg
    binary=$build_target_dir/release/jrg
    usable "$binary" || die 'native binary failed checks; remove cross-compilation settings and retry'
fi

mkdir -p "$bin_dir"
bin_dir=$(CDPATH= cd -P "$bin_dir" && pwd)
[ ! -d "$bin_dir/jrg" ] || die 'jrg destination is a directory'
[ ! -d "$bin_dir/jrg-ai" ] || die 'jrg-ai destination is a directory'
stage_binary=
stage_wrapper=
cleanup() {
    [ -z "$stage_binary" ] || rm -f "$stage_binary"
    [ -z "$stage_wrapper" ] || rm -f "$stage_wrapper"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
stage_binary=$(mktemp "$bin_dir/.jrg-install.XXXXXX")
stage_wrapper=$(mktemp "$bin_dir/.jrg-ai-install.XXXXXX")
cp "$binary" "$stage_binary"
chmod 755 "$stage_binary"
cat > "$stage_wrapper" <<'EOF'
#!/bin/sh
set -eu
exec "$(dirname "$0")/jrg" --ai "$@"
EOF
chmod 755 "$stage_wrapper"
usable "$stage_binary" || die 'staged binary failed verification; existing installation was preserved'
mv -f "$stage_binary" "$bin_dir/jrg"
stage_binary=
mv -f "$stage_wrapper" "$bin_dir/jrg-ai"
stage_wrapper=
"$bin_dir/jrg-ai" --version
printf 'Installed: %s/jrg\nInstalled: %s/jrg-ai\n' "$bin_dir" "$bin_dir"
case ":${PATH:-}:" in
    *":$bin_dir:"*) ;;
    *) printf 'Add this directory to PATH: %s\n' "$bin_dir" ;;
esac
