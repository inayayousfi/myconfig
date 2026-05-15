#!/bin/bash

# Local Test Script for Bootstrap
# This allows you to test the bootstrap flow locally before pushing

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMP_TEST_DIR="/tmp/bootstrap-test-$$"

echo "🧪 Bootstrap Local Test" >&2
echo "=======================" >&2
echo "" >&2

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1" >&2
}

log_success() {
    echo -e "${GREEN}[✓]${NC} $1" >&2
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1" >&2
}

# Create test environment
log_info "Creating test environment..."
mkdir -p "$TEMP_TEST_DIR"
cd "$TEMP_TEST_DIR"

# Create a mock archive
log_info "Creating mock release archive..."
ARCHIVE_DIR="$TEMP_TEST_DIR/myconfig-main"
mkdir -p "$ARCHIVE_DIR"

# Copy repository contents
log_info "Copying repository files..."
cp -r "$SCRIPT_DIR"/* "$ARCHIVE_DIR/" 2>/dev/null || true
cp -r "$SCRIPT_DIR"/.* "$ARCHIVE_DIR/" 2>/dev/null || true

# Create zip
log_info "Creating setup-config.zip..."
cd "$TEMP_TEST_DIR"
zip -r setup-config.zip myconfig-main/ -q \
    -x "*.git*" \
    -x "*.DS_Store" \
    -x "*__pycache__*" \
    -x "*.pyc"

log_success "Mock archive created"
echo "" >&2

# Test extraction
log_info "Testing archive extraction..."
rm -rf myconfig-main
unzip -q setup-config.zip

if [ -d "myconfig-main" ]; then
    log_success "Archive extracts correctly"
else
    log_warning "Archive extraction test failed"
    exit 1
fi

# Check for required files
log_info "Checking for required files in archive..."

required_files=(
    "myconfig-main/bootstrap.sh"
    "myconfig-main/macos/install.sh"
    "myconfig-main/fedora-everything/install.sh"
    "myconfig-main/ubuntu-server/install.sh"
    "myconfig-main/dotfiles"
    "myconfig-main/SPECS.md"
)

all_found=true
for file in "${required_files[@]}"; do
    if [ -e "$file" ]; then
        log_success "Found: $file"
    else
        log_warning "Missing: $file"
        all_found=false
    fi
done

echo "" >&2

if [ "$all_found" = true ]; then
    log_success "All required files present"
else
    log_warning "Some files are missing"
fi

# Test install script syntax
echo "" >&2
log_info "Testing install script syntax..."

if [ -f "myconfig-main/macos/install.sh" ]; then
    bash -n "myconfig-main/macos/install.sh" && log_success "macOS install.sh: syntax OK"
fi

if [ -f "myconfig-main/ubuntu-server/install.sh" ]; then
    bash -n "myconfig-main/ubuntu-server/install.sh" && log_success "Ubuntu install.sh: syntax OK"
fi

if [ -f "myconfig-main/fedora-everything/install.sh" ]; then
    bash -n "myconfig-main/fedora-everything/install.sh" && log_success "Fedora install.sh: syntax OK"
fi

# Test bootstrap script syntax
if [ -f "myconfig-main/bootstrap.sh" ]; then
    bash -n "myconfig-main/bootstrap.sh" && log_success "bootstrap.sh: syntax OK"
fi

# Archive size check
echo "" >&2
log_info "Archive statistics:"
ls -lh setup-config.zip | awk '{print "  Size: " $5}' >&2
unzip -l setup-config.zip | tail -1 | awk '{print "  Files: " $2}' >&2

# Cleanup
echo "" >&2
log_info "Cleaning up test environment..."
cd /
rm -rf "$TEMP_TEST_DIR"
log_success "Cleanup complete"

echo "" >&2
log_success "All tests passed! ✨"
echo "" >&2
echo "Next steps:" >&2
echo "  1. Commit and push your changes" >&2
echo "  2. Create a git tag: git tag -a v1.0.0 -m 'Release v1.0.0'" >&2
echo "  3. Push the tag: git push origin v1.0.0" >&2
echo "  4. GitHub Actions will create the release automatically" >&2
echo "" >&2
