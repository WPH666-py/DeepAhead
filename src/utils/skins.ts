/**
 * DeepAhead 界面皮肤系统
 * 覆盖范围：顶部工具栏、文件树、编辑区（标签栏/空状态/容器）、AI 面板（气泡/输入框/按钮）
 * 皮肤通过注入 <style id="DeepAhead-skin-style"> 覆盖全局样式，编辑器 CodeMirror 主题仍由 editorTheme 独立控制
 */

export type SkinVariant = "light" | "dark";

/** 皮肤调色板：作用于 IDE 各区域的槽位色 */
export interface SkinPalette {
  bg: string; // 顶部工具栏背景
  bgText: string; // 顶部工具栏文字
  sidebarBg: string; // 文件树侧栏背景
  sidebarText: string;
  sidebarHover: string;
  sidebarSelected: string;
  sidebarHeader: string; // "资源管理器"标题色
  editorBg: string; // 编辑区容器/空状态背景
  tabsBg: string; // 标签栏背景
  tabBg: string;
  tabText: string;
  tabActiveBg: string;
  tabActiveText: string;
  aiBg: string; // AI 面板背景
  aiText: string;
  aiTabText: string;
  userBubbleBg: string;
  userBubbleText: string;
  aiBubbleBg: string;
  aiBubbleText: string;
  aiBubbleBorder: string;
  systemBubbleBg: string;
  systemBubbleText: string;
  inputBg: string;
  inputText: string;
  inputBorder: string;
  accent: string; // 强调色：发送按钮、激活 tab 上边框等
  accentText: string;
  border: string; // 通用分隔线
  chipBg: string; // 上下文 chip
  chipText: string;
  chipBorder: string;
}

export interface SkinDefinition {
  id: string;
  name: string;
  /** 来源 GitHub 仓库 */
  source?: string;
  /** 内置皮肤不可删除 */
  builtin: boolean;
  description?: string;
  palettes: { light: SkinPalette; dark?: SkinPalette };
  /** 鲸鱼娘装饰图（编辑区右下角直接显示，带投影） */
  mascot?: { light?: string; dark?: string };
}

// ───────────────────────── 内置皮肤（不可删除） ─────────────────────────

// 本地鲸鱼娘素材（打包进应用，无需 VPN 加载）
const imgCloud = new URL("../assets/skins/whale-cloud.webp", import.meta.url).href;
const imgMaid = new URL("../assets/skins/whale-maid.webp", import.meta.url).href;
const imgMaidNight = new URL("../assets/skins/whale-maid-night.webp", import.meta.url).href;
const imgAds = new URL("../assets/skins/whale-ads.webp", import.meta.url).href;
// 本地原神角色皮肤素材（打包进应用，与鲸鱼娘同款内嵌样式）
const imgFurina = new URL("../assets/skins/genshin-furina.png", import.meta.url).href;
const imgCitlali = new URL("../assets/skins/genshin-citlali.png", import.meta.url).href;
const imgKeqing = new URL("../assets/skins/genshin-keqing.png", import.meta.url).href;
const imgKokomi = new URL("../assets/skins/genshin-kokomi.jpg", import.meta.url).href;
const imgAyaka = new URL("../assets/skins/genshin-ayaka.jpg", import.meta.url).href;
const imgYoimiya = new URL("../assets/skins/genshin-yoimiya.png", import.meta.url).href;
const imgShogun = new URL("../assets/skins/genshin-shogun.png", import.meta.url).href;
const imgNahida = new URL("../assets/skins/genshin-nahida.png", import.meta.url).href;
const imgNilou = new URL("../assets/skins/genshin-nilou.png", import.meta.url).href;
const imgCollei = new URL("../assets/skins/genshin-collei.png", import.meta.url).href;
const imgNoelle = new URL("../assets/skins/genshin-noelle.png", import.meta.url).href;
const imgBarbara = new URL("../assets/skins/genshin-barbara.png", import.meta.url).href;
const imgAmbor = new URL("../assets/skins/genshin-ambor.png", import.meta.url).href;
const imgYelan = new URL("../assets/skins/genshin-yelan.png", import.meta.url).href;
const imgZibai = new URL("../assets/skins/genshin-zibai.png", import.meta.url).href;
const imgGanyu = new URL("../assets/skins/genshin-ganyu.png", import.meta.url).href;
const imgColumbina = new URL("../assets/skins/genshin-columbina.png", import.meta.url).href;
const imgLinnea = new URL("../assets/skins/genshin-linnea.png", import.meta.url).href;
const imgEscoffier = new URL("../assets/skins/genshin-escoffier.png", import.meta.url).href;
const imgNavia = new URL("../assets/skins/genshin-navia.png", import.meta.url).href;
const imgMualani = new URL("../assets/skins/genshin-mualani.png", import.meta.url).href;
const imgSandrone = new URL("../assets/skins/genshin-sandrone.png", import.meta.url).href;
const imgClorinde = new URL("../assets/skins/genshin-clorinde.png", import.meta.url).href;
const imgNicole = new URL("../assets/skins/genshin-nicole.png", import.meta.url).href;
const imgSucrose = new URL("../assets/skins/genshin-sucrose.png", import.meta.url).href;
const imgEula = new URL("../assets/skins/genshin-eula.png", import.meta.url).href;
const imgShenhe = new URL("../assets/skins/genshin-shenhe.png", import.meta.url).href;
const imgLumine = new URL("../assets/skins/genshin-lumine.png", import.meta.url).href;

/** 鲸鱼娘·常规风格 —— 源自 dsh-maid-whale-webUI「云鲸纸面」：纸白/云灰/淡天蓝 + 暮蓝暗色 */
const whaleCloud: SkinDefinition = {
  id: "whale-cloud",
  name: "鲸鱼娘·常规",
  source: "https://github.com/yunxiiQwQ/dsh-maid-whale-webUI",
  builtin: true,
  description: "云鲸纸面：纸白云灰淡天蓝，暗色为暮蓝蓝墨",
  palettes: {
    light: {
      bg: "#fffef9", bgText: "#243746",
      sidebarBg: "#e3f1f7", sidebarText: "#315d78", sidebarHover: "#d0e9f2", sidebarSelected: "#bde0ee", sidebarHeader: "#6d8a94",
      editorBg: "#fffef9",
      tabsBg: "#e9f2f5", tabBg: "#e3f1f7", tabText: "#536b6f", tabActiveBg: "#fffef9", tabActiveText: "#243746",
      aiBg: "#eef6f9", aiText: "#243746", aiTabText: "#697f80",
      userBubbleBg: "#cde8f3", userBubbleText: "#243746",
      aiBubbleBg: "#fffef9", aiBubbleText: "#243746", aiBubbleBorder: "#bde0ee",
      systemBubbleBg: "#fff8e6", systemBubbleText: "#876800",
      inputBg: "#fffef9", inputText: "#243746", inputBorder: "#a8d1df",
      accent: "#5aa7d8", accentText: "#fffef9",
      border: "#cde5ef",
      chipBg: "#cde8f3", chipText: "#315d78", chipBorder: "#a8d1df",
    },
    dark: {
      bg: "#1b2a38", bgText: "#deeff4",
      sidebarBg: "#16232e", sidebarText: "#a8d1df", sidebarHover: "#243746", sidebarSelected: "#315d78", sidebarHeader: "#697f80",
      editorBg: "#1b2a38",
      tabsBg: "#16232e", tabBg: "#1b2a38", tabText: "#8d9f9d", tabActiveBg: "#243746", tabActiveText: "#deeff4",
      aiBg: "#16232e", aiText: "#deeff4", aiTabText: "#8d9f9d",
      userBubbleBg: "#315d78", userBubbleText: "#f2f8fa",
      aiBubbleBg: "#243746", aiBubbleText: "#deeff4", aiBubbleBorder: "#315d78",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#243746", inputText: "#deeff4", inputBorder: "#315d78",
      accent: "#69acc8", accentText: "#0f1c26",
      border: "#315d78",
      chipBg: "#315d78", chipText: "#deeff4", chipBorder: "#438db8",
    },
  },
  mascot: {
    light: imgCloud,
    dark: imgCloud,
  },
};

/** 鲸鱼娘·女仆风格 —— 源自 dsh-deep-whale「深海女仆工坊」：深海蓝/陶瓷白/长春花蓝/柔金 */
const whaleMaid: SkinDefinition = {
  id: "whale-maid",
  name: "鲸鱼娘·女仆",
  source: "https://github.com/Small-tailqwq/dsh-deep-whale",
  builtin: true,
  description: "深海女仆工坊：陶瓷白与柔金蕾丝，暗色为深海蓝",
  palettes: {
    light: {
      bg: "#f8f6f0", bgText: "#172347",
      sidebarBg: "#eef1f8", sidebarText: "#243866", sidebarHover: "#e2e8f5", sidebarSelected: "#e9dfc8", sidebarHeader: "#8a94aa",
      editorBg: "#f8f6f0",
      tabsBg: "#e9edf6", tabBg: "#eef1f8", tabText: "#4d5d7f", tabActiveBg: "#f8f6f0", tabActiveText: "#172347",
      aiBg: "#f4f2ec", aiText: "#172347", aiTabText: "#6f7c99",
      userBubbleBg: "#dfe6f6", userBubbleText: "#172347",
      aiBubbleBg: "#fffdf8", aiBubbleText: "#172347", aiBubbleBorder: "#e2cfaa",
      systemBubbleBg: "#f5ecd9", systemBubbleText: "#7a5f2a",
      inputBg: "#fffdf8", inputText: "#172347", inputBorder: "#c5a468",
      accent: "#536eae", accentText: "#f8f6f0",
      border: "#d8dff0",
      chipBg: "#f0e8d5", chipText: "#7a5f2a", chipBorder: "#c5a468",
    },
    dark: {
      bg: "#0d1836", bgText: "#e6e9f5",
      sidebarBg: "#091333", sidebarText: "#8ea5da", sidebarHover: "#10204d", sidebarSelected: "#1c326b", sidebarHeader: "#4d5d7f",
      editorBg: "#0d1836",
      tabsBg: "#091333", tabBg: "#0d1836", tabText: "#6f7c99", tabActiveBg: "#1c326b", tabActiveText: "#e6e9f5",
      aiBg: "#091333", aiText: "#e6e9f5", aiTabText: "#6f7c99",
      userBubbleBg: "#1c326b", userBubbleText: "#e6e9f5",
      aiBubbleBg: "#10204d", aiBubbleText: "#e6e9f5", aiBubbleBorder: "#3d3420",
      systemBubbleBg: "#2a2415", systemBubbleText: "#e2cfaa",
      inputBg: "#10204d", inputText: "#e6e9f5", inputBorder: "#2c447c",
      accent: "#8ea5da", accentText: "#091333",
      border: "#1c326b",
      chipBg: "#1c326b", chipText: "#e2cfaa", chipBorder: "#c5a468",
    },
  },
  mascot: {
    light: imgMaid,
    dark: imgMaidNight,
  },
};

/** 鲸鱼娘·广告风格 —— 源自 dsh-ads 蓝鲸海报：高饱和商业蓝 +  jackpot 金 */
const whaleAds: SkinDefinition = {
  id: "whale-ads",
  name: "鲸鱼娘·广告",
  source: "https://github.com/Nagi-ovo/dsh-ads",
  builtin: true,
  description: "蓝鲸海报风：高饱和商业蓝与亮金点缀",
  palettes: {
    light: {
      bg: "#ffffff", bgText: "#0f1e3d",
      sidebarBg: "#eef3ff", sidebarText: "#1e3a8a", sidebarHover: "#dbe7ff", sidebarSelected: "#c3d7ff", sidebarHeader: "#64748b",
      editorBg: "#ffffff",
      tabsBg: "#e8efff", tabBg: "#eef3ff", tabText: "#475569", tabActiveBg: "#ffffff", tabActiveText: "#0f1e3d",
      aiBg: "#f5f8ff", aiText: "#0f1e3d", aiTabText: "#64748b",
      userBubbleBg: "#2563eb", userBubbleText: "#ffffff",
      aiBubbleBg: "#ffffff", aiBubbleText: "#0f1e3d", aiBubbleBorder: "#c3d7ff",
      systemBubbleBg: "#fef3c7", systemBubbleText: "#92400e",
      inputBg: "#ffffff", inputText: "#0f1e3d", inputBorder: "#93c5fd",
      accent: "#2563eb", accentText: "#ffffff",
      border: "#dbe7ff",
      chipBg: "#fef3c7", chipText: "#92400e", chipBorder: "#f5b301",
    },
  },
  mascot: {
    light: imgAds,
  },
};

/** 芙宁娜·深蓝咏叹风格 —— 源自 Genshen-Furina-Skin：海蓝/陶瓷白，暗色为深海军蓝 */
const furinaSkin: SkinDefinition = {
  id: "genshin-furina",
  name: "芙宁娜 · 深蓝咏叹",
  source: "https://github.com/WPH666-py/Genshen-Furina-Skin",
  builtin: true,
  description: "海蓝咏叹：潮汐瓷白与深蓝底色，暗色为深海军蓝",
  palettes: {
    light: {
      bg: "#f4f9fc", bgText: "#16344a",
      sidebarBg: "#e3f2f9", sidebarText: "#2a6b8f", sidebarHover: "#d3e9f4", sidebarSelected: "#bfe0f0", sidebarHeader: "#6d94a8",
      editorBg: "#f4f9fc",
      tabsBg: "#e9f2f7", tabBg: "#e3f2f9", tabText: "#4a6b80", tabActiveBg: "#f4f9fc", tabActiveText: "#16344a",
      aiBg: "#eef6fa", aiText: "#16344a", aiTabText: "#5c7f95",
      userBubbleBg: "#cce8f5", userBubbleText: "#16344a",
      aiBubbleBg: "#ffffff", aiBubbleText: "#16344a", aiBubbleBorder: "#bfe0f0",
      systemBubbleBg: "#fff7e8", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#16344a", inputBorder: "#9fd2e8",
      accent: "#3d9dc9", accentText: "#ffffff",
      border: "#cfe5f0",
      chipBg: "#d7ecf7", chipText: "#2a6b8f", chipBorder: "#9fd2e8",
    },
    dark: {
      bg: "#10243a", bgText: "#dcecf5",
      sidebarBg: "#0c1c2e", sidebarText: "#7fb4cc", sidebarHover: "#16344f", sidebarSelected: "#235a7c", sidebarHeader: "#5c7f95",
      editorBg: "#10243a",
      tabsBg: "#0c1c2e", tabBg: "#10243a", tabText: "#7f95a0", tabActiveBg: "#16344f", tabActiveText: "#dcecf5",
      aiBg: "#0c1c2e", aiText: "#dcecf5", aiTabText: "#7f95a0",
      userBubbleBg: "#235a7c", userBubbleText: "#eaf5fb",
      aiBubbleBg: "#16344f", aiBubbleText: "#dcecf5", aiBubbleBorder: "#235a7c",
      systemBubbleBg: "#3a3220", systemBubbleText: "#e8d9a0",
      inputBg: "#16344f", inputText: "#dcecf5", inputBorder: "#235a7c",
      accent: "#62b8e8", accentText: "#0c1c2e",
      border: "#235a7c",
      chipBg: "#1c4966", chipText: "#bcdff0", chipBorder: "#3d7fa5",
    },
  },
  mascot: { light: imgFurina, dark: imgFurina },
};

/** 茜特拉莉·紫粉星夜风格 —— 源自 Genshen-Citlali-Skin：紫粉/星夜深紫，暗色为夜紫 */
const citlaliSkin: SkinDefinition = {
  id: "genshin-citlali",
  name: "茜特拉莉 · 紫粉星夜",
  source: "https://github.com/WPH666-py/Genshen-Citlali-Skin",
  builtin: true,
  description: "紫粉星夜：薰衣草淡紫与星辉粉，暗色为夜之紫",
  palettes: {
    light: {
      bg: "#fbf5ff", bgText: "#2a1b4a",
      sidebarBg: "#f1e9fc", sidebarText: "#53387f", sidebarHover: "#e9ddfa", sidebarSelected: "#ddc8f5", sidebarHeader: "#8a7ba6",
      editorBg: "#fbf5ff",
      tabsBg: "#f3ecfe", tabBg: "#f1e9fc", tabText: "#6f5b8f", tabActiveBg: "#fbf5ff", tabActiveText: "#2a1b4a",
      aiBg: "#f6efff", aiText: "#2a1b4a", aiTabText: "#84719f",
      userBubbleBg: "#e4d4fa", userBubbleText: "#2a1b4a",
      aiBubbleBg: "#ffffff", aiBubbleText: "#2a1b4a", aiBubbleBorder: "#ddc8f5",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#2a1b4a", inputBorder: "#c9aef0",
      accent: "#a855f7", accentText: "#ffffff",
      border: "#e6d9f8",
      chipBg: "#f0e4ff", chipText: "#53387f", chipBorder: "#c9aef0",
    },
    dark: {
      bg: "#160b2e", bgText: "#f0dcff",
      sidebarBg: "#100821", sidebarText: "#b58fe0", sidebarHover: "#241245", sidebarSelected: "#3b1d6b", sidebarHeader: "#84719f",
      editorBg: "#160b2e",
      tabsBg: "#100821", tabBg: "#160b2e", tabText: "#9d8ab8", tabActiveBg: "#241245", tabActiveText: "#f0dcff",
      aiBg: "#100821", aiText: "#f0dcff", aiTabText: "#9d8ab8",
      userBubbleBg: "#3b1d6b", userBubbleText: "#f0dcff",
      aiBubbleBg: "#241245", aiBubbleText: "#f0dcff", aiBubbleBorder: "#55319a",
      systemBubbleBg: "#3a3420", systemBubbleText: "#e8d9a0",
      inputBg: "#241245", inputText: "#f0dcff", inputBorder: "#55319a",
      accent: "#f0abfc", accentText: "#160b2e",
      border: "#3b1d6b",
      chipBg: "#2e1657", chipText: "#d8bbf5", chipBorder: "#55319a",
    },
  },
  mascot: { light: imgCitlali, dark: imgCitlali },
};

/** 刻晴·紫电雷鸣风格 —— 源自 Genshen-Keqing-Skin：电紫/云白，暗色为雷夜紫 */
const keqingSkin: SkinDefinition = {
  id: "genshin-keqing",
  name: "刻晴 · 紫电雷鸣",
  source: "https://github.com/WPH666-py/Genshen-Keqing-Skin",
  builtin: true,
  description: "紫电雷鸣：雷光浅紫与云白，暗色为雷夜深紫",
  palettes: {
    light: {
      bg: "#f7f5fc", bgText: "#231a3d",
      sidebarBg: "#eee9f8", sidebarText: "#4f3f7d", sidebarHover: "#e3dbf4", sidebarSelected: "#d3c5ee", sidebarHeader: "#8a83a4",
      editorBg: "#f7f5fc",
      tabsBg: "#f0ecf9", tabBg: "#eee9f8", tabText: "#6b6290", tabActiveBg: "#f7f5fc", tabActiveText: "#231a3d",
      aiBg: "#f4f1fb", aiText: "#231a3d", aiTabText: "#7f76a2",
      userBubbleBg: "#e0d5f5", userBubbleText: "#231a3d",
      aiBubbleBg: "#ffffff", aiBubbleText: "#231a3d", aiBubbleBorder: "#d3c5ee",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#231a3d", inputBorder: "#c1b2e8",
      accent: "#8b5cf6", accentText: "#ffffff",
      border: "#e0d8f2",
      chipBg: "#ece4fb", chipText: "#4f3f7d", chipBorder: "#c1b2e8",
    },
    dark: {
      bg: "#141020", bgText: "#e8e2f8",
      sidebarBg: "#0f0b1c", sidebarText: "#ab9cd8", sidebarHover: "#221a40", sidebarSelected: "#372a63", sidebarHeader: "#7f76a2",
      editorBg: "#141020",
      tabsBg: "#0f0b1c", tabBg: "#141020", tabText: "#8f86b0", tabActiveBg: "#221a40", tabActiveText: "#e8e2f8",
      aiBg: "#0f0b1c", aiText: "#e8e2f8", aiTabText: "#8f86b0",
      userBubbleBg: "#372a63", userBubbleText: "#e8e2f8",
      aiBubbleBg: "#221a40", aiBubbleText: "#e8e2f8", aiBubbleBorder: "#4d3d8a",
      systemBubbleBg: "#3a3420", systemBubbleText: "#e8d9a0",
      inputBg: "#221a40", inputText: "#e8e2f8", inputBorder: "#4d3d8a",
      accent: "#a78bfa", accentText: "#0f0b1c",
      border: "#372a63",
      chipBg: "#2b2050", chipText: "#cdbdf5", chipBorder: "#6152a8",
    },
  },
  mascot: { light: imgKeqing, dark: imgKeqing },
};

/**
 * 心海·海月之誓风格 —— 源自 Genshen-Kokomi-Skin
 * 调色板直接取自壁纸原色：渊海夜蓝（#0a1a33/#071324）为整个编辑器底色，
 * 瓷白（#fefefd）、波光粉金（#f2bca4/#a58947）、海蓝（#265682）点缀
 */
const kokomiSkin: SkinDefinition = {
  id: "genshin-kokomi",
  name: "心海 · 海月之誓",
  source: "https://github.com/WPH666-py/Genshen-Kokomi-Skin",
  builtin: true,
  description: "海月之誓：海蓝瓷白与波光粉紫，暗色为渊海夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#faf7f2", bgText: "#16324d",
      sidebarBg: "#eef3f8", sidebarText: "#2a5a7d", sidebarHover: "#e3ecf4", sidebarSelected: "#f3dcd3", sidebarHeader: "#7a8a99",
      editorBg: "#faf7f2",
      tabsBg: "#eef2f6", tabBg: "#eef3f8", tabText: "#56687a", tabActiveBg: "#faf7f2", tabActiveText: "#16324d",
      aiBg: "#f4f6f9", aiText: "#16324d", aiTabText: "#5f7488",
      userBubbleBg: "#d9e7f5", userBubbleText: "#16324d",
      aiBubbleBg: "#ffffff", aiBubbleText: "#16324d", aiBubbleBorder: "#cfe0ef",
      systemBubbleBg: "#fdf2e3", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#16324d", inputBorder: "#b8cfe2",
      accent: "#3b8fd4", accentText: "#ffffff",
      border: "#dbe6f0",
      chipBg: "#f3e3d8", chipText: "#7a4a2a", chipBorder: "#e8c7ae",
    },
    dark: {
      bg: "#0a1a33", bgText: "#eef0f2",
      sidebarBg: "#071324", sidebarText: "#aeb3bd", sidebarHover: "#162439", sidebarSelected: "#1d3a5a", sidebarHeader: "#556577",
      editorBg: "#0a1a33",
      tabsBg: "#071324", tabBg: "#0a1a33", tabText: "#556577", tabActiveBg: "#162439", tabActiveText: "#fefefd",
      aiBg: "#071324", aiText: "#e8eef5", aiTabText: "#556577",
      userBubbleBg: "#265682", userBubbleText: "#fefefd",
      aiBubbleBg: "#162439", aiBubbleText: "#eef4fa", aiBubbleBorder: "#265682",
      systemBubbleBg: "#3a3018", systemBubbleText: "#e8d9a0",
      inputBg: "#162439", inputText: "#eef4fa", inputBorder: "#364558",
      accent: "#f2bca4", accentText: "#2a1a12",
      border: "#1b2b43",
      chipBg: "#1f3b5e", chipText: "#d9e6f2", chipBorder: "#3b4a64",
    },
  },
  mascot: { light: imgKokomi, dark: imgKokomi },
};

/**
 * 绫华·霜雪冰刃风格 —— 源自 Genshen-Ayaka-Skin
 * 调色板直接取自壁纸原色：冰夜深蓝（#101b31/#0d1527）为整个编辑器底色，
 * 瓷白（#fefefe）、樱粉（#c68792）、冰紫（#535596）点缀
 */
const ayakaSkin: SkinDefinition = {
  id: "genshin-ayaka",
  name: "绫华 · 霜雪冰刃",
  source: "https://github.com/WPH666-py/Genshen-Ayaka-Skin",
  builtin: true,
  description: "霜雪冰刃：冰蓝瓷白与樱粉点缀，暗色为冰夜深蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#fafbfd", bgText: "#1c3355",
      sidebarBg: "#eef2f9", sidebarText: "#39507a", sidebarHover: "#e2e9f4", sidebarSelected: "#fbdfe6", sidebarHeader: "#7a86a2",
      editorBg: "#fafbfd",
      tabsBg: "#edf1f8", tabBg: "#eef2f9", tabText: "#5a6a86", tabActiveBg: "#fafbfd", tabActiveText: "#1c3355",
      aiBg: "#f4f6fb", aiText: "#1c3355", aiTabText: "#5f7090",
      userBubbleBg: "#e0e9fa", userBubbleText: "#1c3355",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1c3355", aiBubbleBorder: "#ccd9f0",
      systemBubbleBg: "#fdf0f3", systemBubbleText: "#8a4a5a",
      inputBg: "#ffffff", inputText: "#1c3355", inputBorder: "#b8c8e8",
      accent: "#5a8fe0", accentText: "#ffffff",
      border: "#dde6f2",
      chipBg: "#fbe3ea", chipText: "#8a4a5a", chipBorder: "#dba8b8",
    },
    dark: {
      bg: "#101b31", bgText: "#f2f4f8",
      sidebarBg: "#0d1527", sidebarText: "#a7abb4", sidebarHover: "#182237", sidebarSelected: "#2c4d7d", sidebarHeader: "#555b6a",
      editorBg: "#101b31",
      tabsBg: "#0d1527", tabBg: "#101b31", tabText: "#555b6a", tabActiveBg: "#182237", tabActiveText: "#fefefe",
      aiBg: "#0d1527", aiText: "#eef2f8", aiTabText: "#555b6a",
      userBubbleBg: "#535596", userBubbleText: "#f2f4f8",
      aiBubbleBg: "#182237", aiBubbleText: "#eef2f8", aiBubbleBorder: "#535596",
      systemBubbleBg: "#3c2430", systemBubbleText: "#eab8c4",
      inputBg: "#182237", inputText: "#eef2f8", inputBorder: "#3a4355",
      accent: "#c68792", accentText: "#2a1014",
      border: "#242c42",
      chipBg: "#22304e", chipText: "#cfd9f0", chipBorder: "#475985",
    },
  },
  mascot: { light: imgAyaka, dark: imgAyaka },
};

/**
 * 宵宫·琉金云间草风格 —— 源自 Genshen-Yoimiya-Skin
 * 调色板直接取自壁纸原色：星火夜蓝（#222a52/#171d3d）为整个编辑器底色，
 * 瓷白（#f8f6f3）、烟火红橙（#d9584a）、金琉光（#e5a677）点缀
 */
const yoimiyaSkin: SkinDefinition = {
  id: "genshin-yoimiya",
  name: "宵宫 · 琉金云间草",
  source: "https://github.com/WPH666-py/Genshen-Yoimiya-Skin",
  builtin: true,
  description: "琉金云间草：烟火红橙与瓷白，暗色为星火夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f8f6f3", bgText: "#3c2a30",
      sidebarBg: "#f3ecea", sidebarText: "#75505a", sidebarHover: "#ebe0dd", sidebarSelected: "#f3d0c4", sidebarHeader: "#8a6a70",
      editorBg: "#f8f6f3",
      tabsBg: "#f0e8e6", tabBg: "#f3ecea", tabText: "#8a6a70", tabActiveBg: "#f8f6f3", tabActiveText: "#3c2a30",
      aiBg: "#f5efed", aiText: "#3c2a30", aiTabText: "#8b7078",
      userBubbleBg: "#f7dfd3", userBubbleText: "#3c2a30",
      aiBubbleBg: "#ffffff", aiBubbleText: "#3c2a30", aiBubbleBorder: "#e5cdc2",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#3c2a30", inputBorder: "#d9b8ae",
      accent: "#d9584a", accentText: "#ffffff",
      border: "#e8d8d2",
      chipBg: "#f9e2d4", chipText: "#8a4a3a", chipBorder: "#e0b49a",
    },
    dark: {
      bg: "#222a52", bgText: "#f4e6d0",
      sidebarBg: "#171d3d", sidebarText: "#a9a0c2", sidebarHover: "#2c3563", sidebarSelected: "#584b74", sidebarHeader: "#8a81a8",
      editorBg: "#222a52",
      tabsBg: "#171d3d", tabBg: "#222a52", tabText: "#8a81a8", tabActiveBg: "#2c3563", tabActiveText: "#f4e6d0",
      aiBg: "#171d3d", aiText: "#f0e8dc", aiTabText: "#8a81a8",
      userBubbleBg: "#584b74", userBubbleText: "#f7f2e9",
      aiBubbleBg: "#2c3563", aiBubbleText: "#f0e8dc", aiBubbleBorder: "#584b74",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#2c3563", inputText: "#f0e8dc", inputBorder: "#494374",
      accent: "#e5a677", accentText: "#2a1a10",
      border: "#373965",
      chipBg: "#494374", chipText: "#f2ddc8", chipBorder: "#584b74",
    },
  },
  mascot: { light: imgYoimiya, dark: imgYoimiya },
};

/**
 * 雷电将军·奥义梦想真说风格 —— 源自 Genshen-Shogun-Skin
 * 调色板直接取自壁纸原色：求道夜紫（#1e1a3e/#151130）为整个编辑器底色，
 * 瓷白（#f6f2f7）、电紫（#7a5ad8）、樱粉雷光（#b48cf0）点缀
 */
const shogunSkin: SkinDefinition = {
  id: "genshin-shogun",
  name: "雷电将军 · 奥义梦想真说",
  source: "https://github.com/WPH666-py/Genshen-Shogun-Skin",
  builtin: true,
  description: "奥义梦想真说：电紫瓷白与樱粉雷光，暗色为求道夜紫（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f6f2f7", bgText: "#2c2050",
      sidebarBg: "#edeaf5", sidebarText: "#5a4a86", sidebarHover: "#e2ddf0", sidebarSelected: "#f3d7e0", sidebarHeader: "#8a82a8",
      editorBg: "#f6f2f7",
      tabsBg: "#eae6f3", tabBg: "#edeaf5", tabText: "#7a6f9a", tabActiveBg: "#f6f2f7", tabActiveText: "#2c2050",
      aiBg: "#f1edf7", aiText: "#2c2050", aiTabText: "#7a6f9a",
      userBubbleBg: "#d9dcf5", userBubbleText: "#2c2050",
      aiBubbleBg: "#ffffff", aiBubbleText: "#2c2050", aiBubbleBorder: "#ccd3ee",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#2c2050", inputBorder: "#b9c0e8",
      accent: "#7a5ad8", accentText: "#ffffff",
      border: "#e2def0",
      chipBg: "#f3dbe6", chipText: "#8a4a6a", chipBorder: "#dfb8c8",
    },
    dark: {
      bg: "#1e1a3e", bgText: "#efe9ff",
      sidebarBg: "#151130", sidebarText: "#a89ad8", sidebarHover: "#2b2452", sidebarSelected: "#4a3d8e", sidebarHeader: "#6f68a0",
      editorBg: "#1e1a3e",
      tabsBg: "#151130", tabBg: "#1e1a3e", tabText: "#6f68a0", tabActiveBg: "#2b2452", tabActiveText: "#efe9ff",
      aiBg: "#151130", aiText: "#eae4ff", aiTabText: "#6f68a0",
      userBubbleBg: "#4a3d8e", userBubbleText: "#f4f0ff",
      aiBubbleBg: "#2b2452", aiBubbleText: "#eae4ff", aiBubbleBorder: "#4a3d8e",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#2b2452", inputText: "#eae4ff", inputBorder: "#5a4d9e",
      accent: "#b48cf0", accentText: "#1a1030",
      border: "#322a5e",
      chipBg: "#352a66", chipText: "#d8c8ff", chipBorder: "#5a4d9e",
    },
  },
  mascot: { light: imgShogun, dark: imgShogun },
};

/**
 * 纳西妲·心景幻成风格 —— 源自 Genshen-Nahida-Skin
 * 调色板直接取自壁纸原色：净善夜青（#12271d/#0c1c14）为整个编辑器底色，
 * 瓷白（#f5f8ec）、森之青绿（#4c9e48）、金光（#e8c860）点缀
 */
const nahidaSkin: SkinDefinition = {
  id: "genshin-nahida",
  name: "纳西妲 · 心景幻成",
  source: "https://github.com/WPH666-py/Genshen-Nahida-Skin",
  builtin: true,
  description: "心景幻成：森青瓷白与金光，暗色为净善夜青（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f5f8ec", bgText: "#1f3a24",
      sidebarBg: "#e9f0dd", sidebarText: "#4d7048", sidebarHover: "#dcebc9", sidebarSelected: "#d9ecb5", sidebarHeader: "#7d9a68",
      editorBg: "#f5f8ec",
      tabsBg: "#e9f0de", tabBg: "#e9f0dd", tabText: "#7d9a68", tabActiveBg: "#f5f8ec", tabActiveText: "#1f3a24",
      aiBg: "#f0f5e8", aiText: "#1f3a24", aiTabText: "#7d9a68",
      userBubbleBg: "#d9ecc0", userBubbleText: "#1f3a24",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1f3a24", aiBubbleBorder: "#c9e0ad",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1f3a24", inputBorder: "#b4d398",
      accent: "#4a9e46", accentText: "#ffffff",
      border: "#dcebc9",
      chipBg: "#e3f3c8", chipText: "#4a6a3a", chipBorder: "#b9d69a",
    },
    dark: {
      bg: "#12271d", bgText: "#eefad8",
      sidebarBg: "#0c1c14", sidebarText: "#9ec79a", sidebarHover: "#1b3a28", sidebarSelected: "#2d5c3a", sidebarHeader: "#6d8f6a",
      editorBg: "#12271d",
      tabsBg: "#0c1c14", tabBg: "#12271d", tabText: "#6d8f6a", tabActiveBg: "#1b3a28", tabActiveText: "#eefad8",
      aiBg: "#0c1c14", aiText: "#e6f4d8", aiTabText: "#6d8f6a",
      userBubbleBg: "#2d5c3a", userBubbleText: "#f2fbe4",
      aiBubbleBg: "#1b3a28", aiBubbleText: "#e6f4d8", aiBubbleBorder: "#2d5c3a",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1b3a28", inputText: "#e6f4d8", inputBorder: "#3a6b48",
      accent: "#e8c860", accentText: "#2a1f10",
      border: "#24402e",
      chipBg: "#224532", chipText: "#d8f0b8", chipBorder: "#3a6b48",
    },
  },
  mascot: { light: imgNahida, dark: imgNahida },
};

/**
 * 妮露·浮莲舞步远梦聆泉风格 —— 源自 Genshen-Nilou-Skin
 * 调色板直接取自壁纸原色：澄水夜蓝（#0e2a3e/#0a1f30）为整个编辑器底色，
 * 瓷白（#f6faf9）、水之湛蓝（#3f9fc9）、莲紫（#8e7bd8）点缀
 */
const nilouSkin: SkinDefinition = {
  id: "genshin-nilou",
  name: "妮露 · 浮莲舞步远梦聆泉",
  source: "https://github.com/WPH666-py/Genshen-Nilou-Skin",
  builtin: true,
  description: "浮莲舞步远梦聆泉：水蓝瓷白与莲紫，暗色为澄水夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f6faf9", bgText: "#14495e",
      sidebarBg: "#e6f0f2", sidebarText: "#2c6a8a", sidebarHover: "#d3e8ec", sidebarSelected: "#bfe2ea", sidebarHeader: "#7d9aa8",
      editorBg: "#f6faf9",
      tabsBg: "#e6f0f2", tabBg: "#e6f0f2", tabText: "#6d93a8", tabActiveBg: "#f6faf9", tabActiveText: "#14495e",
      aiBg: "#f0f7f9", aiText: "#14495e", aiTabText: "#6d93a8",
      userBubbleBg: "#cfe8f2", userBubbleText: "#14495e",
      aiBubbleBg: "#ffffff", aiBubbleText: "#14495e", aiBubbleBorder: "#a5ccdb",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#14495e", inputBorder: "#a5ccdb",
      accent: "#3f9fc9", accentText: "#ffffff",
      border: "#d3e8ec",
      chipBg: "#dbeaf2", chipText: "#2c6a8a", chipBorder: "#a5ccdb",
    },
    dark: {
      bg: "#0e2a3e", bgText: "#e6f6fb",
      sidebarBg: "#0a1f30", sidebarText: "#9cc4d8", sidebarHover: "#16384f", sidebarSelected: "#1f5678", sidebarHeader: "#6d93a8",
      editorBg: "#0e2a3e",
      tabsBg: "#0a1f30", tabBg: "#0e2a3e", tabText: "#6d93a8", tabActiveBg: "#16384f", tabActiveText: "#e6f6fb",
      aiBg: "#0a1f30", aiText: "#dff3f8", aiTabText: "#6d93a8",
      userBubbleBg: "#1f5678", userBubbleText: "#f2fbfd",
      aiBubbleBg: "#16384f", aiBubbleText: "#dff3f8", aiBubbleBorder: "#1f5678",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#16384f", inputText: "#dff3f8", inputBorder: "#2c6a8a",
      accent: "#5fc6e8", accentText: "#0a2434",
      border: "#16384f",
      chipBg: "#16384f", chipText: "#bfe6f2", chipBorder: "#2c6a8a",
    },
  },
  mascot: { light: imgNilou, dark: imgNilou },
};

/**
 * 柯莱·猫猫秘宝风格 —— 源自 Genshen-Collei-Skin
 * 调色板直接取自壁纸原色：林间夜青（#122418/#0c1a12）为整个编辑器底色，
 * 瓷白（#f5f7ee）、森之新绿（#62a83e）、金饰（#d8b360）点缀
 */
const colleiSkin: SkinDefinition = {
  id: "genshin-collei",
  name: "柯莱 · 猫猫秘宝",
  source: "https://github.com/WPH666-py/Genshen-Collei-Skin",
  builtin: true,
  description: "猫猫秘宝：森青瓷白与金饰，暗色为林间夜青（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f5f7ee", bgText: "#24331f",
      sidebarBg: "#e6eed8", sidebarText: "#4f6b3f", sidebarHover: "#d8e8c2", sidebarSelected: "#cde2b4", sidebarHeader: "#7f9a68",
      editorBg: "#f5f7ee",
      tabsBg: "#e6eed8", tabBg: "#e6eed8", tabText: "#6f8f68", tabActiveBg: "#f5f7ee", tabActiveText: "#24331f",
      aiBg: "#f1f5e8", aiText: "#24331f", aiTabText: "#6f8f68",
      userBubbleBg: "#d5e8bd", userBubbleText: "#24331f",
      aiBubbleBg: "#ffffff", aiBubbleText: "#24331f", aiBubbleBorder: "#c2dba6",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#24331f", inputBorder: "#a9cc8e",
      accent: "#62a83e", accentText: "#ffffff",
      border: "#d8e8c2",
      chipBg: "#e2f0cf", chipText: "#55703f", chipBorder: "#a9cc8e",
    },
    dark: {
      bg: "#122418", bgText: "#e8f4d8",
      sidebarBg: "#0c1a12", sidebarText: "#9cc08a", sidebarHover: "#1a3424", sidebarSelected: "#2a4f38", sidebarHeader: "#6f8f68",
      editorBg: "#122418",
      tabsBg: "#0c1a12", tabBg: "#122418", tabText: "#6f8f68", tabActiveBg: "#1a3424", tabActiveText: "#e8f4d8",
      aiBg: "#0c1a12", aiText: "#e2f0ce", aiTabText: "#6f8f68",
      userBubbleBg: "#2a4f38", userBubbleText: "#f0fae2",
      aiBubbleBg: "#1a3424", aiBubbleText: "#e2f0ce", aiBubbleBorder: "#2a4f38",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1a3424", inputText: "#e2f0ce", inputBorder: "#3a6b48",
      accent: "#d8b360", accentText: "#241a08",
      border: "#2a4f38",
      chipBg: "#1d3a28", chipText: "#cfe4b8", chipBorder: "#3a6b48",
    },
  },
  mascot: { light: imgCollei, dark: imgCollei },
};

/**
 * 诺艾尔·大扫除风格 —— 源自 Genshen-Noelle-Skin
 * 调色板直接取自壁纸原色：玫瑰夜红（#2a1220/#1e0c17）为整个编辑器底色，
 * 蔷薇粉白（#faf4f6）、骑士银（#9aa4b8）、金饰（#e8c052）点缀
 */
const noelleSkin: SkinDefinition = {
  id: "genshin-noelle",
  name: "诺艾尔 · 大扫除",
  source: "https://github.com/WPH666-py/Genshen-Noelle-Skin",
  builtin: true,
  description: "大扫除：蔷薇粉白与骑士银，暗色为玫瑰夜红（取壁纸原色）",
  palettes: {
    light: {
      bg: "#faf4f6", bgText: "#4a2230",
      sidebarBg: "#f2e8ec", sidebarText: "#8a5a68", sidebarHover: "#e8dae0", sidebarSelected: "#f2cdd8", sidebarHeader: "#98767f",
      editorBg: "#faf4f6",
      tabsBg: "#f2e8ec", tabBg: "#f2e8ec", tabText: "#98767f", tabActiveBg: "#faf4f6", tabActiveText: "#4a2230",
      aiBg: "#f7eff2", aiText: "#4a2230", aiTabText: "#98767f",
      userBubbleBg: "#f6dae3", userBubbleText: "#4a2230",
      aiBubbleBg: "#ffffff", aiBubbleText: "#4a2230", aiBubbleBorder: "#e5c2cd",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#4a2230", inputBorder: "#d0a8b8",
      accent: "#c94060", accentText: "#ffffff",
      border: "#e8dae0",
      chipBg: "#f8e0e8", chipText: "#8a4054", chipBorder: "#d0a8b8",
    },
    dark: {
      bg: "#2a1220", bgText: "#fae8f0",
      sidebarBg: "#1e0c17", sidebarText: "#c0aeb8", sidebarHover: "#3a1a2c", sidebarSelected: "#4a2440", sidebarHeader: "#8f7a88",
      editorBg: "#2a1220",
      tabsBg: "#1e0c17", tabBg: "#2a1220", tabText: "#8f7a88", tabActiveBg: "#3a1a2c", tabActiveText: "#fae8f0",
      aiBg: "#1e0c17", aiText: "#f5e4ec", aiTabText: "#8f7a88",
      userBubbleBg: "#4a2440", userBubbleText: "#faf0f5",
      aiBubbleBg: "#3a1a2c", aiBubbleText: "#f5e4ec", aiBubbleBorder: "#4a2440",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#3a1a2c", inputText: "#f5e4ec", inputBorder: "#5a3a4c",
      accent: "#e8c052", accentText: "#2a1018",
      border: "#3a1a2c",
      chipBg: "#33202c", chipText: "#ffd6e4", chipBorder: "#5a3a4c",
    },
  },
  mascot: { light: imgNoelle, dark: imgNoelle },
};

/**
 * 芭芭拉·闪耀奇迹风格 —— 源自 Genshen-Barbara-Skin
 * 调色板直接取自壁纸原色：澄水夜蓝（#12283c/#0c1c2c）为整个编辑器底色，
 * 瓷白（#f6fafb）、水之湛蓝（#4a90d8）、浅金（#e0b368）点缀
 */
const barbaraSkin: SkinDefinition = {
  id: "genshin-barbara",
  name: "芭芭拉 · 闪耀奇迹",
  source: "https://github.com/WPH666-py/Genshen-Barbara-Skin",
  builtin: true,
  description: "闪耀奇迹：水蓝瓷白与浅金，暗色为澄水夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f6fafb", bgText: "#1d4a68",
      sidebarBg: "#e3f0f5", sidebarText: "#2d6a8a", sidebarHover: "#d0e8f0", sidebarSelected: "#bfe0ee", sidebarHeader: "#6fa0b8",
      editorBg: "#f6fafb",
      tabsBg: "#e3f0f5", tabBg: "#e3f0f5", tabText: "#6d90a8", tabActiveBg: "#f6fafb", tabActiveText: "#1d4a68",
      aiBg: "#eef5f9", aiText: "#1d4a68", aiTabText: "#6d90a8",
      userBubbleBg: "#cfe6f2", userBubbleText: "#1d4a68",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1d4a68", aiBubbleBorder: "#b8d8e8",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1d4a68", inputBorder: "#a5cce0",
      accent: "#4a90d8", accentText: "#ffffff",
      border: "#d0e8f0",
      chipBg: "#dcecf5", chipText: "#2d6a8a", chipBorder: "#a5cce0",
    },
    dark: {
      bg: "#12283c", bgText: "#e4f4fb",
      sidebarBg: "#0c1c2c", sidebarText: "#9cc4d8", sidebarHover: "#1c3a52", sidebarSelected: "#2a5a7c", sidebarHeader: "#6d90a8",
      editorBg: "#12283c",
      tabsBg: "#0c1c2c", tabBg: "#12283c", tabText: "#6d90a8", tabActiveBg: "#1c3a52", tabActiveText: "#e4f4fb",
      aiBg: "#0c1c2c", aiText: "#e0f0f8", aiTabText: "#6d90a8",
      userBubbleBg: "#2a5a7c", userBubbleText: "#f0f8fc",
      aiBubbleBg: "#1c3a52", aiBubbleText: "#e0f0f8", aiBubbleBorder: "#2a5a7c",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1c3a52", inputText: "#e0f0f8", inputBorder: "#2a5a7c",
      accent: "#5fc6e8", accentText: "#0a2434",
      border: "#1c3a52",
      chipBg: "#1c3a52", chipText: "#c4e6f2", chipBorder: "#2a5a7c",
    },
  },
  mascot: { light: imgBarbara, dark: imgBarbara },
};

/**
 * 安柏·箭雨风格 —— 源自 Genshen-Ambor-Skin
 * 调色板直接取自壁纸原色：暗火夜红（#26090c/#1c0608）为整个编辑器底色，
 * 瓷白（#faf3f0）、兔兔红（#d93a2e）、琥珀金（#d8a050）点缀
 */
const amborSkin: SkinDefinition = {
  id: "genshin-ambor",
  name: "安柏 · 箭雨",
  source: "https://github.com/WPH666-py/Genshen-Ambor-Skin",
  builtin: true,
  description: "箭雨：兔兔红白与琥珀金，暗色为暗火夜红（取壁纸原色）",
  palettes: {
    light: {
      bg: "#faf3f0", bgText: "#5a2020",
      sidebarBg: "#f3e4de", sidebarText: "#8a4030", sidebarHover: "#e8d4cc", sidebarSelected: "#f0c0b8", sidebarHeader: "#a08078",
      editorBg: "#faf3f0",
      tabsBg: "#f3e4de", tabBg: "#f3e4de", tabText: "#8a5a50", tabActiveBg: "#faf3f0", tabActiveText: "#5a2020",
      aiBg: "#f8ebe4", aiText: "#5a2020", aiTabText: "#8a5a50",
      userBubbleBg: "#f6d0c4", userBubbleText: "#5a2020",
      aiBubbleBg: "#ffffff", aiBubbleText: "#5a2020", aiBubbleBorder: "#dba8a0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#5a2020", inputBorder: "#dba8a0",
      accent: "#d93a2e", accentText: "#ffffff",
      border: "#e8d4cc",
      chipBg: "#f8ddd4", chipText: "#8a4030", chipBorder: "#dba8a0",
    },
    dark: {
      bg: "#26090c", bgText: "#fae4dc",
      sidebarBg: "#1c0608", sidebarText: "#b08a80", sidebarHover: "#380e12", sidebarSelected: "#5a1a1e", sidebarHeader: "#8a5a50",
      editorBg: "#26090c",
      tabsBg: "#1c0608", tabBg: "#26090c", tabText: "#8a5a50", tabActiveBg: "#380e12", tabActiveText: "#fae4dc",
      aiBg: "#1c0608", aiText: "#f5e0d8", aiTabText: "#8a5a50",
      userBubbleBg: "#5a1a1e", userBubbleText: "#faf0e8",
      aiBubbleBg: "#380e12", aiBubbleText: "#f5e0d8", aiBubbleBorder: "#5a1a1e",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#380e12", inputText: "#f5e0d8", inputBorder: "#5a2a20",
      accent: "#e8763e", accentText: "#2a1008",
      border: "#380e12",
      chipBg: "#2e0d10", chipText: "#f0c0a8", chipBorder: "#5a2a20",
    },
  },
  mascot: { light: imgAmbor, dark: imgAmbor },
};

/**
 * 夜兰·玄掷玲珑风格 —— 源自 Genshen-Yelan-Skin
 * 调色板直接取自壁纸原色：深海夜蓝（#0a1a30/#071220）为整个编辑器底色，
 * 玄青蓝白（#eaf4f8）、碧水光（#3fd4f0）、金（#c8a052）点缀
 */
const yelanSkin: SkinDefinition = {
  id: "genshin-yelan",
  name: "夜兰 · 玄掷玲珑",
  source: "https://github.com/WPH666-py/Genshen-Yelan-Skin",
  builtin: true,
  description: "玄掷玲珑：玄青蓝白与碧水光，暗色为深海夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#eaf4f8", bgText: "#16324a",
      sidebarBg: "#d9e9f0", sidebarText: "#2c6a88", sidebarHover: "#c9e0ea", sidebarSelected: "#aed8e8", sidebarHeader: "#5f8ca0",
      editorBg: "#eaf4f8",
      tabsBg: "#d9e9f0", tabBg: "#d9e9f0", tabText: "#5f8096", tabActiveBg: "#eaf4f8", tabActiveText: "#16324a",
      aiBg: "#e2f0f5", aiText: "#16324a", aiTabText: "#5f8096",
      userBubbleBg: "#bfe4f0", userBubbleText: "#16324a",
      aiBubbleBg: "#ffffff", aiBubbleText: "#16324a", aiBubbleBorder: "#93c4d8",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#16324a", inputBorder: "#87b8cc",
      accent: "#1f7fd4", accentText: "#ffffff",
      border: "#c9e0ea",
      chipBg: "#c4e6f2", chipText: "#2c6a88", chipBorder: "#87b8cc",
    },
    dark: {
      bg: "#0a1a30", bgText: "#d8f2fa",
      sidebarBg: "#071220", sidebarText: "#8ab0c4", sidebarHover: "#13283e", sidebarSelected: "#1f4460", sidebarHeader: "#5f7a90",
      editorBg: "#0a1a30",
      tabsBg: "#071220", tabBg: "#0a1a30", tabText: "#5f7a90", tabActiveBg: "#13283e", tabActiveText: "#d8f2fa",
      aiBg: "#071220", aiText: "#d2ecf5", aiTabText: "#5f7a90",
      userBubbleBg: "#1f4460", userBubbleText: "#e8f7fc",
      aiBubbleBg: "#13283e", aiBubbleText: "#d2ecf5", aiBubbleBorder: "#1f4460",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#13283e", inputText: "#d2ecf5", inputBorder: "#2c5a78",
      accent: "#3fd4f0", accentText: "#06121e",
      border: "#13283e",
      chipBg: "#13283e", chipText: "#a8e8f5", chipBorder: "#2c5a78",
    },
  },
  mascot: { light: imgYelan, dark: imgYelan },
};

/**
 * 兹白·三垣威仪法风格 —— 源自 Genshen-Zibai-Skin
 * 调色板直接取自壁纸原色：太阴夜青（#12241e/#0c1a15）为整个编辑器底色，
 * 青玄瓷白（#f2f7f2）、仙草青绿（#2f9e78）、月金（#d8b060）点缀
 */
const zibaiSkin: SkinDefinition = {
  id: "genshin-zibai",
  name: "兹白 · 三垣威仪法",
  source: "https://github.com/WPH666-py/Genshen-Zibai-Skin",
  builtin: true,
  description: "三垣威仪法：青玄瓷白与月金，暗色为太阴夜青（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f2f7f2", bgText: "#1f4038",
      sidebarBg: "#e2eee6", sidebarText: "#3f7a62", sidebarHover: "#d0e4d8", sidebarSelected: "#bcd8c4", sidebarHeader: "#6f9a82",
      editorBg: "#f2f7f2",
      tabsBg: "#e2eee6", tabBg: "#e2eee6", tabText: "#6f907e", tabActiveBg: "#f2f7f2", tabActiveText: "#1f4038",
      aiBg: "#eaf4ee", aiText: "#1f4038", aiTabText: "#6f907e",
      userBubbleBg: "#c8e8d2", userBubbleText: "#1f4038",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1f4038", aiBubbleBorder: "#9ecbb0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1f4038", inputBorder: "#94c4a8",
      accent: "#2f9e78", accentText: "#ffffff",
      border: "#d0e4d8",
      chipBg: "#d2ecdc", chipText: "#3f7a62", chipBorder: "#94c4a8",
    },
    dark: {
      bg: "#12241e", bgText: "#dff4ea",
      sidebarBg: "#0c1a15", sidebarText: "#94bca8", sidebarHover: "#1c362c", sidebarSelected: "#2a4f40", sidebarHeader: "#6f907e",
      editorBg: "#12241e",
      tabsBg: "#0c1a15", tabBg: "#12241e", tabText: "#6f907e", tabActiveBg: "#1c362c", tabActiveText: "#dff4ea",
      aiBg: "#0c1a15", aiText: "#d8f0e2", aiTabText: "#6f907e",
      userBubbleBg: "#2a4f40", userBubbleText: "#eefaf4",
      aiBubbleBg: "#1c362c", aiBubbleText: "#d8f0e2", aiBubbleBorder: "#2a4f40",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1c362c", inputText: "#d8f0e2", inputBorder: "#2f5a44",
      accent: "#d8b060", accentText: "#2a1f08",
      border: "#1c362c",
      chipBg: "#1c362c", chipText: "#c4e8d4", chipBorder: "#2f5a44",
    },
  },
  mascot: { light: imgZibai, dark: imgZibai },
};

/**
 * 甘雨·降众天华风格 —— 源自 Genshen-Ganyu-Skin
 * 调色板直接取自壁纸原色：璃月夜蓝（#101a36/#0a1226）为整个编辑器底色，
 * 冰蓝瓷白（#eef4fb）、冰之蓝（#5a8fe0）、金光（#d8a850）点缀
 */
const ganyuSkin: SkinDefinition = {
  id: "genshin-ganyu",
  name: "甘雨 · 降众天华",
  source: "https://github.com/WPH666-py/Genshen-Ganyu-Skin",
  builtin: true,
  description: "降众天华：冰蓝瓷白与金光，暗色为璃月夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#eef4fb", bgText: "#1d3a5e",
      sidebarBg: "#dfeaf5", sidebarText: "#3d6a8a", sidebarHover: "#cfe0f0", sidebarSelected: "#b8d4ec", sidebarHeader: "#6f93ae",
      editorBg: "#eef4fb",
      tabsBg: "#dfeaf5", tabBg: "#dfeaf5", tabText: "#6f86a8", tabActiveBg: "#eef4fb", tabActiveText: "#1d3a5e",
      aiBg: "#e8f1fa", aiText: "#1d3a5e", aiTabText: "#6f86a8",
      userBubbleBg: "#cfe6f8", userBubbleText: "#1d3a5e",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1d3a5e", aiBubbleBorder: "#a5c8e0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1d3a5e", inputBorder: "#a5c4e0",
      accent: "#5a8fe0", accentText: "#ffffff",
      border: "#cfe0f0",
      chipBg: "#d9e8f5", chipText: "#3d6a8a", chipBorder: "#a5c4e0",
    },
    dark: {
      bg: "#101a36", bgText: "#e0ecfa",
      sidebarBg: "#0a1226", sidebarText: "#9cb8d0", sidebarHover: "#1a2a4c", sidebarSelected: "#2a4070", sidebarHeader: "#6f86a8",
      editorBg: "#101a36",
      tabsBg: "#0a1226", tabBg: "#101a36", tabText: "#6f86a8", tabActiveBg: "#1a2a4c", tabActiveText: "#e0ecfa",
      aiBg: "#0a1226", aiText: "#dceefa", aiTabText: "#6f86a8",
      userBubbleBg: "#2a4070", userBubbleText: "#eef8ff",
      aiBubbleBg: "#1a2a4c", aiBubbleText: "#dceefa", aiBubbleBorder: "#2a4070",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1a2a4c", inputText: "#dceefa", inputBorder: "#3d5a88",
      accent: "#6fc0f0", accentText: "#0a1a30",
      border: "#1a2a4c",
      chipBg: "#1a2a4c", chipText: "#bcd8f0", chipBorder: "#3d5a88",
    },
  },
  mascot: { light: imgGanyu, dark: imgGanyu },
};

/**
 * 哥伦比娅·她的乡愁风格 —— 源自 Genshen-Columbina-Skin
 * 调色板直接取自壁纸原色：月蚀夜紫（#1e1638/#150f28）为整个编辑器底色，
 * 月白鸢尾（#f4f2fa）、靛紫（#7a5fd8）、月光蓝（#8fa8e8）点缀
 */
const columbinaSkin: SkinDefinition = {
  id: "genshin-columbina",
  name: "哥伦比娅 · 她的乡愁",
  source: "https://github.com/WPH666-py/Genshen-Columbina-Skin",
  builtin: true,
  description: "她的乡愁：月白鸢尾与靛紫，暗色为月蚀夜紫（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f4f2fa", bgText: "#3a2a52",
      sidebarBg: "#e9e6f5", sidebarText: "#5a4a8a", sidebarHover: "#dbd6ee", sidebarSelected: "#c8bfe8", sidebarHeader: "#8a80b0",
      editorBg: "#f4f2fa",
      tabsBg: "#e9e6f5", tabBg: "#e9e6f5", tabText: "#8a80b0", tabActiveBg: "#f4f2fa", tabActiveText: "#3a2a52",
      aiBg: "#efeaf9", aiText: "#3a2a52", aiTabText: "#8a80b0",
      userBubbleBg: "#e0d8f5", userBubbleText: "#3a2a52",
      aiBubbleBg: "#ffffff", aiBubbleText: "#3a2a52", aiBubbleBorder: "#b8b0d8",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#3a2a52", inputBorder: "#b8b0d8",
      accent: "#7a5fd8", accentText: "#ffffff",
      border: "#dbd6ee",
      chipBg: "#e8e2f8", chipText: "#5a4a8a", chipBorder: "#b8b0d8",
    },
    dark: {
      bg: "#1e1638", bgText: "#eef2fa",
      sidebarBg: "#150f28", sidebarText: "#b2a8d0", sidebarHover: "#2a2050", sidebarSelected: "#3a2e6e", sidebarHeader: "#8a80b0",
      editorBg: "#1e1638",
      tabsBg: "#150f28", tabBg: "#1e1638", tabText: "#8a80b0", tabActiveBg: "#2a2050", tabActiveText: "#eef2fa",
      aiBg: "#150f28", aiText: "#e4e2f6", aiTabText: "#8a80b0",
      userBubbleBg: "#3a2e6e", userBubbleText: "#f4f2fd",
      aiBubbleBg: "#2a2050", aiBubbleText: "#e4e2f6", aiBubbleBorder: "#3a2e6e",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#2a2050", inputText: "#e4e2f6", inputBorder: "#5a4a9e",
      accent: "#b48cf0", accentText: "#1a1030",
      border: "#2a2050",
      chipBg: "#2a2050", chipText: "#d8ccf0", chipBorder: "#5a4a9e",
    },
  },
  mascot: { light: imgColumbina, dark: imgColumbina },
};

/**
 * 莉奈娅·备忘绝境生存指南风格 —— 源自 Genshen-Linnea-Skin
 * 调色板直接取自壁纸原色：玫夜深红（#2a0e16/#1e080e）为整个编辑器底色，
 * 暮粉瓷白（#fdf4f4）、蔷薇红（#d94a68）、樱粉（#f5b8c8）点缀
 */
const linneaSkin: SkinDefinition = {
  id: "genshin-linnea",
  name: "莉奈娅 · 备忘绝境生存指南",
  source: "https://github.com/WPH666-py/Genshen-Linnea-Skin",
  builtin: true,
  description: "备忘绝境生存指南：暮粉瓷白与蔷薇红，暗色为玫夜深红（取壁纸原色）",
  palettes: {
    light: {
      bg: "#fdf4f4", bgText: "#6a2030",
      sidebarBg: "#f7e6e8", sidebarText: "#8a4050", sidebarHover: "#f0d5da", sidebarSelected: "#f0b8c4", sidebarHeader: "#a87888",
      editorBg: "#fdf4f4",
      tabsBg: "#f7e6e8", tabBg: "#f7e6e8", tabText: "#a87888", tabActiveBg: "#fdf4f4", tabActiveText: "#6a2030",
      aiBg: "#faecef", aiText: "#6a2030", aiTabText: "#a87888",
      userBubbleBg: "#f5d5dc", userBubbleText: "#6a2030",
      aiBubbleBg: "#ffffff", aiBubbleText: "#6a2030", aiBubbleBorder: "#e0a8b8",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#6a2030", inputBorder: "#e0a8b8",
      accent: "#d94a68", accentText: "#ffffff",
      border: "#f0d5da",
      chipBg: "#f8e0e6", chipText: "#8a4050", chipBorder: "#e0a8b8",
    },
    dark: {
      bg: "#2a0e16", bgText: "#fae4ea",
      sidebarBg: "#1e080e", sidebarText: "#c08a98", sidebarHover: "#40121e", sidebarSelected: "#5c1a2a", sidebarHeader: "#a87888",
      editorBg: "#2a0e16",
      tabsBg: "#1e080e", tabBg: "#2a0e16", tabText: "#a87888", tabActiveBg: "#40121e", tabActiveText: "#fae4ea",
      aiBg: "#1e080e", aiText: "#f0d5dc", aiTabText: "#a87888",
      userBubbleBg: "#5c1a2a", userBubbleText: "#fae8ee",
      aiBubbleBg: "#40121e", aiBubbleText: "#f0d5dc", aiBubbleBorder: "#5c1a2a",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#40121e", inputText: "#f0d5dc", inputBorder: "#6a2a3a",
      accent: "#f08aa8", accentText: "#2a0a12",
      border: "#40121e",
      chipBg: "#40121e", chipText: "#f0c0cc", chipBorder: "#6a2a3a",
    },
  },
  mascot: { light: imgLinnea, dark: imgLinnea },
};

/**
 * 爱可菲·花刀技法风格 —— 源自 Genshen-Escoffier-Skin
 * 调色板直接取自壁纸原色：蓝莓夜蓝（#10242e/#0a1a22）为整个编辑器底色，
 * 奶油瓷白（#fbfaf4）、冰蓝（#3fb8e0）、樱红（#d94a68）点缀
 */
const escoffierSkin: SkinDefinition = {
  id: "genshin-escoffier",
  name: "爱可菲 · 花刀技法",
  source: "https://github.com/WPH666-py/Genshen-Escoffier-Skin",
  builtin: true,
  description: "花刀技法：奶油瓷白与冰蓝，暗色为蓝莓夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#fbfaf4", bgText: "#1d4a5e",
      sidebarBg: "#e8f2f4", sidebarText: "#2d6a8a", sidebarHover: "#d3e8ee", sidebarSelected: "#b8dce8", sidebarHeader: "#6f9ab0",
      editorBg: "#fbfaf4",
      tabsBg: "#e8f2f4", tabBg: "#e8f2f4", tabText: "#6f92a0", tabActiveBg: "#fbfaf4", tabActiveText: "#1d4a5e",
      aiBg: "#f0f6f8", aiText: "#1d4a5e", aiTabText: "#6f92a0",
      userBubbleBg: "#cfe8f2", userBubbleText: "#1d4a5e",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1d4a5e", aiBubbleBorder: "#a5d4e0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1d4a5e", inputBorder: "#a5d4e0",
      accent: "#3fb8e0", accentText: "#ffffff",
      border: "#d3e8ee",
      chipBg: "#e0f0f4", chipText: "#2d6a8a", chipBorder: "#a5d4e0",
    },
    dark: {
      bg: "#10242e", bgText: "#e0f4fa",
      sidebarBg: "#0a1a22", sidebarText: "#9cc2d0", sidebarHover: "#1a3a48", sidebarSelected: "#2a5468", sidebarHeader: "#6f92a0",
      editorBg: "#10242e",
      tabsBg: "#0a1a22", tabBg: "#10242e", tabText: "#6f92a0", tabActiveBg: "#1a3a48", tabActiveText: "#e0f4fa",
      aiBg: "#0a1a22", aiText: "#dceff8", aiTabText: "#6f92a0",
      userBubbleBg: "#2a5468", userBubbleText: "#eef8fc",
      aiBubbleBg: "#1a3a48", aiBubbleText: "#dceff8", aiBubbleBorder: "#2a5468",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1a3a48", inputText: "#dceff8", inputBorder: "#3a6a80",
      accent: "#5fd0f0", accentText: "#0a2434",
      border: "#1a3a48",
      chipBg: "#1a3a48", chipText: "#c4e8f2", chipBorder: "#3a6a80",
    },
  },
  mascot: { light: imgEscoffier, dark: imgEscoffier },
};

/**
 * 娜维娅·如霰澄天的鸣礼风格 —— 源自 Genshen-Navia-Skin
 * 调色板直接取自壁纸原色：曜石夜紫（#201826/#161020）为整个编辑器底色，
 * 鎏金瓷白（#fdf8ee）、金穗（#c8962c）、蓝宝石（#4a6ad8）点缀
 */
const naviaSkin: SkinDefinition = {
  id: "genshin-navia",
  name: "娜维娅 · 如霰澄天的鸣礼",
  source: "https://github.com/WPH666-py/Genshen-Navia-Skin",
  builtin: true,
  description: "如霰澄天的鸣礼：鎏金瓷白与黑羽，暗色为曜石夜紫（取壁纸原色）",
  palettes: {
    light: {
      bg: "#fdf8ee", bgText: "#4a3a26",
      sidebarBg: "#f2ecdc", sidebarText: "#8a6a30", sidebarHover: "#e8dfc8", sidebarSelected: "#e8c880", sidebarHeader: "#a08a60",
      editorBg: "#fdf8ee",
      tabsBg: "#f2ecdc", tabBg: "#f2ecdc", tabText: "#96869e", tabActiveBg: "#fdf8ee", tabActiveText: "#4a3a26",
      aiBg: "#f8f2e4", aiText: "#4a3a26", aiTabText: "#96869e",
      userBubbleBg: "#f0e0c0", userBubbleText: "#4a3a26",
      aiBubbleBg: "#ffffff", aiBubbleText: "#4a3a26", aiBubbleBorder: "#d8c090",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#4a3a26", inputBorder: "#d8c090",
      accent: "#c8962c", accentText: "#ffffff",
      border: "#e8dfc8",
      chipBg: "#f4e6c8", chipText: "#8a6a30", chipBorder: "#d8c090",
    },
    dark: {
      bg: "#201826", bgText: "#f4ead8",
      sidebarBg: "#161020", sidebarText: "#a89aae", sidebarHover: "#342a44", sidebarSelected: "#4a3a5e", sidebarHeader: "#96869e",
      editorBg: "#201826",
      tabsBg: "#161020", tabBg: "#201826", tabText: "#96869e", tabActiveBg: "#342a44", tabActiveText: "#f4ead8",
      aiBg: "#161020", aiText: "#e8e0d0", aiTabText: "#96869e",
      userBubbleBg: "#4a3a5e", userBubbleText: "#f8efe0",
      aiBubbleBg: "#342a44", aiBubbleText: "#e8e0d0", aiBubbleBorder: "#4a3a5e",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#342a44", inputText: "#e8e0d0", inputBorder: "#5a4a6e",
      accent: "#e8b848", accentText: "#2a1f08",
      border: "#342a44",
      chipBg: "#2e2438", chipText: "#e8d0a0", chipBorder: "#5a4a6e",
    },
  },
  mascot: { light: imgNavia, dark: imgNavia },
};

/**
 * 玛拉妮·爆瀑飞弹风格 —— 源自 Genshen-Mualani-Skin
 * 调色板直接取自壁纸原色：深浪夜蓝（#0e2a3e/#0a1f2e）为整个编辑器底色，
 * 海蓝瓷白（#f4f9fb）、浪花白（#ffffff）、鲨金（#e0b350）点缀
 */
const mualaniSkin: SkinDefinition = {
  id: "genshin-mualani",
  name: "玛拉妮 · 爆瀑飞弹",
  source: "https://github.com/WPH666-py/Genshen-Mualani-Skin",
  builtin: true,
  description: "爆瀑飞弹：海蓝瓷白与浪花，暗色为深浪夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f4f9fb", bgText: "#14506e",
      sidebarBg: "#e1eef4", sidebarText: "#2d6a8a", sidebarHover: "#d0e6f0", sidebarSelected: "#b8dcec", sidebarHeader: "#6d94a8",
      editorBg: "#f4f9fb",
      tabsBg: "#e1eef4", tabBg: "#e1eef4", tabText: "#6d93a8", tabActiveBg: "#f4f9fb", tabActiveText: "#14506e",
      aiBg: "#ecf5f9", aiText: "#14506e", aiTabText: "#6d93a8",
      userBubbleBg: "#cce4f0", userBubbleText: "#14506e",
      aiBubbleBg: "#ffffff", aiBubbleText: "#14506e", aiBubbleBorder: "#a5ccdc",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#14506e", inputBorder: "#a5ccdc",
      accent: "#2f9fd8", accentText: "#ffffff",
      border: "#d0e6f0",
      chipBg: "#dcecf4", chipText: "#2d6a8a", chipBorder: "#a5ccdc",
    },
    dark: {
      bg: "#0e2a3e", bgText: "#e0f4fb",
      sidebarBg: "#0a1f2e", sidebarText: "#9cc4d8", sidebarHover: "#16384f", sidebarSelected: "#1f5678", sidebarHeader: "#6d93a8",
      editorBg: "#0e2a3e",
      tabsBg: "#0a1f2e", tabBg: "#0e2a3e", tabText: "#6d93a8", tabActiveBg: "#16384f", tabActiveText: "#e0f4fb",
      aiBg: "#0a1f2e", aiText: "#dceff8", aiTabText: "#6d93a8",
      userBubbleBg: "#1f5678", userBubbleText: "#eef8fc",
      aiBubbleBg: "#16384f", aiBubbleText: "#dceff8", aiBubbleBorder: "#1f5678",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#16384f", inputText: "#dceff8", inputBorder: "#2c6a8a",
      accent: "#5fc6e8", accentText: "#0a2434",
      border: "#16384f",
      chipBg: "#16384f", chipText: "#c4e8f2", chipBorder: "#2c6a8a",
    },
  },
  mascot: { light: imgMualani, dark: imgMualani },
};

/**
 * 桑多涅·事象数式万理证毕风格 —— 源自 Genshen-Sandrone-Skin
 * 调色板直接取自壁纸原色：星渊夜蓝（#12122a/#0c0c1e）为整个编辑器底色，
 * 象牙瓷白（#f8f4ee）、赤绯（#a02838）、鎏金（#e8b848）点缀
 */
const sandroneSkin: SkinDefinition = {
  id: "genshin-sandrone",
  name: "桑多涅 · 事象数式万理证毕",
  source: "https://github.com/WPH666-py/Genshen-Sandrone-Skin",
  builtin: true,
  description: "事象数式万理证毕：瓷白沙金与赤绯，暗色为星渊夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f8f4ee", bgText: "#2a2a38",
      sidebarBg: "#ece8f0", sidebarText: "#8a3040", sidebarHover: "#ddd8e8", sidebarSelected: "#f0d8b8", sidebarHeader: "#8a8494",
      editorBg: "#f8f4ee",
      tabsBg: "#ece8f0", tabBg: "#ece8f0", tabText: "#8a8aa8", tabActiveBg: "#f8f4ee", tabActiveText: "#2a2a38",
      aiBg: "#f4f0f4", aiText: "#2a2a38", aiTabText: "#8a8494",
      userBubbleBg: "#f0dec8", userBubbleText: "#2a2a38",
      aiBubbleBg: "#ffffff", aiBubbleText: "#2a2a38", aiBubbleBorder: "#d0c0c8",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#2a2a38", inputBorder: "#d0c0c8",
      accent: "#a02838", accentText: "#ffffff",
      border: "#ddd8e8",
      chipBg: "#f0e0e2", chipText: "#8a3040", chipBorder: "#d0a8b0",
    },
    dark: {
      bg: "#12122a", bgText: "#e8e8f8",
      sidebarBg: "#0c0c1e", sidebarText: "#a8a0b8", sidebarHover: "#20203c", sidebarSelected: "#3a1430", sidebarHeader: "#8a8aa8",
      editorBg: "#12122a",
      tabsBg: "#0c0c1e", tabBg: "#12122a", tabText: "#8a8aa8", tabActiveBg: "#20203c", tabActiveText: "#e8e8f8",
      aiBg: "#0c0c1e", aiText: "#e0e0f4", aiTabText: "#8a8aa8",
      userBubbleBg: "#3a1430", userBubbleText: "#f0e8f0",
      aiBubbleBg: "#20203c", aiBubbleText: "#e0e0f4", aiBubbleBorder: "#4a1a2e",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#20203c", inputText: "#e0e0f4", inputBorder: "#5a5a80",
      accent: "#e8b848", accentText: "#241a08",
      border: "#20203c",
      chipBg: "#20203c", chipText: "#e8d8b0", chipBorder: "#5a4a2e",
    },
  },
  mascot: { light: imgSandrone, dark: imgSandrone },
};

/**
 * 克洛琳德·秉烛剔星月风格 —— 源自 Genshen-Clorinde-Skin
 * 调色板直接取自壁纸原色：雷夜紫渊（#1a1440/#120f2c）为整个编辑器底色，
 * 紫电瓷白（#f4f2fa）、迅捷紫电（#7a5fd8）、金芒（#d8a850）点缀
 */
const clorindeSkin: SkinDefinition = {
  id: "genshin-clorinde",
  name: "克洛琳德 · 秉烛剔星月",
  source: "https://github.com/WPH666-py/Genshen-Clorinde-Skin",
  builtin: true,
  description: "秉烛剔星月：紫电瓷白与金芒，暗色为雷夜紫渊（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f4f2fa", bgText: "#2a2350",
      sidebarBg: "#e9e6f5", sidebarText: "#5a4a8a", sidebarHover: "#dbd6ee", sidebarSelected: "#c8bfe8", sidebarHeader: "#8a80b0",
      editorBg: "#f4f2fa",
      tabsBg: "#e9e6f5", tabBg: "#e9e6f5", tabText: "#8a86a8", tabActiveBg: "#f4f2fa", tabActiveText: "#2a2350",
      aiBg: "#efeaf9", aiText: "#2a2350", aiTabText: "#8a86a8",
      userBubbleBg: "#e0d8f5", userBubbleText: "#2a2350",
      aiBubbleBg: "#ffffff", aiBubbleText: "#2a2350", aiBubbleBorder: "#b8b0d8",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#2a2350", inputBorder: "#b8b0d8",
      accent: "#7a5fd8", accentText: "#ffffff",
      border: "#dbd6ee",
      chipBg: "#e8e2f8", chipText: "#5a4a8a", chipBorder: "#b8b0d8",
    },
    dark: {
      bg: "#1a1440", bgText: "#e8ecfa",
      sidebarBg: "#120f2c", sidebarText: "#a8a0c0", sidebarHover: "#2a2a52", sidebarSelected: "#3a2e6e", sidebarHeader: "#8a86a8",
      editorBg: "#1a1440",
      tabsBg: "#120f2c", tabBg: "#1a1440", tabText: "#8a86a8", tabActiveBg: "#2a2a52", tabActiveText: "#e8ecfa",
      aiBg: "#120f2c", aiText: "#e2e0f6", aiTabText: "#8a86a8",
      userBubbleBg: "#3a2e6e", userBubbleText: "#f0eefc",
      aiBubbleBg: "#2a2a52", aiBubbleText: "#e2e0f6", aiBubbleBorder: "#3a2e6e",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#2a2a52", inputText: "#e2e0f6", inputBorder: "#5a4a9e",
      accent: "#b48cf0", accentText: "#1a1030",
      border: "#2a2a52",
      chipBg: "#2a2a52", chipText: "#d8ccf0", chipBorder: "#5a549e",
    },
  },
  mascot: { light: imgClorinde, dark: imgClorinde },
};

/**
 * 尼可·圣言默示天路历程风格 —— 源自 Genshen-Nicole-Skin
 * 调色板直接取自壁纸原色：星辉夜蓝（#101c36/#0a1224）为整个编辑器底色，
 * 雪蓝瓷白（#f8fafe）、圣蓝（#4a8fd8）、鎏金（#e0b350）点缀
 */
const nicoleSkin: SkinDefinition = {
  id: "genshin-nicole",
  name: "尼可 · 圣言默示天路历程",
  source: "https://github.com/WPH666-py/Genshen-Nicole-Skin",
  builtin: true,
  description: "圣言默示天路历程：雪蓝瓷白与鎏金，暗色为星辉夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f8fafe", bgText: "#21456e",
      sidebarBg: "#e6eef8", sidebarText: "#2d6a8a", sidebarHover: "#d4e2f2", sidebarSelected: "#b8d4ec", sidebarHeader: "#6f93b8",
      editorBg: "#f8fafe",
      tabsBg: "#e6eef8", tabBg: "#e6eef8", tabText: "#7a94b8", tabActiveBg: "#f8fafe", tabActiveText: "#21456e",
      aiBg: "#eef4fc", aiText: "#21456e", aiTabText: "#7a94b8",
      userBubbleBg: "#d6eaf8", userBubbleText: "#21456e",
      aiBubbleBg: "#ffffff", aiBubbleText: "#21456e", aiBubbleBorder: "#a8c8e0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#21456e", inputBorder: "#a8c8e0",
      accent: "#4a8fd8", accentText: "#ffffff",
      border: "#d4e2f2",
      chipBg: "#dceaf6", chipText: "#2d6a8a", chipBorder: "#a8c8e0",
    },
    dark: {
      bg: "#101c36", bgText: "#e0e8fa",
      sidebarBg: "#0a1224", sidebarText: "#9cb8d0", sidebarHover: "#1c2c4e", sidebarSelected: "#2a4070", sidebarHeader: "#7a94b8",
      editorBg: "#101c36",
      tabsBg: "#0a1224", tabBg: "#101c36", tabText: "#7a94b8", tabActiveBg: "#1c2c4e", tabActiveText: "#e0e8fa",
      aiBg: "#0a1224", aiText: "#dce8fa", aiTabText: "#7a94b8",
      userBubbleBg: "#2a4070", userBubbleText: "#eef4fd",
      aiBubbleBg: "#1c2c4e", aiBubbleText: "#dce8fa", aiBubbleBorder: "#2a4070",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1c2c4e", inputText: "#dce8fa", inputBorder: "#3d5a88",
      accent: "#6fb8f0", accentText: "#0a1a30",
      border: "#1c2c4e",
      chipBg: "#1c2c4e", chipText: "#bcd8f0", chipBorder: "#3d5a88",
    },
  },
  mascot: { light: imgNicole, dark: imgNicole },
};

/**
 * 砂糖·禁风灵作成柒伍式风格 —— 源自 Genshen-Sucrose-Skin
 * 调色板直接取自壁纸原色：林中夜绿（#1a2416/#121a0e）为整个编辑器底色，
 * 薄荷瓷白（#f6f8f2）、薄荷绿（#7cae5a）、琥珀（#e09c40）点缀
 */
const sucroseSkin: SkinDefinition = {
  id: "genshin-sucrose",
  name: "砂糖 · 禁风灵作成柒伍式",
  source: "https://github.com/WPH666-py/Genshen-Sucrose-Skin",
  builtin: true,
  description: "禁风灵作成柒伍式：薄荷瓷白与琥珀，暗色为林中夜绿（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f6f8f2", bgText: "#2a4028",
      sidebarBg: "#e6ecdd", sidebarText: "#5c7a46", sidebarHover: "#d8e2c8", sidebarSelected: "#c2d8ac", sidebarHeader: "#7d9470",
      editorBg: "#f6f8f2",
      tabsBg: "#e6ecdd", tabBg: "#e6ecdd", tabText: "#7d9070", tabActiveBg: "#f6f8f2", tabActiveText: "#2a4028",
      aiBg: "#eef2e8", aiText: "#2a4028", aiTabText: "#7d9070",
      userBubbleBg: "#e0ead0", userBubbleText: "#2a4028",
      aiBubbleBg: "#ffffff", aiBubbleText: "#2a4028", aiBubbleBorder: "#b8cc9a",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#2a4028", inputBorder: "#b8cc9a",
      accent: "#e09c40", accentText: "#ffffff",
      border: "#d8e2c8",
      chipBg: "#e4ecd6", chipText: "#5c7a46", chipBorder: "#b8cc9a",
    },
    dark: {
      bg: "#1a2416", bgText: "#e6f0da",
      sidebarBg: "#121a0e", sidebarText: "#a8b890", sidebarHover: "#28361e", sidebarSelected: "#3a4e2a", sidebarHeader: "#7d9070",
      editorBg: "#1a2416",
      tabsBg: "#121a0e", tabBg: "#1a2416", tabText: "#7d9070", tabActiveBg: "#28361e", tabActiveText: "#e6f0da",
      aiBg: "#121a0e", aiText: "#e0ecd6", aiTabText: "#7d9070",
      userBubbleBg: "#3a4e2a", userBubbleText: "#eef6e6",
      aiBubbleBg: "#28361e", aiBubbleText: "#e0ecd6", aiBubbleBorder: "#3a4e2a",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#28361e", inputText: "#e0ecd6", inputBorder: "#4a6040",
      accent: "#f0b860", accentText: "#2a1f08",
      border: "#28361e",
      chipBg: "#28361e", chipText: "#d8e8c0", chipBorder: "#4a6040",
    },
  },
  mascot: { light: imgSucrose, dark: imgSucrose },
};

/**
 * 优菈·凝浪之光剑风格 —— 源自 Genshen-Eula-Skin
 * 调色板直接取自壁纸原色：霜雪夜蓝（#12243e/#0c1a2c）为整个编辑器底色，
 * 冰蓝瓷白（#eff6fb）、冰之蓝（#5a90e0）、霜紫（#8a6ae8）点缀
 */
const eulaSkin: SkinDefinition = {
  id: "genshin-eula",
  name: "优菈 · 凝浪之光剑",
  source: "https://github.com/WPH666-py/Genshen-Eula-Skin",
  builtin: true,
  description: "凝浪之光剑：冰蓝瓷白与霜紫，暗色为霜雪夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#eff6fb", bgText: "#1d3c62",
      sidebarBg: "#dfeaf5", sidebarText: "#3d6a8a", sidebarHover: "#cfe0f0", sidebarSelected: "#b8d4ec", sidebarHeader: "#6f93ae",
      editorBg: "#eff6fb",
      tabsBg: "#dfeaf5", tabBg: "#dfeaf5", tabText: "#6f86a8", tabActiveBg: "#eff6fb", tabActiveText: "#1d3c62",
      aiBg: "#e7f0f9", aiText: "#1d3c62", aiTabText: "#6f86a8",
      userBubbleBg: "#cfe6f8", userBubbleText: "#1d3c62",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1d3c62", aiBubbleBorder: "#a5c8e0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1d3c62", inputBorder: "#a5c4e0",
      accent: "#5a90e0", accentText: "#ffffff",
      border: "#cfe0f0",
      chipBg: "#d9e8f5", chipText: "#3d6a8a", chipBorder: "#a5c4e0",
    },
    dark: {
      bg: "#12243e", bgText: "#e0ecfa",
      sidebarBg: "#0c1a2c", sidebarText: "#9cb8d0", sidebarHover: "#1a2a4c", sidebarSelected: "#2a4070", sidebarHeader: "#6f86a8",
      editorBg: "#12243e",
      tabsBg: "#0c1a2c", tabBg: "#12243e", tabText: "#6f86a8", tabActiveBg: "#1a2a4c", tabActiveText: "#e0ecfa",
      aiBg: "#0c1a2c", aiText: "#dceefa", aiTabText: "#6f86a8",
      userBubbleBg: "#2a4070", userBubbleText: "#eef8ff",
      aiBubbleBg: "#1a2a4c", aiBubbleText: "#dceefa", aiBubbleBorder: "#2a4070",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1a2a4c", inputText: "#dceefa", inputBorder: "#3d5a88",
      accent: "#7fc0f0", accentText: "#0a1a30",
      border: "#1a2a4c",
      chipBg: "#1a2a4c", chipText: "#bcd8f0", chipBorder: "#3d5a88",
    },
  },
  mascot: { light: imgEula, dark: imgEula },
};

/**
 * 申鹤·神女遣灵真诀风格 —— 源自 Genshen-Shenhe-Skin
 * 调色板直接取自壁纸原色：霜夜蓝（#162a3e/#0e1e2e）为整个编辑器底色，
 * 银白瓷蓝（#f4f8fc）、冰蓝（#5a90d8）、朱红（#c03a3a）点缀
 */
const shenheSkin: SkinDefinition = {
  id: "genshin-shenhe",
  name: "申鹤 · 神女遣灵真诀",
  source: "https://github.com/WPH666-py/Genshen-Shenhe-Skin",
  builtin: true,
  description: "神女遣灵真诀：银白瓷蓝与朱红，暗色为霜夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f4f8fc", bgText: "#1d3a5e",
      sidebarBg: "#e0ebf5", sidebarText: "#3d6a8a", sidebarHover: "#d0e0f0", sidebarSelected: "#b8d4ec", sidebarHeader: "#6f93ae",
      editorBg: "#f4f8fc",
      tabsBg: "#e0ebf5", tabBg: "#e0ebf5", tabText: "#6f86a8", tabActiveBg: "#f4f8fc", tabActiveText: "#1d3a5e",
      aiBg: "#ecf3fa", aiText: "#1d3a5e", aiTabText: "#6f86a8",
      userBubbleBg: "#cfe6f8", userBubbleText: "#1d3a5e",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1d3a5e", aiBubbleBorder: "#a5c8e0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1d3a5e", inputBorder: "#a5c4e0",
      accent: "#5a90d8", accentText: "#ffffff",
      border: "#d0e0f0",
      chipBg: "#d9e8f5", chipText: "#3d6a8a", chipBorder: "#a5c4e0",
    },
    dark: {
      bg: "#162a3e", bgText: "#e0ecfa",
      sidebarBg: "#0e1e2e", sidebarText: "#9cb8d0", sidebarHover: "#1e3450", sidebarSelected: "#2c4a6e", sidebarHeader: "#6f86a8",
      editorBg: "#162a3e",
      tabsBg: "#0e1e2e", tabBg: "#162a3e", tabText: "#6f86a8", tabActiveBg: "#1e3450", tabActiveText: "#e0ecfa",
      aiBg: "#0e1e2e", aiText: "#dceefa", aiTabText: "#6f86a8",
      userBubbleBg: "#2c4a6e", userBubbleText: "#eef8ff",
      aiBubbleBg: "#1e3450", aiBubbleText: "#dceefa", aiBubbleBorder: "#2c4a6e",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1e3450", inputText: "#dceefa", inputBorder: "#3d5a88",
      accent: "#7fc0f0", accentText: "#0a1a30",
      border: "#1e3450",
      chipBg: "#1e3450", chipText: "#bcd8f0", chipBorder: "#3d5a88",
    },
  },
  mascot: { light: imgShenhe, dark: imgShenhe },
};

/**
 * 荧·六元素随机风格 —— 源自 Genshen-Lumine-Skin
 * 调色板直接取自壁纸原色：星穹夜蓝（#0e1a34/#0a1224）为整个编辑器底色，
 * 星海蓝白（#f4f8fc）、蓝金（#4a8fd8）、星光（#e0b860）点缀
 */
const lumineSkin: SkinDefinition = {
  id: "genshin-lumine",
  name: "荧 · 六元素随机",
  source: "https://github.com/WPH666-py/Genshen-Lumine-Skin",
  builtin: true,
  description: "荧的六元素随机：星海蓝白与金芒，暗色为星穹夜蓝（取壁纸原色）",
  palettes: {
    light: {
      bg: "#f4f8fc", bgText: "#1d3a5e",
      sidebarBg: "#e4ecfa", sidebarText: "#3d6a8a", sidebarHover: "#d0e0f2", sidebarSelected: "#bcd4ec", sidebarHeader: "#7186a8",
      editorBg: "#f4f8fc",
      tabsBg: "#e4ecfa", tabBg: "#e4ecfa", tabText: "#7188a8", tabActiveBg: "#f4f8fc", tabActiveText: "#1d3a5e",
      aiBg: "#ecf2fc", aiText: "#1d3a5e", aiTabText: "#7188a8",
      userBubbleBg: "#d0e4f8", userBubbleText: "#1d3a5e",
      aiBubbleBg: "#ffffff", aiBubbleText: "#1d3a5e", aiBubbleBorder: "#a8c4e0",
      systemBubbleBg: "#fdf3e0", systemBubbleText: "#8a6414",
      inputBg: "#ffffff", inputText: "#1d3a5e", inputBorder: "#a8c4e0",
      accent: "#4a8fd8", accentText: "#ffffff",
      border: "#d0e0f2",
      chipBg: "#dce9f6", chipText: "#3d6a8a", chipBorder: "#a8c4e0",
    },
    dark: {
      bg: "#0e1a34", bgText: "#e0ecfa",
      sidebarBg: "#0a1224", sidebarText: "#9caec8", sidebarHover: "#1c2c50", sidebarSelected: "#2a4074", sidebarHeader: "#7188a8",
      editorBg: "#0e1a34",
      tabsBg: "#0a1224", tabBg: "#0e1a34", tabText: "#7188a8", tabActiveBg: "#1c2c50", tabActiveText: "#e0ecfa",
      aiBg: "#0a1224", aiText: "#dceafa", aiTabText: "#7188a8",
      userBubbleBg: "#2a4074", userBubbleText: "#eef6ff",
      aiBubbleBg: "#1c2c50", aiBubbleText: "#dceafa", aiBubbleBorder: "#2a4074",
      systemBubbleBg: "#3d3420", systemBubbleText: "#e8d9a0",
      inputBg: "#1c2c50", inputText: "#dceafa", inputBorder: "#4a5e8a",
      accent: "#e0b860", accentText: "#241a08",
      border: "#1c2c50",
      chipBg: "#1c2c50", chipText: "#d8e4f8", chipBorder: "#4a5e8a",
    },
  },
  mascot: { light: imgLumine, dark: imgLumine },
};

export const BUILTIN_SKINS: SkinDefinition[] = [whaleCloud, whaleMaid, whaleAds, furinaSkin, citlaliSkin, keqingSkin, kokomiSkin, ayakaSkin, yoimiyaSkin, shogunSkin, nahidaSkin, nilouSkin, colleiSkin, noelleSkin, barbaraSkin, amborSkin, yelanSkin, zibaiSkin, ganyuSkin, columbinaSkin, linneaSkin, escoffierSkin, naviaSkin, mualaniSkin, sandroneSkin, clorindeSkin, nicoleSkin, sucroseSkin, eulaSkin, shenheSkin, lumineSkin];

// ───────────────────────── 自定义皮肤持久化 ─────────────────────────

const CUSTOM_SKINS_KEY = "DeepAhead-custom-skins";

export function getCustomSkins(): SkinDefinition[] {
  try {
    const raw = localStorage.getItem(CUSTOM_SKINS_KEY);
    return raw ? (JSON.parse(raw) as SkinDefinition[]) : [];
  } catch {
    return [];
  }
}

export function saveCustomSkins(skins: SkinDefinition[]) {
  localStorage.setItem(CUSTOM_SKINS_KEY, JSON.stringify(skins));
}

export function addCustomSkin(skin: SkinDefinition) {
  const list = getCustomSkins().filter((s) => s.id !== skin.id);
  list.push({ ...skin, builtin: false });
  saveCustomSkins(list);
}

/** 仅允许删除自定义皮肤；内置皮肤会被忽略 */
export function removeCustomSkin(id: string): boolean {
  if (BUILTIN_SKINS.some((s) => s.id === id)) return false;
  const list = getCustomSkins();
  const next = list.filter((s) => s.id !== id);
  if (next.length === list.length) return false;
  saveCustomSkins(next);
  return true;
}

export function getAllSkins(): SkinDefinition[] {
  return [...BUILTIN_SKINS, ...getCustomSkins()];
}

export function getSkinById(id: string): SkinDefinition | undefined {
  return getAllSkins().find((s) => s.id === id);
}

// ───────────────────────── 皮肤应用 ─────────────────────────

const STYLE_TAG_ID = "DeepAhead-skin-style";

/** 由调色板生成覆盖 CSS（作用于文件树 / 编辑区 / AI 区域） */
export function buildSkinCss(p: SkinPalette, mascotUrl?: string): string {
  const mascotCss = mascotUrl
    ? `
.editor-main-content::after {
  content: ""; position: absolute; right: 20px; bottom: 20px;
  width: 380px; height: 380px; pointer-events: none; z-index: 5;
  background: url("${mascotUrl}") no-repeat right bottom / contain;
  opacity: 1; border-radius: 14px;
  filter: drop-shadow(0 8px 20px rgba(0, 0, 0, 0.3)) saturate(1.3) contrast(1.08);
}`
    : "";
  return `
/* DeepAhead 皮肤覆盖层（自动生成，请勿手动编辑） */
.editor-header { background: ${p.bg} !important; border-bottom-color: ${p.border} !important; }
.editor-header, .editor-header .menu-button, .editor-header .env-select, .editor-header select { color: ${p.bgText} !important; }
.editor-header .menu-button:hover { background: ${p.sidebarHover} !important; }
.editor-header .runtime-select, .editor-header .runfile-select, .editor-header .browser-select {
  background: ${p.inputBg} !important; color: ${p.inputText} !important;
  border: 1px solid ${p.inputBorder} !important;
}
.editor-header .runtime-select:focus, .editor-header .runfile-select:focus, .editor-header .browser-select:focus {
  border-color: ${p.accent} !important; outline: none;
}
.editor-header .runtime-select option, .editor-header .runfile-select option, .editor-header .browser-select option {
  background: ${p.inputBg} !important; color: ${p.inputText} !important;
}
.editor-header .add-runtime-btn {
  background: ${p.inputBg} !important; color: ${p.accent} !important; border: 1px solid ${p.inputBorder} !important;
}
.editor-header .add-runtime-btn:hover { background: ${p.sidebarHover} !important; }

.file-explorer { background: ${p.sidebarBg} !important; border-right-color: ${p.border} !important; }
.file-explorer-header { color: ${p.sidebarHeader} !important; border-bottom-color: ${p.border} !important; }
.file-explorer .file-item { color: ${p.sidebarText} !important; }
.file-explorer .file-item:hover { background: ${p.sidebarHover} !important; }
.file-explorer .file-item.selected { background: ${p.sidebarSelected} !important; }
.file-explorer .file-item.open { background: ${p.sidebarSelected} !important; }
.file-explorer .file-item.open:hover { background: ${p.sidebarHover} !important; }

.editor-area { background: ${p.editorBg} !important; }
.tabs-bar { background: ${p.tabsBg} !important; border-bottom-color: ${p.border} !important; }
.tabs-bar .tab { background: ${p.tabBg} !important; color: ${p.tabText} !important; }
.tabs-bar .tab.active { background: ${p.tabActiveBg} !important; color: ${p.tabActiveText} !important; border-top-color: ${p.accent} !important; }
.editor-main-content { background: ${p.editorBg} !important; position: relative; }
.editor-empty { background: ${p.editorBg} !important; color: ${p.aiTabText} !important; }
.editor-empty .empty-text, .editor-empty .empty-sub { color: ${p.aiTabText} !important; }

.ai-panel { background: ${p.aiBg} !important; border-left-color: ${p.border} !important; color: ${p.aiText} !important; }
.ai-panel-tabs { border-bottom-color: ${p.border} !important; }
.ai-panel-tabs .ai-tab { color: ${p.aiTabText} !important; }
.ai-panel-tabs .ai-tab.active { color: ${p.aiText} !important; border-bottom-color: ${p.accent} !important; }
.ai-panel .message.user-message { background: ${p.userBubbleBg} !important; color: ${p.userBubbleText} !important; }
.ai-panel .message.ai-message { background: ${p.aiBubbleBg} !important; color: ${p.aiBubbleText} !important; border-color: ${p.aiBubbleBorder} !important; }
.ai-panel .message.system-message { background: ${p.systemBubbleBg} !important; color: ${p.systemBubbleText} !important; border-color: ${p.chipBorder} !important; }
.ai-panel .msg-role { color: ${p.aiTabText} !important; }
.ai-input-area { border-top-color: ${p.border} !important; }
.ai-input-area textarea { background: ${p.inputBg} !important; color: ${p.inputText} !important; border-color: ${p.inputBorder} !important; }
.ai-input-area textarea:focus { border-color: ${p.accent} !important; }
.ai-input-area select { background: ${p.inputBg} !important; color: ${p.inputText} !important; border-color: ${p.inputBorder} !important; }
.ai-send-btn { background: ${p.accent} !important; color: ${p.accentText} !important; }
.ai-send-btn:hover:not(:disabled) { filter: brightness(1.12); }
.ai-context-chip { background: ${p.chipBg} !important; color: ${p.chipText} !important; border-color: ${p.chipBorder} !important; }
.ai-context-add-btn { color: ${p.chipText} !important; border-color: ${p.chipBorder} !important; }
.ai-quick-actions .quick-action-btn { background: ${p.aiBubbleBg} !important; color: ${p.aiText} !important; border-color: ${p.border} !important; }
${mascotCss}
`;
}

/** 应用皮肤：skinId 为 null 时恢复默认样式 */
export function applySkin(skinId: string | null, variant: SkinVariant = "light") {
  let tag = document.getElementById(STYLE_TAG_ID) as HTMLStyleElement | null;
  if (!skinId) {
    tag?.remove();
    delete document.body.dataset.skin;
    return;
  }
  const skin = getSkinById(skinId);
  if (!skin) {
    tag?.remove();
    return;
  }
  // 若皮肤没有暗色变体但用户选了暗色，回退到亮色
  const v: SkinVariant = variant === "dark" && !skin.palettes.dark ? "light" : variant;
  const palette = v === "dark" ? skin.palettes.dark! : skin.palettes.light;
  const mascotUrl = v === "dark" ? skin.mascot?.dark ?? skin.mascot?.light : skin.mascot?.light;
  if (!tag) {
    tag = document.createElement("style");
    tag.id = STYLE_TAG_ID;
    document.head.appendChild(tag);
  }
  tag.textContent = buildSkinCss(palette, mascotUrl);
  document.body.dataset.skin = `${skin.id}:${v}`;
}

// ───────────────────────── 颜色工具（converter 也会用） ─────────────────────────

export function hexToRgb(hex: string): [number, number, number] | null {
  const m = hex.replace("#", "");
  if (!/^[0-9a-fA-F]{3}([0-9a-fA-F]{3})?$/.test(m)) return null;
  const full = m.length === 3 ? m.split("").map((c) => c + c).join("") : m;
  return [parseInt(full.slice(0, 2), 16), parseInt(full.slice(2, 4), 16), parseInt(full.slice(4, 6), 16)];
}

export function rgbToHex(r: number, g: number, b: number): string {
  const c = (n: number) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}

/** 向白色混合（amount: 0~1） */
export function lighten(hex: string, amount: number): string {
  const rgb = hexToRgb(hex);
  if (!rgb) return hex;
  return rgbToHex(rgb[0] + (255 - rgb[0]) * amount, rgb[1] + (255 - rgb[1]) * amount, rgb[2] + (255 - rgb[2]) * amount);
}

/** 向黑色混合（amount: 0~1） */
export function darken(hex: string, amount: number): string {
  const rgb = hexToRgb(hex);
  if (!rgb) return hex;
  return rgbToHex(rgb[0] * (1 - amount), rgb[1] * (1 - amount), rgb[2] * (1 - amount));
}

/** 粗略判断颜色是否为浅色 */
export function isLightColor(hex: string): boolean {
  const rgb = hexToRgb(hex);
  if (!rgb) return true;
  return (rgb[0] * 299 + rgb[1] * 587 + rgb[2] * 114) / 1000 > 150;
}

// 开发模式下皮肤代码热更新后，自动重绘当前皮肤（无需手动重选）
if (import.meta.hot) {
  import.meta.hot.accept(() => {
    const id = localStorage.getItem("DeepAhead-skin-id");
    const v = (localStorage.getItem("DeepAhead-skin-variant") as SkinVariant) || "light";
    applySkin(id, v);
  });
}
