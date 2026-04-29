# Kokuban Kernel CI Center

## 项目概述

Kokuban Kernel CI Center 是一个专为 Android Linux 内核编译设计的集中式持续集成与交付（CI/CD）平台。该项目旨在通过标准化的构建流程，解决多设备内核维护中的重复性工作问题，并将构建模式收敛到当前仍在维护的 `LKM` 与 `ReSukiSU` 两条路径。

核心架构采用 **Rust** 编写的 CLI 工具（`kokuban_ci_core`）作为逻辑中枢，配合 **GitHub Actions** 进行流程编排，实现了从源码同步、工具链配置、KernelSU 集成到最终构建发布的完全自动化。

## 核心特性

* **集中化构建编排**：通过统一的 Rust 核心程序管理所有构建逻辑，替代了传统的碎片化 Shell 脚本，确保了构建过程的类型安全与逻辑严密性。
* **多设备支持**：支持通过配置文件定义不同设备的构建参数（Defconfig、工具链、源码仓库），当前已覆盖三星与小米多个平台。
* **精简的构建模式**：围绕 `LKM` 与 `ReSukiSU` 两种模式提供自动补丁与集成支持，已移除旧的 `KSU/MKSU` 内置分支流转。
* **动态特性注入**：支持在构建阶段按需注入 `SuSFS`、`BBG` 与 `Re-Kernel`，其中 `SuSFS` 仅在 `ReSukiSU` 构建中启用。
* **智能工具链管理**：支持从远程 URL 自动下载、校验并解压编译工具链，兼容分卷压缩格式，并自动配置交叉编译环境变量（CLANG, GCC, Binutils）。
* **发布工作流闭环**：构建完成后自动打包 AnyKernel3 刷机包，推送到对应的 GitHub Releases 页面，并通过 Telegram Bot API 发送详细的发布通知。
* **上游监听**：`Watch Upstream KernelSU` 会对所有支持对应 KernelSU 变体的项目生效，并自动同步上游变更。

## 系统架构

本项目由以下核心组件构成：

1. **CI Core (Rust)**: 位于 `ci_core_rs/` 目录。负责解析 `projects.json` 配置、生成 GitHub Actions 构建矩阵、执行具体的编译指令（Make）、处理 AnyKernel3 打包以及执行发布通知。
2. **配置中心**: 位于 `configs/` 目录。`projects.json` 定义了所有受管项目的元数据；`upstream_commits.json` 用于追踪上游变更。
3. **工作流 (Workflows)**: 位于 `.github/workflows/` 目录。定义了 CI 的触发条件（手动触发、定时触发、上游变更触发）并调用 CI Core 执行实际任务。

## 分支与模式

当前结构围绕两个源码分支工作：

* `main`：默认源码分支，对应 `LKM` 构建路径。
* `resukisu`：`ReSukiSU` 专用源码分支。

工作流中的 **Build Mode Override** 目前只保留以下选项：

* `default`：跟随当前源码分支。
* `resukisu`：强制按 `ReSukiSU` 模式构建。
* `lkm`：强制按 `LKM` 模式构建。

## 支持设备列表

当前配置文件 (`configs/projects.json`) 已包含以下设备支持：

| 代号 | 设备型号 (SoC) | 对应项目 Key |
| :--- | :--- | :--- |
| **Z5** | Galaxy Z Fold/Flip 5 (SM8550) | `z5_sm8550` |
| **S23** | Galaxy S23 Series (SM8550) | `s23_sm8550` |
| **S24** | Galaxy S24 Series (SM8650) | `s24_sm8650` |
| **S25** | Galaxy S25 Series (SM8750) | `s25_sm8750` |
| **Tab S9** | Galaxy Tab S9 Series (SM8550) | `tabs9_sm8550` |
| **Tab S10** | Galaxy Tab S10 (MT6989) | `tabs10_mt6989` |
| **Z6** | Galaxy Z Fold/Flip 6 (SM8650) | `z6_sm8650` |
| **Mi17** | Xiaomi 17 Series (SM8850) | `mi17_sm8850` |

## 构建与使用

### 1. 手动触发构建

在 GitHub Actions 页面选择 "Build Kernel" 工作流，并配置以下参数：

* **Select Project**: 选择目标设备（如 `s23_sm8550`）。
* **Git Branch/Tag**: 指定内核源码分支，通常使用 `main` 或 `resukisu`。
* **Build Mode Override**: 选择构建模式（默认为 `default`，即跟随分支策略）。
* **Create Release**: 是否在构建成功后创建 GitHub Release。
* **Apply SuSFS / Apply BBG / Apply Re-Kernel**: 默认开启；其中 `SuSFS` 仅在 `ReSukiSU` 构建时真正生效。

### 2. 仓库初始化与分支整理

运行 `Setup Kernel Repos` 后，中心仓库会同步设备仓库通用文件，并额外尝试删除历史遗留的 `ksu` / `mksu` 远端分支，使设备仓库结构统一到当前的 `main` / `resukisu` 模型。

### 3. 上游监听

`Watch Upstream KernelSU` 会读取每个项目的 `supported_ksu`，对所有支持对应上游变体的项目生效；当前项目均支持 `resukisu`，以 [configs/projects.json](configs/projects.json) 为准。

### 4. 本地模式：x86-64 Linux / Ubuntu 快速编译

本地模式会自动准备内核源码，不要求当前目录已经存在 `./kernel_source`。它会在本机缓存根目录中按项目、分支与构建变体隔离工作区，方便多个设备同时保留缓存：

```text
~/.cache/kokuban-kernel-ci/
├── builds/<project>/<branch>-<variant>/kernel_source
├── downloads/toolchains
├── repos/kernels/<project>.git
├── repos/AnyKernel3
├── artifacts/<project>/<build-id>
├── logs/<project>/<build-id>.log
├── sccache
└── ccache
```

推荐先在 x86-64 Linux / Ubuntu 上安装依赖：

```bash
sudo apt-get update
sudo apt-get install -y build-essential git libncurses5-dev bc bison flex \
  libssl-dev p7zip-full lz4 cpio curl wget libelf-dev dwarves jq lld \
  pahole libdw-dev unzip zip
```

然后直接运行本地构建：

```bash
# LKM / main
./kokuban build s23_sm8550

# ReSukiSU
./kokuban build s25_sm8750 resukisu resukisu

# ReSukiSU 默认启用 SuSFS、BBG 与 Re-Kernel，如需临时关闭可显式传参
./kokuban build s25_sm8750 resukisu resukisu --no-susfs --no-bbg --no-rekernel

# 指定自定义缓存根目录
./kokuban build mi17_sm8850 resukisu resukisu --local-root ~/kokuban-local-cache
```

`./kokuban` 是仓库根目录中的轻量 CLI 包装器。第一次运行会自动编译 `kokuban_ci_core` 的 release 版，之后会直接复用 `ci_core_rs/target/release/kokuban_ci_core`。常用命令：

```bash
./kokuban list                 # 列出可用项目
./kokuban features             # 查看所有项目的 SuSFS / BBG / Re-Kernel 支持状态
./kokuban features s25_sm8750  # 查看单个项目的 SuSFS / BBG / Re-Kernel 配置
./kokuban validate             # 校验项目配置完整性
./kokuban doctor               # 检查本地依赖
./kokuban cache status         # 查看本地缓存占用
./kokuban cache prune --keep-artifacts 5 --older-than-days 14
./kokuban toolchain checksums  # 输出已缓存工具链包的 SHA-256
./kokuban local ...            # 直接透传到 kokuban_ci_core local
./kokuban core ...             # 透传到 kokuban_ci_core 原生命令
```

正式构建前可以先看计划，不会改动源码或缓存：

```bash
./kokuban plan s25_sm8750 resukisu resukisu
```

本地 CLI 默认会给构建传入 `apply_susfs=true`、`apply_bbg=true` 与 `apply_rekernel=true`。也可以写入本地默认配置：

```bash
./kokuban config show
./kokuban config set apply_susfs true
./kokuban config set apply_bbg true
./kokuban config set apply_rekernel true
./kokuban config set local_root ~/kokuban-local-cache
```

配置默认保存在 `$XDG_CONFIG_HOME/kokuban-kernel-ci/config`，如果没有设置 `XDG_CONFIG_HOME`，则使用 `~/.config/kokuban-kernel-ci/config`。
`cache` 与 `toolchain checksums` 命令会优先使用这里配置的 `local_root`，也可以通过 `--local-root` 临时覆盖。

可以把常用构建保存为预设：

```bash
./kokuban preset set daily-s25 s25_sm8750 resukisu resukisu
./kokuban preset list
./kokuban preset show daily-s25
./kokuban run daily-s25
```

本地模式默认会执行 `git fetch` 更新源码，并复用已经下载过的工具链包、内核 mirror、AnyKernel3 仓库、`sccache` / `ccache` 目录。已有缓存后，可以用离线模式跳过网络访问：

```bash
./kokuban build s23_sm8550 --offline
```

如果只想使用已有源码、不主动拉取远端更新，可使用 `--no-fetch`。如果希望清理内核源码工作区中的未跟踪文件，可使用 `--clean`。

构建完成后，产物会保留在工作区，同时归档到 `artifacts/<project>/<build-id>`，并更新 `artifacts/<project>/latest` 软链接。归档内容包含刷机包、`.config`、`vmlinux.symvers` 与本次构建日志。

每次构建还会写入 `build-info.json`，记录项目、源码提交、SuSFS / BBG / Re-Kernel 状态、主机信息、工具链缓存路径与 SHA-256。构建失败时，CLI 会从日志中提取关键错误行和最后一段日志，直接打印失败摘要。

缓存清理示例：

```bash
./kokuban cache clean artifacts
./kokuban cache clean logs
./kokuban cache clean project s23_sm8550
./kokuban cache clean all
```

工具链下载支持可选内容校验。在项目配置中添加 `toolchain_sha256` 后，缓存命中和新下载都会验证文件内容：

```json
"toolchain_sha256": {
  "https://example.com/toolchain.tar.gz": "expected_sha256_hex"
}
```

本地构建会对同一项目加锁，避免两个进程同时更新同一个 mirror 或工作区。不同项目可以并行构建。

如果构建进程被强制杀掉留下旧锁，可以清理锁或在确认安全时强制覆盖：

```bash
./kokuban cache clean locks
./kokuban build s23_sm8550 --force-lock
```

如果希望在固定 Ubuntu 环境中构建，可以使用仓库内的 `Dockerfile` 或 `.devcontainer/devcontainer.json`。脚本检查入口：

```bash
./scripts/check-shell.sh
./scripts/test-kokuban-cli.sh
```

### 5. 本地开发与调试

核心逻辑可独立运行。在配置好 Rust 环境及相关依赖（`repo`, `git`, `make` 等）后，可通过以下命令调试：

```bash
# 解析项目配置
cargo run --bin kokuban_ci_core -- parse --project s23_sm8550

# 执行构建流程 (需自行准备环境)
cargo run --bin kokuban_ci_core -- build --project s23_sm8550 --branch main --do-release false

# 执行 ReSukiSU 构建
cargo run --bin kokuban_ci_core -- build --project s25_sm8750 --branch resukisu --do-release false
