/**
 * DeepAhead 插件样式转换器
 * 输入 DSH 皮肤插件的 GitHub 仓库地址，自动抓取仓库中的 skin.json 与 CSS 变量，
 * 转换为 DeepAhead 的 SkinDefinition（可直接注册为自定义皮肤）。
 *
 * 需要可访问 GitHub 的网络环境（如 VPN）。全程只读公开仓库，不产生任何副作用。
 */

import {
  SkinDefinition,
  SkinPalette,
  darken,
  isLightColor,
  lighten,
} from "./skins";

export interface ConvertResult {
  skin: SkinDefinition;
  /** 转换过程中的提示（如：未找到暗色变量，已自动派生） */
  warnings: string[];
}

interface RepoRef {
  owner: string;
  repo: string;
}

// ───────────────────────── GitHub 访问 ─────────────────────────

/** 解析 GitHub 仓库地址（支持 https://github.com/o/r、带 .git、带 /tree/... 后缀） */
export function parseGitHubUrl(input: string): RepoRef {
  const m = input.trim().match(/github\.com[/:]([\w.-]+)\/([\w.-]+)/i);
  if (!m) throw new Error("无法识别的 GitHub 仓库地址，格式应为 https://github.com/owner/repo");
  return { owner: m[1], repo: m[2].replace(/\.git$/i, "") };
}

async function fetchJson(url: string): Promise<any> {
  let res: Response;
  try {
    res = await fetch(url);
  } catch {
    throw new Error("无法连接 GitHub，请检查 VPN 网络连接后重试");
  }
  if (!res.ok) {
    if (res.status === 404) throw new Error("仓库或文件不存在（404），请确认地址正确且仓库为公开仓库");
    if (res.status === 403) throw new Error("GitHub API 访问受限（403），可能是请求频率超限，请稍后再试");
    throw new Error(`GitHub 请求失败：HTTP ${res.status}`);
  }
  return res.json();
}

async function fetchText(url: string): Promise<string> {
  let res: Response;
  try {
    res = await fetch(url);
  } catch {
    throw new Error("无法连接 GitHub 原始文件，请检查 VPN 网络连接后重试");
  }
  if (!res.ok) throw new Error(`下载文件失败：HTTP ${res.status}`);
  return res.text();
}

const rawUrl = (ref: RepoRef, branch: string, path: string) =>
  `https://raw.githubusercontent.com/${ref.owner}/${ref.repo}/${branch}/${path}`;

// ───────────────────────── CSS 变量解析 ─────────────────────────

type VarMap = Record<string, string>;

/** 从 CSS 文本中提取 --变量: #颜色 定义 */
function extractVars(css: string): VarMap {
  const out: VarMap = {};
  const re = /(--[\w-]+)\s*:\s*(#[0-9a-fA-F]{3,8})\b/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(css))) out[m[1]] = m[2].toLowerCase();
  return out;
}

/** 提取暗色作用域内的变量（data-*dark* / .dark / prefers-color-scheme: dark） */
function extractDarkVars(css: string): VarMap {
  const out: VarMap = {};
  // 匹配暗色选择器块：选择器中含 dark，或 dark 媒体查询
  const blockRe = /([^{}]+)\{([^{}]*)\}/g;
  let m: RegExpExecArray | null;
  while ((m = blockRe.exec(css))) {
    const selector = m[1];
    if (/dark/i.test(selector)) Object.assign(out, extractVars(m[2]));
  }
  return out;
}

/** 按变量名关键词从变量表中找色 */
function pick(vars: VarMap, ...keywords: string[]): string | undefined {
  for (const kw of keywords) {
    for (const [k, v] of Object.entries(vars)) {
      if (k.toLowerCase().includes(kw)) return v;
    }
  }
  return undefined;
}

// ───────────────────────── 调色板推导 ─────────────────────────

/** 由提取的变量构建 DeepAhead 调色板；缺失槽位用基础色自动派生 */
function derivePalette(vars: VarMap, accent: string, warnings: string[], label: string): SkinPalette {
  // 基础色：背景 / 文字 / 强调
  const bgBase =
    pick(vars, "bg-layer-1", "bg-base", "background", "bg-100", "neutral-50") ??
    pick(vars, "neutral-00", "porcelain", "paper") ??
    "#ffffff";
  const light = isLightColor(bgBase);
  const textBase =
    pick(vars, "label-primary", "ink", "text", "neutral-1000", "neutral-900") ??
    (light ? "#1f2937" : "#e5e7eb");
  const layer2 = pick(vars, "bg-layer-2", "bg-200", "neutral-100") ?? (light ? darken(bgBase, 0.04) : lighten(bgBase, 0.06));
  const layer3 = pick(vars, "bg-layer-3", "bg-300", "neutral-200") ?? (light ? darken(bgBase, 0.09) : lighten(bgBase, 0.12));
  const border = pick(vars, "border-l2", "border-l1", "border") ?? (light ? darken(bgBase, 0.12) : lighten(bgBase, 0.18));
  const hover = pick(vars, "interactive-bg-hover-solid", "bg-hover") ?? (light ? darken(bgBase, 0.07) : lighten(bgBase, 0.09));
  const selected = pick(vars, "interactive-bg-active", "bg-active") ?? lighten(accent, light ? 0.72 : 0.1);
  const secondaryText = pick(vars, "label-secondary", "label-tertiary", "muted") ?? (light ? lighten(textBase, 0.35) : darken(textBase, 0.3));
  if (!Object.keys(vars).length) warnings.push(`${label}：未提取到 CSS 变量，已使用默认派生配色`);

  const userBubble = light ? lighten(accent, 0.78) : darken(accent, 0.35);
  return {
    bg: bgBase,
    bgText: textBase,
    sidebarBg: layer2,
    sidebarText: textBase,
    sidebarHover: hover,
    sidebarSelected: selected,
    sidebarHeader: secondaryText,
    editorBg: bgBase,
    tabsBg: layer2,
    tabBg: layer2,
    tabText: secondaryText,
    tabActiveBg: bgBase,
    tabActiveText: textBase,
    aiBg: layer2,
    aiText: textBase,
    aiTabText: secondaryText,
    userBubbleBg: userBubble,
    userBubbleText: isLightColor(userBubble) ? textBase : "#ffffff",
    aiBubbleBg: light ? lighten(bgBase, 0.02) : lighten(bgBase, 0.08),
    aiBubbleText: textBase,
    aiBubbleBorder: border,
    systemBubbleBg: light ? "#fff8e6" : "#3d3420",
    systemBubbleText: light ? "#876800" : "#e8d9a0",
    inputBg: light ? lighten(bgBase, 0.02) : lighten(bgBase, 0.08),
    inputText: textBase,
    inputBorder: border,
    accent,
    accentText: isLightColor(accent) ? darken(accent, 0.75) : "#ffffff",
    border,
    chipBg: light ? lighten(accent, 0.82) : darken(accent, 0.3),
    chipText: light ? darken(accent, 0.35) : lighten(accent, 0.55),
    chipBorder: light ? lighten(accent, 0.45) : accent,
  };
}

// ───────────────────────── 主转换流程 ─────────────────────────

/**
 * 将 GitHub 上的 DSH 皮肤插件仓库转换为 DeepAhead 自定义皮肤。
 * 流程：解析地址 → 读仓库信息 → 列文件树 → 读 skin.json（名称/强调色）
 * → 读 CSS 模块提取配色变量（含暗色作用域）→ 找装饰图 → 组装 SkinDefinition
 */
export async function convertGitHubRepoToSkin(input: string): Promise<ConvertResult> {
  const warnings: string[] = [];
  const ref = parseGitHubUrl(input);

  // 1. 仓库信息（拿默认分支）
  const repoInfo = await fetchJson(`https://api.github.com/repos/${ref.owner}/${ref.repo}`);
  const branch: string = repoInfo.default_branch || "main";

  // 2. 文件树
  const tree = await fetchJson(
    `https://api.github.com/repos/${ref.owner}/${ref.repo}/git/trees/${branch}?recursive=1`
  );
  const paths: string[] = (tree.tree || []).map((n: any) => n.path as string);

  // 3. skin.json（名称 / 强调色 / 描述），跳过 skin.build.json
  const skinJsonPath = paths.find((p) => /(^|\/)skin\.json$/i.test(p));
  let meta: any = {};
  if (skinJsonPath) {
    try {
      meta = JSON.parse(await fetchText(rawUrl(ref, branch, skinJsonPath)));
    } catch {
      warnings.push("skin.json 解析失败，已忽略其中的元信息");
    }
  } else {
    warnings.push("未找到 skin.json，名称与强调色将根据仓库信息派生");
  }

  // 4. CSS 模块：优先与 skin.json 同目录的 src/client/*.module.css，其次任意 .css
  const skinDir = skinJsonPath ? skinJsonPath.replace(/(^|\/)skin\.json$/i, "") : "";
  const cssCandidates = paths.filter((p) => p.endsWith(".css"));
  const cssPath =
    cssCandidates.find((p) => skinDir && p.startsWith(skinDir) && /src\/client\/.*\.module\.css$/i.test(p)) ??
    cssCandidates.find((p) => skinDir && p.startsWith(skinDir)) ??
    cssCandidates.find((p) => /src\/client\/.*\.module\.css$/i.test(p)) ??
    cssCandidates[0];

  let lightVars: VarMap = {};
  let darkVars: VarMap = {};
  if (cssPath) {
    const css = await fetchText(rawUrl(ref, branch, cssPath));
    lightVars = extractVars(css);
    darkVars = extractDarkVars(css);
  } else {
    warnings.push("仓库中未找到 CSS 文件，将完全依赖派生配色");
  }

  // 5. 强调色：skin.json accent > CSS brand 变量 > DeepSeek 蓝
  const accent: string =
    (typeof meta.accent === "string" && meta.accent) ||
    pick(lightVars, "brand-primary", "accent", "primary") ||
    "#4d6bfe";

  // 6. 调色板
  const light = derivePalette(lightVars, accent, warnings, "亮色");
  let dark: SkinPalette | undefined;
  if (Object.keys(darkVars).length > 0) {
    dark = derivePalette(darkVars, accent, warnings, "暗色");
  } else {
    // 未声明暗色作用域时，基于亮色派生一套暗色，保证亮/暗可切换
    dark = derivePalette({}, accent, warnings, "暗色");
    dark = {
      ...dark,
      bg: darken(light.bg, 0.82), bgText: lighten(light.bgText, 0.85),
      sidebarBg: darken(light.sidebarBg, 0.86), sidebarText: lighten(light.sidebarText, 0.7),
      sidebarHover: darken(light.sidebarBg, 0.74), sidebarSelected: darken(accent, 0.45),
      sidebarHeader: darken(light.sidebarText, 0.2),
      editorBg: darken(light.editorBg, 0.82),
      tabsBg: darken(light.tabsBg, 0.86), tabBg: darken(light.tabBg, 0.82),
      tabText: darken(light.sidebarText, 0.2), tabActiveBg: darken(light.tabActiveBg, 0.72), tabActiveText: lighten(light.tabActiveText, 0.85),
      aiBg: darken(light.aiBg, 0.86), aiText: lighten(light.aiText, 0.85), aiTabText: darken(light.sidebarText, 0.2),
      userBubbleBg: darken(accent, 0.4), userBubbleText: "#ffffff",
      aiBubbleBg: darken(light.aiBubbleBg, 0.78), aiBubbleText: lighten(light.aiBubbleText, 0.85), aiBubbleBorder: darken(light.border, 0.5),
      inputBg: darken(light.inputBg, 0.78), inputText: lighten(light.inputText, 0.85), inputBorder: darken(light.border, 0.4),
      border: darken(light.border, 0.55),
      chipBg: darken(accent, 0.45), chipText: lighten(accent, 0.6), chipBorder: darken(accent, 0.15),
    };
    warnings.push("未找到暗色 CSS 变量，暗色变体已基于亮色自动派生");
  }

  // 7. 装饰图：优先 assets/background/ 下图片，其次文件名含 maid/whale/poster 的图片
  const imgRe = /\.(webp|png|jpe?g)$/i;
  const images = paths.filter((p) => imgRe.test(p) && !/^preview\//.test(p) && !/\/preview\//.test(p));
  const mascotPath =
    images.find((p) => /assets\/background\//i.test(p)) ??
    images.find((p) => /(maid|whale|poster|mascot)/i.test(p) && /assets/i.test(p)) ??
    images.find((p) => /assets/i.test(p));
  const mascot = mascotPath ? { light: rawUrl(ref, branch, mascotPath), dark: rawUrl(ref, branch, mascotPath) } : undefined;
  if (!mascotPath) warnings.push("未找到装饰图片，皮肤将不含角色水印");

  const skin: SkinDefinition = {
    id: `custom-${ref.owner}-${ref.repo}`.toLowerCase().replace(/[^a-z0-9-]/g, "-"),
    name: meta.name || meta.nameEn || `${ref.owner}/${ref.repo}`,
    source: `https://github.com/${ref.owner}/${ref.repo}`,
    builtin: false,
    description: meta.tagline || meta.description || repoInfo.description || "从 GitHub 转换的自定义皮肤",
    palettes: { light, dark },
    mascot,
  };
  return { skin, warnings };
}
