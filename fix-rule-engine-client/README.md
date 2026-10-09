# dsh-rule-engine-client 配套包自动安装（profile: web）

## 背景

`dsh-rule-engine@0.6.2` 的 `cordis.patch.yml` 引用了同名配套包 `dsh-rule-engine-client`
（作者 jilian-dsh 的客户端插件：回合末裁决卡片；npm 唯一版本 `0.1.0`）。该包**不在**
rule-engine 的 dependencies/peers 里、需单独安装。缺失时：

- 本次热挂载日志已出现 `Cannot find package 'dsh-rule-engine-client'`（`fell back to restart`）；
- 若直接重启，rule-engine 进入 bundle 层后该 patch 行无法解析，可能导致启动失败。

## 方案

用官方安装通路一键安装：

```
dsh plugin --profile web add dsh-rule-engine-client@0.1.0
```

- pnpm（桌面 shim，`%DSH_HOME%\.desktop-bin`）在 profile 目录执行安装；
- dsh 自动对账：该包无 `dsh.bundle` → 保持"普通依赖"，**不进入** bundle 层，并由市场在下次启动时 shim 挂载；
- 桌面投影对普通依赖"原样保留"（`projection.mjs`：Real profile dependencies carry through unchanged），不会被下次启动清理。

**改动范围**：`profiles\web\package.json`（+1 依赖）、`profiles\web\pnpm-lock.yaml`（重解析）、
`profiles\web\node_modules\dsh-rule-engine-client`（新增）。
**不触碰**：bundles 顺序、cordis.patch.yml、harness/核心包、共享 pool、其他 generation。

## 使用

```powershell
cd D:\projects-py\DeepAhead\fix-rule-engine-client
.\apply.bat
```

预期输出：

1. `[1/7]`–`[7/7]` 逐步执行，末尾 `DONE`；
2. 中间 pnpm 会打印 `+ dsh-rule-engine-client 0.1.0`;
3. **出现以下 dsh 警告是正常的**（设计行为，非错误）：
   `dsh: warning: dsh-rule-engine-client declares no dsh.bundle — installed as a plain dependency, not a profile layer (...)`；
4. `verified: dsh-rule-engine-client@0.1.0`、`dependency recorded: 0.1.0`。

之后重启桌面端，rule-engine 的 patch 行即可解析。

## 回滚

```powershell
.\rollback.bat
```

从快照还原 `package.json` / `pnpm-lock.yaml`，删除安装目录与快照。无快照时安全无操作，重复回滚安全。

快照位置：`profiles\.generations\apply-rule-engine-client\`。

## 幂等性

- apply 重复执行：若已安装 `0.1.0` → 提示 "Already installed ... Nothing to do"（退出 0）；
- 快照已存在 → 中止并提示先运行 rollback.bat；
- apply 失败 → 保留快照，可随时 rollback 完整还原。
