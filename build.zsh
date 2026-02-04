#!/usr/bin/env zsh

# Build script for code with multi-platform and multi-configuration support
# Usage: ./build.zsh platform="android" --release
#        ./build.zsh                      (default: native build)
#        ./build.zsh --release            (default platform, release mode)
#        ./build.zsh platform="android"   (Android, debug mode)

emulate -L zsh
setopt errexit nounset pipefail

typeset    SCRIPT_DIR WORKSPACE_ROOT CODE_RS_DIR
typeset -l PLATFORM BUILD_MODE
typeset -a BUILD_FLAGS CARGO_BUILD_FLAGS

# Script configuration
SCRIPT_DIR="${0:a:h}"
WORKSPACE_ROOT="${SCRIPT_DIR}"
CODE_RS_DIR="${WORKSPACE_ROOT}/code-rs"

# Build configuration variables
PLATFORM="${PLATFORM:-native}"
BUILD_MODE="debug"
BUILD_FLAGS=()
CARGO_BUILD_FLAGS=()

# Color output
typeset -r RED='\033[0;31m'
typeset -r GREEN='\033[0;32m'
typeset -r YELLOW='\033[1;33m'
typeset -r BLUE='\033[0;34m'
typeset -r NC='\033[0m'

# Helper functions
function log-info
{   print "${BLUE}ℹ${NC} $*"
}

function log-success
{print "${GREEN}✓${NC} $*"
}

function log-warn
{	print "${YELLOW}⚠${NC} $*"
}

function log-error
{	print "${RED}✗${NC} $*" >&2
}

function show-usage
{   emulate -L zsh
  	cat << 'EOF'
Build script for code - Multi-platform build orchestration

USAGE:
  ./build.zsh [OPTIONS]

OPTIONS:
  platform="<name>"   Target platform (native, android)
                      Default: native

  --release           Build in release mode (optimized, smaller size)
                      Default: debug mode

  --help              Show this help message

EXAMPLES:
  # Build for native platform in debug mode
  ./build.zsh

  # Build for Android in release mode
  ./build.zsh platform="android" --release

  # Build for native platform in release mode
  ./build.zsh --release

  # Build for Android in debug mode
  ./build.zsh platform="android"

SUPPORTED PLATFORMS:
  native              Build for macOS (host system)
  android             Build for Android aarch64 (ARM64)

EOF
}

# Parse command line arguments
function parse-args
{   emulate -L zsh

    typeset arg

    for arg in "$@"; do
        case "$arg" in
            platform=*)
                PLATFORM="${arg#platform=}"
                ;;
				
            --release)
                BUILD_MODE="release"
                CARGO_BUILD_FLAGS+=("--release")
                ;;

            --debug)
                BUILD_MODE="debug"
                ;;

            --help|-h)
                show-usage
                exit 0
                ;;

            *)
                log-error "Unknown argument: ${arg}"
                show-usage
                exit 1
                ;;
        esac
    done
}

# Validate platform
function validate-platform
{	emulate -L zsh

	case "$PLATFORM" in
        native|android) return 0 ;;
        
		*)
            log-error "Unknown platform: ${PLATFORM}"
            print     "Supported platforms: native, android"
            exit 1
            ;;
    esac
}

# Setup Android environment
function setup-android-env
{   emulate -L zsh

    log-info "Setting up Android build environment..."
    
    # Check if OpenSSL is built
    if [[ ! -d "/tmp/openssl-android-aarch64" ]]; then
        log-error "OpenSSL not found at /tmp/openssl-android-aarch64"
        log-info  "Building OpenSSL for Android with static libraries..."
        
        typeset OPENSSL_TMP NDK_ROOT TOOLCHAIN_PATH CC AR
                OPENSSL_TMP="/tmp/openssl-1.1.1w"

        if [[ ! -d "$OPENSSL_TMP" ]]; then
            cd /tmp
			curl -sSL "https://www.openssl.org/source/openssl-1.1.1w.tar.gz" ||
			wget -qO- "https://www.openssl.org/source/openssl-1.1.1w.tar.gz"  | tar  xz
        fi
        
        cd "$OPENSSL_TMP"

        NDK_ROOT="/opt/homebrew/share/android-ndk"
        TOOLCHAIN_PATH="${NDK_ROOT}/toolchains/llvm/prebuilt/darwin-x86_64"
        
        export CC="${TOOLCHAIN_PATH}/bin/aarch64-linux-android24-clang"
        export AR="${TOOLCHAIN_PATH}/bin/llvm-ar"
        export PATH="${TOOLCHAIN_PATH}/bin:${PATH}"
        
        # Clean previous builds to ensure static libraries
        make clean 2>/dev/null || true
        
        # Build with no-shared to create only static libraries
        ./Configure android-arm64 --prefix=/tmp/openssl-android-aarch64 no-shared
        make -j$(sysctl -n hw.ncpu)
        make install
        
        log-success "OpenSSL built successfully with static libraries"
    fi
    
    # Export Android-specific environment variables
    export OPENSSL_DIR="/tmp/openssl-android-aarch64"
    export OPENSSL_STATIC="1"
    export OPENSSL_LIB_DIR="${OPENSSL_DIR}/lib"
    export OPENSSL_INCLUDE_DIR="${OPENSSL_DIR}/include"
    export ANDROID_NDK_ROOT="/opt/homebrew/share/android-ndk"
    
    typeset TOOLCHAIN_PATH
            TOOLCHAIN_PATH="${ANDROID_NDK_ROOT}/toolchains/llvm/prebuilt/darwin-x86_64"
			
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${TOOLCHAIN_PATH}/bin/aarch64-linux-android24-clang"
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="${TOOLCHAIN_PATH}/bin/llvm-ar"
    
    BUILD_FLAGS+=("--target" "aarch64-linux-android")
    
    log-success "Android environment ready"
}


function setup-native-env
{	log-info "Setting up native build environment..."
    # Native builds don't need special setup
    log-success "Native environment ready"
}

# Validate environment
function validate-env
{   emulate -L zsh

    log-info "Validating environment..."

    if [[ ! -d "$CODE_RS_DIR" ]]; then
        log-error "code-rs directory not found at $CODE_RS_DIR"
        exit 1
    fi

    if ! command -v rustup &> /dev/null; then
        log-error  "rustup not found. Please install Rust."
        exit 1
    fi

    if ! command -v cargo &> /dev/null; then
        log-error  "cargo not found. Please install Rust."
        exit 1
    fi

    if [[ "$PLATFORM" == "android" ]]; then
		if [[ ${(f)"$(rustup target list)"} != *$'aarch64-linux-android (installed)'* ]]; then
            log-warn "aarch64-linux-android target not installed"
            log-info "Installing target..."
            rustup    target add aarch64-linux-android
        fi

        if [[ ! -d "/opt/homebrew/share/android-ndk" ]]; then
            log-error "Android NDK not found at /opt/homebrew/share/android-ndk"
            log-info  "Install with: brew install android-ndk"
            exit 1
        fi
    fi
    
    log-success "Environment validated"
}

# Perform the build
function perform-build
{   emulate -L zsh

	typeset OUTPUT_DIR BINARY_SIZE BINARY_TYPE
            OUTPUT_DIR="${CODE_RS_DIR}/target"
    
    if [[ "${PLATFORM}" == "android" ]]; then
        OUTPUT_DIR="${OUTPUT_DIR}/aarch64-linux-android"
    fi
    
    if [[ "${BUILD_MODE}" == "release" ]]; then
        OUTPUT_DIR="${OUTPUT_DIR}/release"
    else
        OUTPUT_DIR="${OUTPUT_DIR}/debug"
    fi
    
    log-info "Building code for ${PLATFORM} in ${BUILD_MODE} mode..."
    log-info "Output will be: ${OUTPUT_DIR}/code"
    
    cd "${CODE_RS_DIR}"
    
    # Build with appropriate flags
    log-info "Running: cargo build --bin code ${BUILD_FLAGS} ${CARGO_BUILD_FLAGS}"
    
    # For Android, ensure all environment variables are passed to cargo
    if [[ "${PLATFORM}" == "android" ]]; then
        typeset NDK_ROOT TOOLCHAIN_PATH
                NDK_ROOT="/opt/homebrew/share/android-ndk"
                TOOLCHAIN_PATH="${NDK_ROOT}/toolchains/llvm/prebuilt/darwin-x86_64"
        
        log-info "Using OpenSSL: ${OPENSSL_DIR}"
        if ! env                                                                                            \
            PATH="${TOOLCHAIN_PATH}/bin:${PATH}"                                                            \
            CC="${TOOLCHAIN_PATH}/bin/aarch64-linux-android24-clang"                                        \
            AR="${TOOLCHAIN_PATH}/bin/llvm-ar"                                                              \
            OPENSSL_DIR="${OPENSSL_DIR}"                                                                    \
            OPENSSL_STATIC="1"                                                                              \
            OPENSSL_LIB_DIR="${OPENSSL_LIB_DIR}"                                                            \
            OPENSSL_INCLUDE_DIR="${OPENSSL_INCLUDE_DIR}"                                                    \
            CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${TOOLCHAIN_PATH}/bin/aarch64-linux-android24-clang" \
            CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="${TOOLCHAIN_PATH}/bin/llvm-ar"                           \
            rustup run 1.90.0 cargo build                                                                   \
            --bin code                                                                                      \
            $BUILD_FLAGS                                                                                    \
            $CARGO_BUILD_FLAGS; then
            	log-error "Build failed"
            	exit 1
        fi
    else
        if ! rustup run 1.90.0 cargo build \
            --bin code                     \
            $BUILD_FLAGS                   \
            $CARGO_BUILD_FLAGS; then
            log-error "Build failed"
            exit 1
        fi
    fi
    
    # Verify output
    if [[ ! -f "${OUTPUT_DIR}/code" ]]; then
        log-error "Binary not found at ${OUTPUT_DIR}/code"
        exit 1
    fi
    
    # Get binary info
    BINARY_SIZE=${${"$(ls -lh --  "${OUTPUT_DIR}/code")"}[(w)5]}
    BINARY_TYPE=${"$(file     --  "${OUTPUT_DIR}/code")"#*:}
    
    log-success "Build completed successfully!"
    log-info "Binary size: ${BINARY_SIZE}"
    log-info "Binary type: ${BINARY_TYPE}"
    
    # Show platform-specific next steps
    case "${PLATFORM}" in
        android)
            print
            log-info "Android binary ready for deployment to Termux:"
            log-info "  adb push '${OUTPUT_DIR}/code' /data/data/com.termux/files/usr/bin/code"
            log-info "  adb shell chmod +x /data/data/com.termux/files/usr/bin/code"
            log-info "  adb shell code --version"
            ;;

        native)
            print
            log-info "Native binary ready:"
            log-info "  ${OUTPUT_DIR}/code"
    esac
}

function main
{	emulate -L zsh

	log-info "Code build system"
    
    parse-args "$@"
    
    validate-platform
    log-info "Platform:   ${PLATFORM}"
    log-info "Build mode: ${BUILD_MODE}"
    
    validate-env
    
    case "${PLATFORM}" in
        android) setup-android-env ;;
        native)  setup-native-env  ;;

    esac
    
    perform-build
}

main "$@"
