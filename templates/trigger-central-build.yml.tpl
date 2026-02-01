name: Trigger Central Build
on:
  push:
    branches: [ main, resukisu, mksu, ksu ]
jobs:
  trigger:
    runs-on: ubuntu-latest
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