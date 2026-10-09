# DeepAhead 打包与发布流程（v0.5.5 起）

本文记录**从改代码到 Gitee / GitHub 双端发布**的完整链路，以及本机环境的两个坑。

---

## 0. 本机环境事实（先看这个）

| 项 | 现状 | 说明 |
| --- | --- | --- |
| Node / pnpm | `node v24.9.0`、`pnpm 10.34.5`（DSH 随附） | 可直接用 |
| Rust | `cargo` / `rustc` 在 `C:\Users\<用户>\.cargo\bin` | 在 PATH 里 |
| **git** | **不在 PATH**，只有便携版 `D:\projects-py\git-portable\cmd\git.exe`（2.55） | 所有 git 命令走全路径 |
| 远端 | `origin` = `github.com/WPH666-py/DeepAhead`；`gitee` = `gitee.com/ph-wang_admin/DeepAhead` | 仓库里已有双远端 |
| GitHub 凭据 | 全局 gitconfig 用 `gh.exe auth git-credential`（`D:\projects-py\Harness-CN\tools\bin\gh.exe`） | 免密可用 |
| Gitee 凭据 | `credential.provider = generic` → Windows 凭据管理器 | 免密可用（`ls-remote` 已实测） |

> ⚠️ **坑 1**：用 `npx tauri build` 时，DSH 的 pnpm 包装器会误判"pnpm 没有输出"并在 ~430s 后 SIGKILL。
> 解决办法：先手动 `pnpm build`，再用 `npx tauri build --no-bundle` 只编译二进制，最后 `npx tauri bundle`。
>
> ⚠️ **坑 2**：便携版 git 若跨盘符（`D:\...` 仓库）操作，必须先 `git config --global --add safe.directory *`（本机已配置）。

---

## 1. 改版本号（三处必须一致）

```powershell
# package.json / src-tauri/tauri.conf.json / src-tauri/Cargo.toml
$v = "0.5.5"
(Get-Content package.json -Raw) -replace '"version": "0\.5\.\d+"', "`"version`": `"$v`"" | Set-Content package.json -NoNewline
(Get-Content src-tauri\tauri.conf.json -Raw) -replace '"version": "0\.5\.\d+"', "`"version`": `"$v`"" | Set-Content src-tauri\tauri.conf.json -NoNewline
(Get-Content src-tauri\Cargo.toml -Raw) -replace 'version = "0\.5\.\d+"', "version = `"$v`"" | Set-Content src-tauri\Cargo.toml -NoNewline
```

同时写发布说明 `docs/releases/v<版本>.md`（Gitee / GitHub Release 正文直接用它）。

## 2. 本地校验（改完必跑）

```powershell
cargo test  --manifest-path src-tauri\Cargo.toml --lib   # Rust 单测
npx vue-tsc --noEmit                                     # 前端类型检查
pnpm build                                               # 前端产物
```

## 3. 打包 NSIS 安装包

```powershell
pnpm build                          # ① 前端产物（dist/）
npx tauri build --no-bundle         # ② 编译 release 二进制
npx tauri bundle --bundles nsis     # ③ 打 NSIS 安装包
```

产物位置：

```
src-tauri\target\release\DeepAhead.exe
src-tauri\target\release\bundle\nsis\DeepAhead_0.5.5_x64-setup.exe   ← 发布用
```

> 安装包文件名**必须含 `setup`**：应用内的自动更新靠这个特征挑附件
> （`src-tauri/src/update.rs` → 优先 `.exe` 且名字含 `setup`）。

## 4. 提交并推送双远端

```powershell
$git = "D:\projects-py\git-portable\cmd\git.exe"
& $git add -A
& $git commit -m "feat(approval): 执行许可三档 + 规则引擎开关由档位接管 + 裁决卡片❌放行自动继续 + 日志实时落盘，v0.5.5"
& $git tag v0.5.5
& $git push origin main --tags      # GitHub
& $git push gitee  main --tags      # Gitee
```

## 5. 发布 Release（带安装包附件）

### 5.1 Gitee（**自动更新依赖它**）

应用内「检查更新」只读 Gitee：`https://gitee.com/api/v5/repos/ph-wang_admin/DeepAhead/releases`。
所以 **Gitee 上必须创建 release 并上传安装包**，否则用户收不到更新提示。

需要一枚 Gitee 私人令牌（**只存本地，不要提交进仓库**）：
Gitee → 设置 → 私人令牌 → 生成，勾选 `projects` 权限。

```powershell
$env:GITEE_TOKEN = "<你的 Gitee 私人令牌>"
$v = "0.5.5"
$notes = Get-Content "docs\releases\v$v.md" -Raw
$body = @{
  access_token = $env:GITEE_TOKEN
  tag_name     = "v$v"
  name         = "DeepAhead v$v"
  body         = $notes
  target_commitish = "main"
} | ConvertTo-Json -Depth 4

# ① 建 release
$rel = Invoke-RestMethod -Method Post -Uri "https://gitee.com/api/v5/repos/ph-wang_admin/DeepAhead/releases" `
        -ContentType "application/json; charset=utf-8" -Body ([Text.Encoding]::UTF8.GetBytes($body))

# ② 传安装包附件（multipart，字段名必须是 file）
$setup = "src-tauri\target\release\bundle\nsis\DeepAhead_${v}_x64-setup.exe"
curl.exe -s -X POST "https://gitee.com/api/v5/repos/ph-wang_admin/DeepAhead/releases/$($rel.id)/attach_files" `
  -F "access_token=$env:GITEE_TOKEN" -F "file=@$setup"
```

### 5.2 GitHub

```powershell
# 已装 gh（D:\projects-py\Harness-CN\tools\bin\gh.exe），或设 $env:GH_TOKEN
& "D:\projects-py\Harness-CN\tools\bin\gh.exe" release create "v$v" `
  "src-tauri\target\release\bundle\nsis\DeepAhead_${v}_x64-setup.exe" `
  --repo WPH666-py/DeepAhead --title "DeepAhead v$v" --notes-file "docs\releases\v$v.md"
```

没有 `gh` 时用 REST API：

```powershell
$env:GITHUB_TOKEN = "<你的 GitHub PAT（repo 权限）>"
# 建 release 后 PUT 上传资产（upload_url 见创建响应）
```

## 6. 发布后自检

```powershell
# 接口能否读到 release 与附件（这就是应用内"检查更新"读的同一个接口）
Invoke-RestMethod "https://gitee.com/api/v5/repos/ph-wang_admin/DeepAhead/releases" |
  Select-Object -First 1 | ForEach-Object { $_.tag_name; $_.assets.name }
```

也可以直接跑被忽略的联调测试：

```powershell
cargo test --manifest-path src-tauri\Cargo.toml --lib -- --ignored live_gitee
```

---

## 7. 一页速查

```
改代码 → 三处版本号 → cargo test + vue-tsc → pnpm build
      → npx tauri build --no-bundle → npx tauri bundle --bundles nsis
      → git add/commit/tag → push origin + gitee
      → Gitee release + setup.exe 附件（自动更新用）
      → GitHub release + setup.exe 附件
```
