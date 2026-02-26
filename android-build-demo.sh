#!/usr/bin/env zsh
# Android Build Demo - Shows how to build for Android ARM v8

emulate -L zsh
setopt err_exit pipe_fail

print "Android Cross-Compilation Setup Demo"
print "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
print ""

# 1. Check if NDK is installed
print "1. Checking for Android NDK installation..."
print ""

# Function to detect NDK
function detect_ndk {
    typeset -a locations
    typeset loc
    
    # Check environment first
    if [[ -n ${ANDROID_NDK:-} && -d $ANDROID_NDK ]]; then
        print "OK: Found ANDROID_NDK env var: $ANDROID_NDK"
        return 0
    fi
    
    # Common locations
    locations=(
        ~/Android/Sdk/ndk/27.*(-/N)
        ~/Android/Sdk/ndk/26.*(-/N)
        ~/Android/ndk/*(-/N)
        ~/android-ndk-r*(-/N)
        /opt/android-ndk
        /usr/local/android-ndk
    )
    
    for loc in $locations; do
        if [[ -d $loc/toolchains/llvm/prebuilt ]]; then
            print "OK: Found NDK at: $loc"
            print -rn -- $loc
            return 0
        fi
    done
    
    print "ERROR: NDK not found in standard locations:"
    print "   • ~/Android/Sdk/ndk/<version>"
    print "   • ~/Android/ndk/<version>"
    print "   • /opt/android-ndk"
    return 1
}

ndk_path=$(detect_ndk)
if [[ -z $ndk_path ]]; then
    print ""
    print "How to install Android NDK:"
    print ""
    print "   Option 1: Using Android Studio"
    print "   • Open Android Studio"
    print "   • Go to SDK Manager → SDK Tools"
    print "   • Install 'NDK (Side by side)' version 26+"
    print ""
    print "   Option 2: Manual download"
    print "   • Visit: https://developer.android.com/ndk/downloads"
    print "   • Extract to: ~/Android/Sdk/ndk/<version>"
    print ""
    print "   Option 3: Using homebrew (macOS)"
    print "   • brew install android-ndk"
    print ""
    print "   Option 4: Set ANDROID_NDK env var"
    print "   • export ANDROID_NDK=/path/to/ndk"
    print ""
    exit 1
fi

print ""
print "2. Verifying NDK tools..."
print ""

typeset host_os linker ar
case ${OSTYPE} in
    darwin*) host_os="darwin-x86_64" ;;
    linux*)  host_os="linux-x86_64" ;;
    *)       print "ERROR: Unsupported OS: ${OSTYPE}"; exit 1 ;;
esac

prebuilt_dir="${ndk_path}/toolchains/llvm/prebuilt/${host_os}"
linker="${prebuilt_dir}/bin/aarch64-linux-android24-clang"
ar="${prebuilt_dir}/bin/llvm-ar"

print "Host OS: $host_os"
print "NDK prebuilt path: $prebuilt_dir"
print ""

if [[ ! -x $linker ]]; then
    print "ERROR: Linker not found: $linker"
    exit 1
fi
print "OK: Linker found: ${linker:t}"

if [[ ! -x $ar ]]; then
    print "ERROR: ar tool not found: $ar"
    exit 1
fi
print "OK: ar tool found: ${ar:t}"

print ""
print "3. Checking Rust target installation..."
print ""

if ! rustup target list | grep -q "aarch64-linux-android (installed)"; then
    print "Rust target not installed, installing..."
    rustup target add aarch64-linux-android
    print "OK: Rust target installed"
else
    print "OK: Rust target already installed"
fi

print ""
print "4. Build commands for Android:"
print ""
print "   # Simple build"
print "   ./build-fast.zsh --target android"
print ""
print "   # With explicit NDK path"
print "   ./build-fast.zsh --target android --android-ndk ~/Android/Sdk/ndk/27.0.12077973"
print ""
print "   # Environment variable approach"
print "   export ANDROID_NDK=~/Android/Sdk/ndk/27.0.12077973"
print "   ./build-fast.zsh --target android"
print ""
print "   # Build and transfer to device"
print "   ./build-fast.zsh --target android"
print "   adb push ./target/aarch64-linux-android/dev/code /data/data/com.termux/files/home/code"
print ""

print "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
print "Ready to build for Android!"
print ""
print "Run: ./build-fast.zsh --target android"
print ""
