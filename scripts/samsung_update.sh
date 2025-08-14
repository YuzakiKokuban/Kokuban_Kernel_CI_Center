#!/bin/bash

# =================================================================
# Kokuban Kernel CI Center - Samsung Source Update Script
# =================================================================
# This script automates the entire process of updating a Samsung
# kernel repository from an official source ZIP file. It handles
# multiple branches with different patching and integration requirements.
# =================================================================

# Exit immediately if a command exits with a non-zero status.
set -e

# --- Helper Functions ---
print_info() {
    echo -e "\n\e[1;34m[INFO]\e[0m $1"
}

print_error() {
    echo -e "\n\e[1;31m[ERROR]\e[0m $1" >&2
    exit 1
}

# --- Patching and Integration Functions ---

# Function to apply the LZ4 v1.10 patch.
apply_lz4_patch() {
    local kernel_root="$1" patch_root="$2" kmi_version_full="$3"
    local kmi_version=$(echo "$kmi_version_full" | grep -oE '[0-9]+\.[0-9]+')
    local zram_dir="$patch_root/zram"

    print_info "Applying LZ4 v1.10 patch for KMI ${kmi_version}..."
    [ ! -d "$zram_dir" ] && print_error "zram patch directory not found at $zram_dir"

    print_info "  - Overwriting include/linux/lz4.h and replacing lib/lz4/..."
    cp -f "$zram_dir/include/linux/lz4.h" "$kernel_root/include/linux/lz4.h"
    rm -rf "$kernel_root/lib/lz4" && mkdir -p "$kernel_root/lib/lz4"
    cp -r "$zram_dir/lz4/." "$kernel_root/lib/lz4/"
    git add include/linux/lz4.h lib/lz4
    git commit -m "feat: Import lz4 v1.10 library files" -m "[skip build]"

    local version_patch_file="$zram_dir/$kmi_version/lz4_1.10.0.patch"
    [ ! -f "$version_patch_file" ] && print_error "LZ4 patch for KMI $kmi_version not found at $version_patch_file"
    
    print_info "  - Applying version-specific patch: $(basename "$version_patch_file")"
    git apply "$version_patch_file"
    git commit -am "feat: Apply lz4 backport patch for KMI $kmi_version" -m "[skip build]"
}

# Function to apply the universal syscall hooks patch.
apply_syscall_patch() {
    local patch_root="$1"
    local syscall_patch_file="$patch_root/samsung/syscall_hooks.patch"

    print_info "Applying syscall hooks patch..."
    [ ! -f "$syscall_patch_file" ] && print_error "Syscall patch not found at $syscall_patch_file"

    git apply "$syscall_patch_file"
    git commit -am "feat: Apply syscall hooks for sukisu" -m "[skip build]"
}

# Function to apply the susfs patch.
apply_susfs_patch() {
    local patch_root="$1" kmi_version="$2"
    local susfs_repo_dir="$patch_root/susfs4ksu"
    
    print_info "Applying susfs patch for KMI $kmi_version..."
    
    print_info "  - Cloning susfs repository (branch: $kmi_version)..."
    rm -rf "$susfs_repo_dir"
    git clone --depth=1 --branch "$kmi_version" https://gitlab.com/simonpunk/susfs4ksu.git "$susfs_repo_dir"

    local susfs_patch_dir="$susfs_repo_dir/kernel_patches"
    [ ! -d "$susfs_patch_dir" ] && print_error "susfs kernel_patches directory not found."

    print_info "  - Copying new files (susfs.c, sus_su.c, *.h)..."
    cp "$susfs_patch_dir/fs/susfs.c" "fs/"
    cp "$susfs_patch_dir/fs/sus_su.c" "fs/"
    cp "$susfs_patch_dir/include/linux/susfs.h" "include/linux/"
    cp "$susfs_patch_dir/include/linux/sus_su.h" "include/linux/"
    cp "$susfs_patch_dir/include/linux/susfs_def.h" "include/linux/"
    git add fs/susfs.c fs/sus_su.c include/linux/susfs.h include/linux/sus_su.h include/linux/susfs_def.h
    git commit -m "feat: Add susfs source files" -m "[skip build]"

    print_info "  - Applying main susfs patch..."
    local main_patch_file=$(find "$susfs_patch_dir" -name '50_*.patch' | head -n 1)
    [ -z "$main_patch_file" ] && print_error "Main susfs patch (50_*.patch) not found."
    
    if ! git apply --3way "$main_patch_file"; then
        print_info "  - 'git apply --3way' failed. Checking for fs/namespace.c.rej..."
        if [ -f "fs/namespace.c.rej" ]; then
            print_info "  - Found 'fs/namespace.c.rej'. Attempting manual patch with .rej file..."
            patch fs/namespace.c < fs/namespace.c.rej
            rm fs/namespace.c.rej fs/namespace.c.orig
            git add fs/namespace.c
        else
            print_error "Failed to apply susfs patch and no .rej file found for automatic fixing. Manual intervention required."
        fi
    fi
    git commit -am "feat: Apply susfs main patch for KMI $kmi_version" -m "[skip build]"
}

# Function to update defconfig for sukisuultra by appending settings.
update_defconfig_for_sukisu() {
    local defconfig_path="$1"
    
    print_info "Updating defconfig for sukisuultra by appending settings..."
    
    cat <<EOF >> "$defconfig_path"

#
# Configurations for SukiSU-Ultra
#
CONFIG_KSU=y
CONFIG_KSU_SUSFS=y
CONFIG_KSU_SUSFS_HAS_MAGIC_MOUNT=y
CONFIG_KSU_SUSFS_SUS_PATH=y
CONFIG_KSU_SUSFS_SUS_MOUNT=y
CONFIG_KSU_SUSFS_AUTO_ADD_SUS_KSU_DEFAULT_MOUNT=y
CONFIG_KSU_SUSFS_AUTO_ADD_SUS_BIND_MOUNT=y
CONFIG_KSU_SUSFS_SUS_KSTAT=y
# CONFIG_KSU_SUSFS_SUS_OVERLAYFS is not set
CONFIG_KSU_SUSFS_TRY_UMOUNT=y
CONFIG_KSU_SUSFS_AUTO_ADD_TRY_UMOUNT_FOR_BIND_MOUNT=y
CONFIG_KSU_SUSFS_SPOOF_UNAME=y
CONFIG_KSU_SUSFS_ENABLE_LOG=y
CONFIG_KSU_SUSFS_HIDE_KSU_SUSFS_SYMBOLS=y
CONFIG_KSU_SUSFS_SPOOF_CMDLINE_OR_BOOTCONFIG=y
CONFIG_KSU_SUSFS_OPEN_REDIRECT=y
CONFIG_KSU_HOOK_KPROBES=n
CONFIG_KSU_MANUAL_HOOK=y
CONFIG_KSU_SUSFS_SUS_SU=n
CONFIG_KPM=y
EOF

    git add "$defconfig_path"
    git commit -m "feat: Update defconfig for sukisuultra" -m "[skip build]"
}

# Function to run the appropriate KernelSU/SukiSU setup script.
run_ksu_setup_script() {
    local ksu_type="$1"
    
    declare -A KSU_SETUP_URLS
    KSU_SETUP_URLS["ksu"]="https://raw.githubusercontent.com/tiann/KernelSU/main/kernel/setup.sh"
    KSU_SETUP_URLS["mksu"]="https://raw.githubusercontent.com/5ec1cff/KernelSU/main/kernel/setup.sh"
    KSU_SETUP_URLS["sukisuultra"]="https://raw.githubusercontent.com/SukiSU-Ultra/SukiSU-Ultra/main/kernel/setup.sh"
    local setup_url="${KSU_SETUP_URLS[$ksu_type]}"

    print_info "Running setup script for '$ksu_type' from $setup_url..."
    if [[ "$ksu_type" == "sukisuultra" ]]; then
      curl -LSs "$setup_url" | bash -s susfs-main
    else
      curl -LSs "$setup_url" | bash -
    fi
    
    git add .
    git commit -m "feat: Integrate $ksu_type via setup.sh" -m "[skip build]"
}


# --- Argument Parsing ---
while [[ "$#" -gt 0 ]]; do
    case $1 in
        --project-name) PROJECT_NAME="$2"; shift ;;
        --source-zip-url) SOURCE_ZIP_URL="$2"; shift ;;
        --kmi-version) KMI_VERSION="$2"; shift ;;
        *) print_error "Unknown parameter passed: $1";;
    esac
    shift
done

# Validate arguments
if [ -z "$PROJECT_NAME" ] || [ -z "$SOURCE_ZIP_URL" ] || [ -z "$KMI_VERSION" ]; then
    print_error "Missing required arguments: --project-name, --source-zip-url, or --kmi-version"
fi
if [ -z "$GH_TOKEN" ]; then
    print_error "Environment variable GH_TOKEN is not set."
fi

# --- Setup ---
print_info "Setting up environment..."
export TZ='Asia/Shanghai'
git config --global user.name "Yuzaki-Kokuban"
git config --global user.email "yuzakikokuban@github.com"
WORKSPACE=$(pwd)
SOURCE_DIR="$WORKSPACE/source_code"
PATCH_DIR="$WORKSPACE/patches" 

# --- Get Project Info from projects.json ---
print_info "Fetching details for project: $PROJECT_NAME"
PROJECT_JSON=$(jq -r --arg PKEY "$PROJECT_NAME" '.[$PKEY]' "$WORKSPACE/configs/projects.json")
PROJECT_REPO=$(echo "$PROJECT_JSON" | jq -r '.repo')
DEFCONFIG=$(echo "$PROJECT_JSON" | jq -r '.defconfig')

if [ -z "$PROJECT_REPO" ] || [ "$PROJECT_REPO" == "null" ]; then
    print_error "Could not find repo for project '$PROJECT_NAME' in configs/projects.json"
fi

# --- Download and Extract Source ---
print_info "Downloading and extracting Samsung source code..."
mkdir -p "$SOURCE_DIR"
cd "$SOURCE_DIR"

wget --quiet -O source.zip "$SOURCE_ZIP_URL"
unzip -q source.zip
TAR_FILE=$(find . -name "kernel_platform.tar.gz" -print -quit)
[ -z "$TAR_FILE" ] && print_error "kernel_platform.tar.gz not found in the downloaded zip."
tar -xzf "$TAR_FILE"

print_info "  - Locating kernel source directory..."
# Find the directory containing the top-level Kconfig file. This is a reliable way to identify the kernel source root.
KERNEL_SRC_ROOT=$(find . -name "Kconfig" -type f -printf '%h\n' | head -n 1)
if [ -z "$KERNEL_SRC_ROOT" ]; then
    print_error "Could not locate the kernel source root (Kconfig file not found)."
fi
cd "$KERNEL_SRC_ROOT"
KERNEL_SRC_PATH=$(pwd)
print_info "Kernel source root identified at: $KERNEL_SRC_PATH"

# --- Clone and Update Target Repository ---
print_info "Cloning target repository: $PROJECT_REPO"
cd "$WORKSPACE"
git clone "https://yuzakikokuban:$GH_TOKEN@github.com/$PROJECT_REPO.git" target_repo
cd target_repo

# --- Update main Branch ---
print_info "Updating 'main' (LKM) branch..."
git checkout main
print_info "  - Cleaning old source files..."
ls -a | grep -vE '^\.$|^\.\.$|^\.git$|^\.github$' | xargs rm -rf
print_info "  - Copying new source files..."
cp -r "$KERNEL_SRC_PATH"/. .
git add .
git commit -m "build: Update kernel source for $PROJECT_NAME" -m "From: $SOURCE_ZIP_URL" -m "[skip build]"

apply_lz4_patch "$(pwd)" "$PATCH_DIR" "$KMI_VERSION"

# --- Sync Other Branches to main ---
print_info "Syncing other branches (ksu, mksu, sukisuultra) to main..."
git checkout -B ksu main
git checkout -B mksu main
git checkout -B sukisuultra main

# --- Process ksu/mksu Branches ---
for branch in ksu mksu; do
    print_info "Processing '$branch' branch..."
    git checkout "$branch"
    run_ksu_setup_script "$branch"
done

# --- Process sukisuultra Branch ---
print_info "Processing 'sukisuultra' branch..."
git checkout sukisuultra
DEFCONFIG_FILE_PATH="arch/arm64/configs/$DEFCONFIG"
apply_syscall_patch "$PATCH_DIR"
apply_susfs_patch "$PATCH_DIR" "$KMI_VERSION"
update_defconfig_for_sukisu "$DEFCONFIG_FILE_PATH"
run_ksu_setup_script "sukisuultra"

# --- Push All Changes ---
print_info "Pushing all updated branches to remote..."
git push origin main ksu mksu sukisuultra --force

print_info "✅ Update process completed successfully!"
