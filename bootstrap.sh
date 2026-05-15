#!/bin/bash

# Bootstrap Script for Setup Configuration
# Downloads and installs the complete development environment
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ZiedYousfi/myconfig/main/bootstrap.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/ZiedYousfi/myconfig/main/bootstrap.sh | bash -s -- macos
#   curl -fsSL https://raw.githubusercontent.com/ZiedYousfi/myconfig/main/bootstrap.sh | bash -s -- ubuntu
#   curl -fsSL https://raw.githubusercontent.com/ZiedYousfi/myconfig/main/bootstrap.sh | bash -s -- fedora

# set -e (Disabled to ensure script continues even if some steps fail)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Repository configuration
REPO_OWNER="ZiedYousfi"
REPO_NAME="myconfig"
GITHUB_REPO="${REPO_OWNER}/${REPO_NAME}"

# Installation directory
INSTALL_DIR="${HOME}/.setup-config"
TEMP_DIR="/tmp/setup-config-$$"

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1" >&2
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $1" >&2
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1" >&2
}

log_error() {
    echo -e "${RED}[✗]${NC} $1" >&2
}

print_banner() {
    echo -e "${CYAN}${BOLD}" >&2
    cat << "EOF" >&2
╔═══════════════════════════════════════════════════════════╗
║                                                           ║
║        Setup Configuration Bootstrap Script               ║
║                                                           ║
║        Automated Development Environment Setup            ║
║                                                           ║
╚═══════════════════════════════════════════════════════════╝
EOF
    echo -e "${NC}" >&2
}

detect_platform() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "macos"
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command -v apt-get &> /dev/null; then
            echo "ubuntu"
        elif command -v dnf &> /dev/null; then
            echo "fedora"
        elif command -v pacman &> /dev/null; then
            echo "unknown"
        else
            echo "unknown"
        fi
    elif [[ "$OSTYPE" == "msys"* ]] || [[ "$OSTYPE" == "cygwin"* ]] || [[ "$(uname -s)" == "MINGW"* ]] || [[ "$(uname -s)" == "MSYS"* ]]; then
        echo "windows"
    else
        echo "unknown"
    fi
}

prompt_platform() {
    echo -e "${BOLD}Select your platform:${NC}" >&2
    echo "" >&2
    echo "  1) macOS" >&2
    echo "  2) Ubuntu Server" >&2
    echo "  3) Fedora Everything" >&2
    echo "  4) Windows" >&2
    echo "  5) Exit" >&2
    echo "" >&2

    while true; do
        read -p "Enter your choice (1-5): " choice
        case $choice in
            1)
                echo "macos"
                return 0
                ;;
            2)
                echo "ubuntu"
                return 0
                ;;
            3)
                echo "fedora"
                return 0
                ;;
            4)
                echo "windows"
                return 0
                ;;
            5)
                log_info "Installation cancelled by user"
                exit 0
                ;;
            *)
                log_error "Invalid choice. Please enter 1, 2, 3, 4, or 5."
                ;;
        esac
    done
}

get_latest_release_tag() {
    log_info "Fetching latest release information..."

    local release_url="https://api.github.com/repos/${GITHUB_REPO}/releases/latest"
    local tag=$(curl -fsSL "$release_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

    if [ -z "$tag" ]; then
        log_warning "No releases found, will clone from main branch instead"
        echo "main"
    else
        echo "$tag"
    fi
}

download_and_extract() {
    local tag="$1"

    log_info "Creating temporary directory..."
    mkdir -p "$TEMP_DIR"
    cd "$TEMP_DIR"

    if [ "$tag" = "main" ]; then
        log_info "Downloading repository from main branch..."
        local archive_url="https://github.com/${GITHUB_REPO}/archive/refs/heads/main.zip"
    else
        log_info "Downloading release $tag..."
        local archive_url="https://github.com/${GITHUB_REPO}/releases/download/${tag}/setup-config.zip"

        # If release asset doesn't exist, fall back to archive
        if ! curl -fsSL --head "$archive_url" > /dev/null 2>&1; then
            log_warning "Release asset not found, downloading from archive..."
            archive_url="https://github.com/${GITHUB_REPO}/archive/refs/tags/${tag}.zip"
        fi
    fi

    if ! curl -fsSL -o setup-config.zip "$archive_url"; then
        log_error "Failed to download from $archive_url"
        exit 1
    fi

    log_success "Downloaded successfully"

    log_info "Extracting archive..."
    if ! unzip -q setup-config.zip; then
        log_error "Failed to extract archive"
        exit 1
    fi

    # Remove the zip file
    rm setup-config.zip

    # Find the source directory more robustly
    local extracted_dir=""

    # 1. Try to find a directory containing known platform directories (repo root markers)
    extracted_dir=$(find . -maxdepth 2 -type d \( -name "ubuntu-server" -o -name "macos" -o -name "fedora-everything" -o -name "windows" \) -exec dirname {} \; | head -n 1)

    # 2. If not found, check if there's exactly one subdirectory
    if [ -z "$extracted_dir" ] || [ "$extracted_dir" = "." ]; then
        local subdir_count=$(find . -mindepth 1 -maxdepth 1 -type d | wc -l)
        if [ "$subdir_count" -eq 1 ]; then
            extracted_dir=$(find . -mindepth 1 -maxdepth 1 -type d)
        fi
    fi

    # 3. Fall back to current temp directory
    if [ -z "$extracted_dir" ] || [ "$extracted_dir" = "." ]; then
        extracted_dir="."
    fi

    # Return absolute path
    (cd "$extracted_dir" && pwd)

    log_success "Extracted successfully" >&2
}

run_installation() {
    local platform="$1"
    local source_dir="$2"

    log_info "Preparing installation directory..."
    log_info "Source directory: $source_dir"

    # Backup existing installation if it exists
    if [ -d "$INSTALL_DIR" ]; then
        local backup_timestamp=$(date +%Y%m%d_%H%M%S)
        local backup_dir="${INSTALL_DIR}.backup.${backup_timestamp}"
        log_warning "Backing up existing installation to $backup_dir"
        mv "$INSTALL_DIR" "$backup_dir"
    fi

    # Create installation directory
    mkdir -p "$INSTALL_DIR"

    # Copy files
    if [[ "$OSTYPE" == "darwin"* ]]; then
        cp -pR "$source_dir"/. "$INSTALL_DIR/"
    else
        cp -a "$source_dir"/. "$INSTALL_DIR/"
    fi

    # Verify copy
    if [ ! -d "$INSTALL_DIR/ubuntu-server" ] && [ ! -d "$INSTALL_DIR/macos" ] && [ ! -d "$INSTALL_DIR/fedora-everything" ] && [ ! -d "$INSTALL_DIR/windows" ]; then
        log_error "File copy failed or source directory was empty. Check $source_dir"
        exit 1
    fi

    log_success "Files copied to $INSTALL_DIR"

    # Make install scripts executable
    chmod +x "$INSTALL_DIR"/*/install.sh 2>/dev/null || true

    # Run the appropriate installation script
    log_info "Starting ${platform} installation..."
    echo "" >&2

    local install_status=0

    case $platform in
        macos)
            if [ ! -f "$INSTALL_DIR/macos/install.sh" ]; then
                log_error "macOS install script not found at $INSTALL_DIR/macos/install.sh"
                exit 1
            fi
            cd "$INSTALL_DIR/macos"
            ./install.sh || install_status=$?
            ;;
        ubuntu)
            if [ ! -f "$INSTALL_DIR/ubuntu-server/install.sh" ]; then
                log_error "Ubuntu install script not found at $INSTALL_DIR/ubuntu-server/install.sh"
                exit 1
            fi
            cd "$INSTALL_DIR/ubuntu-server"
            ./install.sh || install_status=$?
            ;;
        fedora)
            if [ ! -f "$INSTALL_DIR/fedora-everything/install.sh" ]; then
                log_error "Fedora install script not found at $INSTALL_DIR/fedora-everything/install.sh"
                exit 1
            fi
            cd "$INSTALL_DIR/fedora-everything"
            if [ "${EUID:-$(id -u)}" -eq 0 ]; then
                ./install.sh || install_status=$?
            elif command -v sudo >/dev/null 2>&1; then
                sudo ./install.sh || install_status=$?
            else
                log_error "Fedora installation requires root. Re-run with sudo or install sudo first."
                exit 1
            fi
            ;;
        windows)
            if [ ! -f "$INSTALL_DIR/windows/install.ps1" ]; then
                log_error "Windows install script not found at $INSTALL_DIR/windows/install.ps1"
                exit 1
            fi
            cd "$INSTALL_DIR/windows"
            powershell.exe -ExecutionPolicy Bypass -File install.ps1 || install_status=$?
            ;;
        *)
            log_error "Unsupported platform: $platform"
            exit 1
            ;;
    esac

    if [ "$install_status" -ne 0 ]; then
        log_error "${platform} installation failed with exit code $install_status"
        exit "$install_status"
    fi
}

cleanup() {
    log_info "Cleaning up temporary files..."
    rm -rf "$TEMP_DIR"
    log_success "Cleanup complete"
}

ensure_dependencies() {
    local platform="$1"
    local missing_deps=()

    if ! command -v curl &> /dev/null; then
        missing_deps+=("curl")
    fi

    if ! command -v unzip &> /dev/null; then
        missing_deps+=("unzip")
    fi

    if [ ${#missing_deps[@]} -eq 0 ]; then
        return 0
    fi

    log_info "Missing required dependencies: ${missing_deps[*]}"

    case $platform in
        ubuntu)
            log_info "Installing missing dependencies via apt-get..."
            sudo apt-get update
            sudo apt-get install -y "${missing_deps[@]}"
            ;;
        fedora)
            log_info "Installing missing dependencies via dnf..."
            sudo dnf install -y "${missing_deps[@]}"
            ;;
        macos)
            if command -v brew &> /dev/null; then
                log_info "Installing missing dependencies via Homebrew..."
                brew install "${missing_deps[@]}"
            else
                log_error "Missing dependencies ${missing_deps[*]} and Homebrew is not installed."
                log_info "Please install Homebrew or manually install: ${missing_deps[*]}"
                exit 1
            fi
            ;;
        windows)
            log_info "Windows detected. No extra dependencies needed."
            log_info "PowerShell is pre-installed on Windows."
            ;;
        *)
            log_error "Please manually install the following dependencies: ${missing_deps[*]}"
            exit 1
            ;;
    esac
}

main() {
    print_banner

    # Check if platform was provided as argument
    local platform="${1:-}"

    if [ -z "$platform" ]; then
        # Auto-detect platform
        local detected_platform=$(detect_platform)

        if [ "$detected_platform" = "unknown" ]; then
            log_warning "Could not detect platform automatically"
            platform=$(prompt_platform)
        else
            log_info "Detected platform: $detected_platform"
            echo "" >&2
            read -p "Is this correct? (Y/n): " confirm

            if [[ "$confirm" =~ ^[Nn] ]]; then
                platform=$(prompt_platform)
            else
                platform="$detected_platform"
            fi
        fi
    else
        # Normalize platform argument
        platform=$(echo "$platform" | tr '[:upper:]' '[:lower:]')
        case "$platform" in
            macos|mac|darwin)
                platform="macos"
                ;;
            ubuntu|linux|server)
                platform="ubuntu"
                ;;
            fedora|fedora-everything)
                platform="fedora"
                ;;
            windows|win|mingw*|msys*|cygwin*)
                platform="windows"
                ;;
            *)
                log_error "Unknown platform: $platform"
                log_info "Supported platforms: macos, ubuntu, fedora, windows"
                exit 1
                ;;
        esac
    fi

    log_info "Selected platform: ${BOLD}${platform}${NC}"
    echo "" >&2

    # Ensure required dependencies are installed
    ensure_dependencies "$platform"

    # Get latest release
    local release_tag=$(get_latest_release_tag)

    # Download and extract
    local extracted_dir=$(download_and_extract "$release_tag")

    # Run installation
    run_installation "$platform" "$extracted_dir"

    # Cleanup
    cleanup

    echo "" >&2
    log_success "${BOLD}Installation complete!${NC}"
    echo "" >&2
    log_info "Configuration installed to: $INSTALL_DIR"
    log_info "Dotfiles managed from: ~/dotfiles"
    echo "" >&2
    if [ "$platform" == "windows" ]; then
        log_info "Windows setup complete!"
        log_info "GlazeWM and Zebar will start on your next login."
        log_info "WSL Ubuntu setup will run automatically if installed."
        log_info "Please restart your computer for all changes to take effect."
    else
        log_info "Please restart your terminal or run: ${CYAN}source ~/.zshrc${NC}"
    fi
    echo "" >&2
}

# Set up error handling
# trap 'log_error "An error occurred. Cleaning up..."; cleanup; exit 1' ERR

# Run main function
main "$@"
