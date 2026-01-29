# 由中央 CI 系统自动生成和管理
# 请勿手动修改此文件
name: Trigger Central Build
on:
  push:
    branches: [ main, resukisu, mksu, ksu ]
jobs:
  trigger:
    runs-on: ubuntu-latest
    # 新增：如果 commit message 包含 [skip ci]，则不运行此任务
    if: "!contains(github.event.head_commit.message, '[skip ci]')"
    steps:
      - name: Trigger build in kernel-ci repository
        uses: peter-evans/repository-dispatch@v3
        with:
          repository: __REPO_OWNER__/Kokuban_Kernel_CI_Center
          token: ${{ secrets.CI_TOKEN }}
          event-type: build-kernel
          client-payload: >-
            {
              "project": "__PROJECT_KEY__",
              "branch": "${{ github.ref_name }}"
            }