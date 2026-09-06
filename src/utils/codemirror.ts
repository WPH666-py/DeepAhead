import { EditorView, keymap, drawSelection, highlightActiveLine, highlightSpecialChars, lineNumbers } from "@codemirror/view";
import { EditorState, StateEffect } from "@codemirror/state";
import { indentWithTab, history, defaultKeymap, historyKeymap } from "@codemirror/commands";
import { indentOnInput, bracketMatching, foldGutter, foldKeymap } from "@codemirror/language";
import { closeBrackets, closeBracketsKeymap, autocompletion, completionKeymap } from "@codemirror/autocomplete";
import { lintKeymap } from "@codemirror/lint";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { javascript } from "@codemirror/lang-javascript";
import { python } from "@codemirror/lang-python";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { rust } from "@codemirror/lang-rust";
import { java } from "@codemirror/lang-java";
import { cpp } from "@codemirror/lang-cpp";
import { go } from "@codemirror/lang-go";
import { sql } from "@codemirror/lang-sql";
import { xml } from "@codemirror/lang-xml";

export type EditorTheme = "classic" | "green" | "dark";

/** 经典纯白主题 */
const classicTheme = EditorView.theme({
  "&": { backgroundColor: "#ffffff", color: "#333" },
  ".cm-content": { caretColor: "#333" },
  "&.cm-focused .cm-cursor": { borderLeftColor: "#333" },
  ".cm-activeLine": { backgroundColor: "#e8f4fd" },
  ".cm-activeLineGutter": { backgroundColor: "#b3d9ff" },
  ".cm-gutters": { backgroundColor: "#f5f5f5", color: "#999", border: "none" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 8px 0 12px" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, .cm-content ::selection": { backgroundColor: "#d7e8ff !important" },
  ".cm-panels": { backgroundColor: "#f5f5f5", color: "#333" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid #ddd" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid #ddd" },
  ".cm-searchMatch": { backgroundColor: "#fff3a0" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "#ffe066" },
  ".cm-tooltip": { backgroundColor: "#fff", border: "1px solid #ddd", borderRadius: "4px" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "2px 6px" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "#d7e8ff", color: "#333" },
  ".cm-foldGutter .cm-gutterElement": { padding: "0 4px" },
  ".cm-foldMarker": { color: "#999" },
});

/** 护眼淡绿主题 */
const greenTheme = EditorView.theme({
  "&": { backgroundColor: "#dff2e2", color: "#1a1a1a" },
  ".cm-content": { caretColor: "#1a1a1a" },
  "&.cm-focused .cm-cursor": { borderLeftColor: "#1a1a1a" },
  ".cm-activeLine": { backgroundColor: "#d0ead6" },
  ".cm-activeLineGutter": { backgroundColor: "#c2e3ca" },
  ".cm-gutters": { backgroundColor: "#c2e3ca", color: "#40513a", border: "none" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 8px 0 12px" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, .cm-content ::selection": { backgroundColor: "#a9d8b4 !important" },
  ".cm-panels": { backgroundColor: "#c2e3ca", color: "#1a1a1a" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid #9fc9ab" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid #9fc9ab" },
  ".cm-searchMatch": { backgroundColor: "#fff176" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "#ffeb3b" },
  ".cm-tooltip": { backgroundColor: "#eaf7ec", border: "1px solid #9fc9ab", borderRadius: "4px" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "2px 6px" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "#a9d8b4", color: "#1a1a1a" },
  ".cm-foldGutter .cm-gutterElement": { padding: "0 4px" },
  ".cm-foldMarker": { color: "#5a715c" },
});

/** 深色专业主题 */
const darkTheme = EditorView.theme({
  "&": { backgroundColor: "#1e1e2e", color: "#cdd6f4" },
  ".cm-content": { caretColor: "#f5e0dc" },
  "&.cm-focused .cm-cursor": { borderLeftColor: "#f5e0dc" },
  ".cm-activeLine": { backgroundColor: "#ffffff", color: "#000000 !important" },
  ".cm-activeLine *": { color: "#000000 !important" },
  ".cm-activeLineGutter": { backgroundColor: "#313244" },
  ".cm-gutters": { backgroundColor: "#181825", color: "#6c7086", border: "none" },
  ".cm-lineNumbers .cm-gutterElement": { padding: "0 8px 0 12px" },
  ".cm-selectionBackground, &.cm-focused .cm-selectionBackground, .cm-content ::selection": { backgroundColor: "#45475a !important" },
  ".cm-panels": { backgroundColor: "#181825", color: "#cdd6f4" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid #313244" },
  ".cm-panels.cm-panels-bottom": { borderTop: "1px solid #313244" },
  ".cm-searchMatch": { backgroundColor: "#f9e2af" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "#fab387" },
  ".cm-tooltip": { backgroundColor: "#1e1e2e", border: "1px solid #313244", borderRadius: "4px" },
  ".cm-tooltip-autocomplete > ul > li": { padding: "2px 6px" },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "#313244", color: "#cdd6f4" },
  ".cm-foldGutter .cm-gutterElement": { padding: "0 4px" },
  ".cm-foldMarker": { color: "#6c7086" },
});

/** 主题映射 */
const themeMap: Record<EditorTheme, any> = {
  classic: classicTheme,
  green: greenTheme,
  dark: darkTheme,
};

/** 深色主题语法高亮覆盖（让代码有彩色） */
const darkHighlightStyle = EditorView.baseTheme({
  ".cm-keyword": { color: "#cba6f7" },
  ".cm-operator": { color: "#89dceb" },
  ".cm-variable": { color: "#cdd6f4" },
  ".cm-variable-2": { color: "#89b4fa" },
  ".cm-variable-3": { color: "#89dceb" },
  ".cm-type": { color: "#f9e2af" },
  ".cm-def": { color: "#89b4fa" },
  ".cm-string": { color: "#a6e3a1" },
  ".cm-string-2": { color: "#f38ba8" },
  ".cm-comment": { color: "#6c7086", fontStyle: "italic" },
  ".cm-number": { color: "#fab387" },
  ".cm-atom": { color: "#fab387" },
  ".cm-meta": { color: "#f5c2e7" },
  ".cm-tag": { color: "#f38ba8" },
  ".cm-attribute": { color: "#89b4fa" },
  ".cm-property": { color: "#89dceb" },
  ".cm-qualifier": { color: "#f9e2af" },
  ".cm-builtin": { color: "#cba6f7" },
  ".cm-function": { color: "#89b4fa" },
  ".cm-bracket": { color: "#f9e2af" },
  ".cm-link": { color: "#89b4fa", textDecoration: "underline" },
  ".cm-error": { color: "#f38ba8" },
  ".cm-invalidchar": { color: "#f38ba8" },
});

/** 手动组合 basicSetup（避免依赖 codemirror meta-package） */
function basicSetup() {
  return [
    lineNumbers(),
    drawSelection(),
    highlightActiveLine(),
    highlightSpecialChars(),
    history(),
    foldGutter(),
    indentOnInput(),
    bracketMatching(),
    closeBrackets(),
    autocompletion(),
    highlightSelectionMatches(),
    keymap.of([
      ...closeBracketsKeymap,
      ...defaultKeymap,
      ...searchKeymap,
      ...historyKeymap,
      ...foldKeymap,
      ...completionKeymap,
      ...lintKeymap,
      indentWithTab,
    ]),
  ];
}

/** 根据文件扩展名获取对应的语言扩展 */
function getLanguageExtension(filename: string) {
  const ext = filename.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "js":
    case "jsx":
    case "ts":
    case "tsx":
      return javascript({ typescript: ext === "ts" || ext === "tsx", jsx: true });
    case "py": return python();
    case "html": case "htm": return html();
    case "css": case "scss": case "less": return css();
    case "json": return json();
    case "md": return markdown();
    case "rs": return rust();
    case "java": return java();
    case "c": case "cpp": case "h": case "hpp": return cpp();
    case "go": return go();
    case "sql": return sql();
    case "xml": case "svg": return xml();
    default: return [];
  }
}

/** 创建 CodeMirror 6 编辑器 */
export function createEditor(
  parent: HTMLElement,
  initialContent: string = "",
  filename: string = "untitled.txt",
  theme: EditorTheme = "classic",
): EditorView {
  const langExt = getLanguageExtension(filename);
  const extensions: any[] = [
    basicSetup(),
    themeMap[theme],
    ...(theme === "dark" ? [darkHighlightStyle] : []),
    ...(Array.isArray(langExt) ? langExt : [langExt]),
  ];
  return new EditorView({
    state: EditorState.create({ doc: initialContent, extensions }),
    parent,
  });
}

export function destroyEditor(view: any) { view.destroy(); }

export function getEditorContent(view: any): string {
  return view.state.doc.toString();
}

export function setEditorContent(view: any, content: string) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: content } });
}

export function setEditorLanguage(view: any, filename: string, theme: EditorTheme = "classic") {
  const langExt = getLanguageExtension(filename);
  view.dispatch({
    effects: StateEffect.reconfigure.of([
      basicSetup(),
      themeMap[theme],
      ...(theme === "dark" ? [darkHighlightStyle] : []),
      ...(Array.isArray(langExt) ? langExt : [langExt]),
    ]),
  });
}

export function setEditorTheme(view: any, filename: string, theme: EditorTheme) {
  const langExt = getLanguageExtension(filename);
  view.dispatch({
    effects: StateEffect.reconfigure.of([
      basicSetup(),
      themeMap[theme],
      ...(theme === "dark" ? [darkHighlightStyle] : []),
      ...(Array.isArray(langExt) ? langExt : [langExt]),
    ]),
  });
}
