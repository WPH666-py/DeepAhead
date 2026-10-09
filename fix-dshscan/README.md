# dshscan 依赖不匹配修复（profile: web）

## 诊断结论（2026-09-08，只读证据采集）

| 项目 | 结论 |
|---|---|
| 身份 | 当前 Agent 即运行在该 profile 的 harness 实例内（`$DSH_HOME`、进程 `dsh ... web --port 63459` 均指向 `profiles\web`、`dsh-desktop`）→ 全程只读分析，修改由外部终端执行 |
| 唯一问题 | 桌面安装器 `install-compat` 警告（warn 级）：`@shaoshi/dshscan: introduced host-compatibility risks — @deepseek-ai/dsh-tools@0.1.0-rc.6 vs 0.1.2-alpha.1`（来源：`profiles\web\.dsh-market\log.ndjson` L57） |
| 根因 | `@shaoshi/dshscan@0.5.0` 声明**精确** peer `@deepseek-ai/dsh-tools@0.1.0-rc.6`（所有已发布版本均如此，npm `latest` 即 0.5.0，无修复版可升级）；宿主单例池 `profiles\node_modules` 提供 `0.1.2-alpha.1`（`dsh-base` 依赖 `^0.1.2-alpha.1`，全核心套件同版本）。`@deepseek-ai/*` 按设计不在 generation 内落地，运行时从 pool 解析 |
| 佐证 | 该 generation 自己的 `pnpm-lock.yaml` 与 `.modules.yaml` 均把 `@deepseek-ai/dsh-tools@0.1.0-rc.6` 规划在 `node_modules\@deepseek-ai\dsh-tools`（路径存在但目录为空）——安装器规划了正确布局却未交付物理文件 |
| 影响 | 非启动失败：安装 exit=0，harness.log 无加载错误，插件仅用 `defineTool`（API 兼容），当前插件实际工作正常 |
| 其他项 | bundles 顺序正确、无重复条目、cordis.patch.yml 为空、无其他依赖问题 |

## 修复（方案 A：给插件其声明的版本）

在 dshscan generation 的 `node_modules\@deepseek-ai\dsh-tools` 物理安装 `@deepseek-ai/dsh-tools@0.1.0-rc.6`（官方 tarball，本地校验包名/版本）。Node 解析从插件目录向上，先命中本地 rc.6，不再落回 pool 的 0.1.2-alpha.1 → 声明与实际一致，不匹配消除。

**不改动**：harness 应用、共享 pool、bundle 顺序、cordis.patch.yml、profile package.json、任何其他插件。

**影响/回收**：仅影响该 generation 的增量文件；重启后生效。该 generation 是桌面托管缓存，将来 dshscan 版本更新（或上游修复 peer）时会被桌面重建，本补丁随之自然回收。

## 使用

1. 在外部终端（cmd 或 PowerShell）执行：
   ```bat
   cd /d D:\projects-py\DeepAhead\fix-dshscan
   apply.bat
   ```
2. 把 apply.bat 的全部 stdout/stderr 回贴。
3. 预期输出末尾：`DONE ... verified: @deepseek-ai/dsh-tools@0.1.0-rc.6 ... Expected output: 0.1.0-rc.6`。
4. 重启桌面端让修复生效（当前运行实例不受影响）。
5. 需要回退时执行 `rollback.bat`（无快照时安全无操作；重复回滚安全）。

快照位置：`C:\Users\admin\AppData\Roaming\dsh-desktop\harness\profiles\.generations\apply-dshscan-rc6\state.json`。
