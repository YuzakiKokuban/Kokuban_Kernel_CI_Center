# Kokuban Kernel CI Center

## 项目概述

Kokuban Kernel CI Center 是一个专为 Android Linux 内核编译设计的集中式持续集成与交付（CI/CD）平台。该项目旨在通过标准化的构建流程，解决多设备内核维护中的重复性工作问题，并将构建模式收敛到当前仍在维护的 `LKM` 与 `ReSukiSU` 两条路径。

核心架构采用 **Rust** 编写的 CLI 工具（`kokuban_ci_core`）作为逻辑中枢，配合 **GitHub Actions** 进行流程编排，实现了从源码同步、工具链配置、KernelSU 集成到最终构建发布的完全自动化。

## 核心特性

* **集中化构建编排**：通过统一的 Rust 核心程序管理所有构建逻辑，替代了传统的碎片化 Shell 脚本，确保了构建过程的类型安全与逻辑严密性。
* **多设备支持**：支持通过配置文件定义不同设备的构建参数（Defconfig、工具链、源码仓库），当前已覆盖三星与小米多个平台。
* **精简的构建模式**：围绕 `LKM` 与 `ReSukiSU` 两种模式提供自动补丁与集成支持，已移除旧的 `KSU/MKSU` 内置分支流转。
* **动态特性注入**：支持在构建阶段按需注入 `SuSFS` 与 `BBG`，其中 `SuSFS` 仅在 `ReSukiSU` 构建中启用。
* **智能工具链管理**：支持从远程 URL 自动下载、校验并解压编译工具链，兼容分卷压缩格式，并自动配置交叉编译环境变量（CLANG, GCC, Binutils）。
* **发布工作流闭环**：构建完成后自动打包 AnyKernel3 刷机包，推送到对应的 GitHub Releases 页面，并通过 Telegram Bot API 发送详细的发布通知。
* **受控的上游监听**：`Watch Upstream KernelSU` 目前只对白名单项目生效，默认仅跟踪 `S25` 与 `Mi17` 的 `ReSukiSU` 更新。

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
* **Apply SuSFS / Apply BBG**: 默认开启；其中 `SuSFS` 仅在 `ReSukiSU` 构建时真正生效。

### 2. 仓库初始化与分支整理

运行 `Setup Kernel Repos` 后，中心仓库会同步设备仓库通用文件，并额外尝试删除历史遗留的 `ksu` / `mksu` 远端分支，使设备仓库结构统一到当前的 `main` / `resukisu` 模型。

### 3. 上游监听

`Watch Upstream KernelSU` 当前只对配置了 `watch_upstream_variants` 的项目生效。默认情况下，仅 `s25_sm8750` 与 `mi17_sm8850` 会跟随 `ReSukiSU` 上游更新。

### 4. 本地开发与调试

核心逻辑可独立运行。在配置好 Rust 环境及相关依赖（`repo`, `git`, `make` 等）后，可通过以下命令调试：

```bash
# 解析项目配置
cargo run --bin kokuban_ci_core -- parse --project s23_sm8550

# 执行构建流程 (需自行准备环境)
cargo run --bin kokuban_ci_core -- build --project s23_sm8550 --branch main --do-release false

# 执行 ReSukiSU 构建
cargo run --bin kokuban_ci_core -- build --project s25_sm8750 --branch resukisu --do-release false
