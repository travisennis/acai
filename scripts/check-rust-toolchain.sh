#!/usr/bin/env sh
set -eu

toolchain_file="rust-toolchain.toml"
version=$(awk -F'"' '/^[[:space:]]*channel[[:space:]]*=/{ print $2; exit }' "$toolchain_file")

if [ -z "$version" ]; then
    echo "ERROR: could not read Rust channel from $toolchain_file" >&2
    exit 1
fi

fail=0

check_pinned_version() {
    file=$1
    if grep -q "toolchain: stable" "$file"; then
        echo "ERROR: $file uses floating Rust stable; expected toolchain: $version" >&2
        fail=1
    fi

    if grep -q "dtolnay/rust-toolchain@stable" "$file"
    then
        missing=$(awk '
            /dtolnay\/rust-toolchain@stable/ {
                found = 0
                for (i = 0; i < 8 && getline line; i++) {
                    if (line ~ /toolchain:/) {
                        found = 1
                        break
                    }
                    if (line ~ /^[[:space:]]*-[[:space:]]+(uses|run):/) {
                        break
                    }
                }
                if (!found) {
                    print FILENAME ":" NR
                }
            }
        ' "$file")

        if [ -n "$missing" ]; then
            echo "ERROR: $file has dtolnay/rust-toolchain@stable steps without explicit toolchain: $version" >&2
            echo "$missing" >&2
            fail=1
        fi
    fi
}

check_expected_version() {
    file=$1
    mismatches=$(grep -n "toolchain:" "$file" | grep -v "toolchain: $version" || true)
    if [ -n "$mismatches" ]; then
        echo "ERROR: $file has Rust toolchain pins that do not match $toolchain_file ($version):" >&2
        echo "$mismatches" >&2
        fail=1
    fi
}

check_pinned_version ".github/workflows/ci.yml"
check_pinned_version ".github/workflows/release.yml"
check_expected_version ".github/workflows/ci.yml"
check_expected_version ".github/workflows/release.yml"

# Scheduled checks include one deliberate exception: the MSRV compatibility job.
# All other stable Rust jobs should use the project toolchain version.
scheduled_non_msrv=$(awk '
    /^  [[:alnum:]_-]+:/ {
        job = $1
        sub(":", "", job)
    }
    /toolchain:/ && job != "msrv" {
        print FILENAME ":" NR ":" $0
    }
' .github/workflows/scheduled.yml | grep -v "toolchain: $version" || true)

if [ -n "$scheduled_non_msrv" ]; then
    echo "ERROR: scheduled workflow has non-MSRV Rust pins that do not match $toolchain_file ($version):" >&2
    echo "$scheduled_non_msrv" >&2
    fail=1
fi

awk '
    /^  [[:alnum:]_-]+:/ {
        job = $1
        sub(":", "", job)
    }
    /dtolnay\/rust-toolchain@stable/ && job != "msrv" {
        found = 0
        for (i = 0; i < 8 && getline line; i++) {
            if (line ~ "toolchain: " expected) {
                found = 1
                break
            }
            if (line ~ /^[[:space:]]*-[[:space:]]+(uses|run):/) {
                break
            }
        }
        if (!found) {
            print FILENAME ":" NR
        }
    }
' expected="$version" .github/workflows/scheduled.yml > /tmp/cake-rust-toolchain-missing.$$

if [ -s /tmp/cake-rust-toolchain-missing.$$ ]; then
    echo "ERROR: scheduled workflow has non-MSRV stable Rust steps without explicit toolchain: $version" >&2
    cat /tmp/cake-rust-toolchain-missing.$$ >&2
    fail=1
fi
rm -f /tmp/cake-rust-toolchain-missing.$$

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "Rust toolchain pins match $toolchain_file ($version)"
