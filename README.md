# Kokuban Kernel CI Center

## 项目概述

Kokuban Kernel CI Center 是一个专为 Android Linux 内核编译设计的集中式持续集成与交付（CI/CD）平台。该项目旨在通过标准化的构建流程，解决多设备、多分支内核维护中的重复性工作问题。

核心架构采用 **Rust** 编写的 CLI 工具（`kokuban_ci_core`）作为逻辑中枢，配合 **GitHub Actions** 进行流程编排，实现了从源码同步、工具链配置、KernelSU 集成到最终构建发布的完全自动化。

## 核心特性

* **集中化构建编排**：通过统一的 Rust 核心程序管理所有构建逻辑，替代了传统的碎片化 Shell 脚本，确保了构建过程的类型安全与逻辑严密性。
* **多设备与多架构支持**：支持通过配置文件定义不同设备的构建参数（Defconfig、工具链、源码仓库），目前已适配 Samsung Galaxy S23/S24/S25 系列及 Tab S10 等设备。
* **自动化 KernelSU 集成**：内置对多种 KernelSU 变体（Official KSU, MKSU, ReSukiSU）的自动补丁与集成支持，可根据分支策略自动选择集成方式（Built-in 或 LKM）。
* **智能工具链管理**：支持从远程 URL 自动下载、校验并解压编译工具链，兼容分卷压缩格式，并自动配置交叉编译环境变量（CLANG, GCC, Binutils）。
* **发布工作流闭环**：构建完成后自动打包 AnyKernel3 刷机包，推送到对应的 GitHub Releases 页面，并通过 Telegram Bot API 发送详细的发布通知。

## 系统架构

本项目由以下核心组件构成：

1.  **CI Core (Rust)**: 位于 `ci_core_rs/` 目录。负责解析 `projects.json` 配置、生成 GitHub Actions 构建矩阵、执行具体的编译指令（Make）、处理 AnyKernel3 打包以及执行发布通知。
2.  **配置中心**: 位于 `configs/` 目录。`projects.json` 定义了所有受管项目的元数据；`upstream_commits.json` 用于追踪上游变更。
3.  **工作流 (Workflows)**: 位于 `.github/workflows/` 目录。定义了 CI 的触发条件（手动触发、定时触发、上游变更触发）并调用 CI Core 执行实际任务。

## 支持设备列表

当前配置文件 (`configs/projects.json`) 已包含以下设备支持：

| 代号 | 设备型号 (SoC) | 对应项目 Key |
| :--- | :--- | :--- |
| **Z5** | Galaxy Z Fold/Flip 5 (SM8550) | `z5_sm8550` |
| **S23** | Galaxy S23 Series (SM8550) | `s23_sm8550` |
| **S24** | Galaxy S24 Series (SM8650) | `s24_sm8650` |
| **S25** | Galaxy S25 Series (SM8750) | `s25_sm8750` |
| **Tab S10** | Galaxy Tab S10 (MT6989) | `tabs10_mt6989` |

## 构建与使用

### 1. 手动触发构建

在 GitHub Actions 页面选择 "Build Kernel" 工作流，并配置以下参数：

* **Select Project**: 选择目标设备（如 `s23_sm8550`）。
* **Git Branch/Tag**: 指定内核源码的分支。
* **KSU Variant Override**: 选择 KernelSU 变体（默认为 `default`，即跟随分支策略）。
* **Create Release**: 是否在构建成功后创建 GitHub Release。

### 2. 本地开发与调试

核心逻辑可独立运行。在配置好 Rust 环境及相关依赖（`repo`, `git`, `make` 等）后，可通过以下命令调试：

```bash
# 解析项目配置
cargo run --bin kokuban_ci_core -- parse --project s23_sm8550

# 执行构建流程 (需自行准备环境)
cargo run --bin kokuban_ci_core -- build --project s23_sm8550 --branch main --do-release false