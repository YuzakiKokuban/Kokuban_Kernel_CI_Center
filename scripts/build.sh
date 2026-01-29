#!/usr/bin/env bash
set -e

BUILD_VARIANT="${BRANCH_NAME}"

case "$BUILD_VARIANT" in
  main|lkm)
    VERSION_SUFFIX="LKM"
    ;;
  ksu)
    VERSION_SUFFIX="KSU"
    ;;
  mksu)
    VERSION_SUFFIX="MKSU"
    ;;
  resukisu|sukisuultra)
    VERSION_SUFFIX="ReSuki"
    ;;
  *)
    VERSION_SUFFIX="$BUILD_VARIANT"
    ;;
esac

if [ -f "../KernelSU/kernel/setup.sh" ]; then
    echo "KernelSU already setup."
else
    if [[ "$BUILD_VARIANT" == "resukisu" ]]; then
        curl -LSs "https://raw.githubusercontent.com/ReSukiSU/ReSukiSU/main/kernel/setup.sh" | bash -s builtin
    elif [[ "$BUILD_VARIANT" == "mksu" ]]; then
        curl -LSs "https://raw.githubusercontent.com/5ec1cff/KernelSU/main/kernel/setup.sh" | bash -
    elif [[ "$BUILD_VARIANT" == "ksu" ]]; then
        curl -LSs "https://raw.githubusercontent.com/tiann/KernelSU/main/kernel/setup.sh" | bash -
    fi
fi

if [ -n "$PROJECT_TOOLCHAIN_URLS" ]; then
    mkdir -p toolchain_download
    cd toolchain_download
    
    URLS=$(echo "$PROJECT_TOOLCHAIN_URLS" | python3 -c "import sys, json; print(' '.join(json.load(sys.stdin)))")
    
    for url in $URLS; do
        wget "$url"
    done
    
    if ls *.tar.gz.00 1> /dev/null 2>&1; then
        cat *.tar.gz.* | tar -zxf - -C ..
    elif ls *.tar.gz 1> /dev/null 2>&1; then
        for tarball in *.tar.gz; do
            tar -zxf "$tarball" -C ..
        done
    fi
    
    cd ..
    rm -rf toolchain_download
fi

if [[ "$PROJECT_EXTRA_HOST_ENV" == "true" ]]; then
    export LD_LIBRARY_PATH="$PWD/$PROJECT_TOOLCHAIN_PREFIX/build-tools/linux-x86/lib64:$LD_LIBRARY_PATH"
fi

export PATH="$PWD/$PROJECT_TOOLCHAIN_PREFIX/bin:$PATH"

if [ -n "$PROJECT_TOOLCHAIN_EXPORTS" ]; then
    EXPORTS=$(echo "$PROJECT_TOOLCHAIN_EXPORTS" | python3 -c "import sys, json; print(' '.join(json.load(sys.stdin)))")
    for path in $EXPORTS; do
        if [ -d "$PWD/$PROJECT_TOOLCHAIN_PREFIX/$path" ]; then
            export PATH="$PWD/$PROJECT_TOOLCHAIN_PREFIX/$path:$PATH"
        fi
    done
fi

export ARCH=arm64
export CLANG_TRIPLE=aarch64-linux-gnu-
export CROSS_COMPILE=aarch64-linux-gnu-
export CROSS_COMPILE_COMPAT=arm-linux-gnueabi-

if command -v ccache >/dev/null; then
    export CC="ccache clang"
    export CXX="ccache clang++"
    export CCACHE_DIR="$PWD/.ccache"
    ccache -M 5G
fi

if [[ "$PROJECT_LTO" == "thin" ]]; then
    export LTO=thin
fi

make O=out $PROJECT_DEFCONFIG

if [ -n "$PROJECT_DISABLE_SECURITY" ]; then
    DISABLE_LIST=$(echo "$PROJECT_DISABLE_SECURITY" | python3 -c "import sys, json; print(' '.join(json.load(sys.stdin)))")
    for config in $DISABLE_LIST; do
        scripts/config --file out/.config --disable "$config"
    done
fi

make O=out -j$(nproc --all)

git clone "$PROJECT_AK3_REPO" -b "$PROJECT_AK3_BRANCH" AnyKernel3
cp out/arch/arm64/boot/Image AnyKernel3/
cd AnyKernel3
zip -r9 "../$PROJECT_ZIP_NAME-$VERSION_SUFFIX.zip" *
cd ..

if [[ "$DO_RELEASE" == "true" ]]; then
    gh release create "${PROJECT_ZIP_NAME}-${VERSION_SUFFIX}-$(date +%Y%m%d)" \
        "$PROJECT_ZIP_NAME-$VERSION_SUFFIX.zip" \
        --title "$PROJECT_ZIP_NAME $VERSION_SUFFIX Build" \
        --notes "Automated build for $BUILD_VARIANT" || true
fi