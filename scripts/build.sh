#!/usr/bin/env bash
set -e

if [ -f "build_vars.sh" ]; then
    source build_vars.sh
fi

if [[ "$DO_RELEASE" == "true" ]]; then
    if [ -z "$RELEASE_TAG" ] || [ -z "$FINAL_ZIP_NAME" ]; then
        exit 1
    fi
fi

if [ -f "../KernelSU/kernel/setup.sh" ]; then
    :
else
    if [[ "$BRANCH_NAME" == "resukisu" ]]; then
        curl -LSs "https://raw.githubusercontent.com/ReSukiSU/ReSukiSU/main/kernel/setup.sh" | bash -s builtin
    elif [[ "$BRANCH_NAME" == "mksu" ]]; then
        curl -LSs "https://raw.githubusercontent.com/5ec1cff/KernelSU/main/kernel/setup.sh" | bash -
    elif [[ "$BRANCH_NAME" == "ksu" ]]; then
        curl -LSs "https://raw.githubusercontent.com/tiann/KernelSU/main/kernel/setup.sh" | bash -
    fi
fi

if [ -n "$PROJECT_TOOLCHAIN_URLS" ]; then
    mkdir -p toolchain_download
    cd toolchain_download
    
    URLS=$(echo "$PROJECT_TOOLCHAIN_URLS" | python3 -c "import sys, json; print(' '.join(json.load(sys.stdin)))")
    
    for url in $URLS; do
        wget -q "$url"
    done
    
    if ls *.tar.gz.[0-9]* 1> /dev/null 2>&1; then
        cat *.tar.gz.* | tar -zxf - --warning=no-unknown-keyword -C ..
    elif ls *part_aa* 1> /dev/null 2>&1 || ls *_aa.tar.gz 1> /dev/null 2>&1 || ls *.tar.gz.aa 1> /dev/null 2>&1; then
        cat *.tar.gz | tar -zxf - --warning=no-unknown-keyword -C ..
    elif ls *.tar.gz 1> /dev/null 2>&1; then
        for tarball in *.tar.gz; do
            tar -zxf "$tarball" --warning=no-unknown-keyword -C ..
        done
    fi
    
    cd ..
    rm -rf toolchain_download
fi

TOOLCHAIN_BASE_PATH="$PWD/$PROJECT_TOOLCHAIN_PREFIX"

if [ -n "$PROJECT_TOOLCHAIN_EXPORTS" ]; then
    EXPORTS=$(echo "$PROJECT_TOOLCHAIN_EXPORTS" | python3 -c "import sys, json; print(' '.join(json.load(sys.stdin)))")
    for path in $EXPORTS; do
        if [ -d "$PWD/$PROJECT_TOOLCHAIN_PREFIX/$path" ]; then
            export PATH="$PWD/$PROJECT_TOOLCHAIN_PREFIX/$path:$PATH"
        fi
    done
fi

if [[ "$PROJECT_EXTRA_HOST_ENV" == "true" ]]; then
    LLD_COMPILER_RT="-fuse-ld=lld --rtlib=compiler-rt"
    sysroot_flags="--sysroot=$TOOLCHAIN_BASE_PATH/gcc/linux-x86/host/x86_64-linux-glibc2.17-4.8/sysroot "
    cflags="-I$TOOLCHAIN_BASE_PATH/kernel-build-tools/linux-x86/include "
    ldflags="-L $TOOLCHAIN_BASE_PATH/kernel-build-tools/linux-x86/lib64 "
    ldflags+=${LLD_COMPILER_RT}
    export LD_LIBRARY_PATH="$TOOLCHAIN_BASE_PATH/kernel-build-tools/linux-x86/lib64:$LD_LIBRARY_PATH"
    export HOSTCFLAGS="$sysroot_flags $cflags"
    export HOSTLDFLAGS="$sysroot_flags $ldflags"
fi

export ARCH=arm64
export CLANG_TRIPLE=aarch64-linux-gnu-
export CROSS_COMPILE=aarch64-linux-gnu-
export CROSS_COMPILE_COMPAT=arm-linux-gnueabi-

TARGET_SOC_NAME=$(echo "$PROJECT_KEY" | cut -d'_' -f2)

declare -a MAKE_ARGS
MAKE_ARGS+=(O=out)
MAKE_ARGS+=(ARCH=arm64)
MAKE_ARGS+=(LLVM=1)
MAKE_ARGS+=(LLVM_IAS=1)
MAKE_ARGS+=(TARGET_SOC=${TARGET_SOC_NAME})

if command -v ccache >/dev/null; then
    export CC="ccache clang"
    export CXX="ccache clang++"
    export CCACHE_DIR="$PWD/.ccache"
    ccache -M 5G
    MAKE_ARGS+=("CC=ccache clang")
else
    MAKE_ARGS+=(CC=clang)
fi

make "${MAKE_ARGS[@]}" $PROJECT_DEFCONFIG

declare -a DISABLE_CONFIGS=(
    "UH" 
    "RKP" 
    "KDP" 
    "SECURITY_DEFEX" 
    "INTEGRITY" 
    "FIVE" 
    "TRIM_UNUSED_KSYMS"
)

if [ -n "$PROJECT_DISABLE_SECURITY" ]; then
    JSON_LIST=$(echo "$PROJECT_DISABLE_SECURITY" | python3 -c "import sys, json; print(' '.join(json.load(sys.stdin)))")
    for config in $JSON_LIST; do
        DISABLE_CONFIGS+=("$config")
    done
fi

for config in "${DISABLE_CONFIGS[@]}"; do
    scripts/config --file out/.config --disable "$config"
done

if [[ "$PROJECT_LTO" == "thin" ]]; then
    scripts/config --file out/.config -e LTO_CLANG_THIN -d LTO_CLANG_FULL
elif [[ "$PROJECT_LTO" == "full" ]]; then
    scripts/config --file out/.config -e LTO_CLANG_FULL -d LTO_CLANG_THIN
fi

declare -a MAKE_ARGS_BUILD
MAKE_ARGS_BUILD=("${MAKE_ARGS[@]}")

if [ "$PROJECT_VERSION_METHOD" == "file" ]; then
    echo "${FINAL_LOCALVERSION}-g$(git rev-parse --short HEAD)" > ./localversion
else
    MAKE_ARGS_BUILD+=("LOCALVERSION=${FINAL_LOCALVERSION}")
fi

make -j$(nproc) "${MAKE_ARGS_BUILD[@]}"

if [ "$PROJECT_VERSION_METHOD" == "file" ]; then echo -n > ./localversion; fi

git clone "$PROJECT_AK3_REPO" -b "$PROJECT_AK3_BRANCH" AnyKernel3
cp out/arch/arm64/boot/Image AnyKernel3/
cd AnyKernel3

zip -r9 "../$FINAL_ZIP_NAME" . -x ".git*" -x ".github*" -x "README.md" -x "LICENSE" -x "*.gitignore"
cd ..

if [[ "$DO_RELEASE" == "true" ]]; then
    gh release create "$RELEASE_TAG" \
        "$FINAL_ZIP_NAME" \
        --title "$RELEASE_TITLE" \
        --notes "Automated build for $BRANCH_NAME" \
        --verify-tag || true
fi