/**
 * 轻量 Markdown → HTML 渲染器（用于 .md 文件在线预览 / CSV 表格预览）
 * 支持：标题、段落、粗体/斜体/删除线、行内代码、代码块、链接、图片、
 *       无序/有序列表、引用、GFM 表格、分隔线、任务列表。
 * 原始 HTML 一律转义，避免 XSS。
 */

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** 行内渲染：`code`、**bold**、*italic*、~~del~~、[link](url)、![img](url) */
function renderInline(text: string): string {
  let s = escapeHtml(text);
  // 图片 ![alt](src)
  s = s.replace(/!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g, (_m, alt, src) => {
    return `<img src="${src}" alt="${alt}" onerror="this.style.display='none'"/>`;
  });
  // 链接 [text](url)
  s = s.replace(/\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g, (_m, t, url) => {
    const safeUrl = /^(https?:|mailto:|#|\/)/i.test(url) ? url : "#";
    return `<a href="${safeUrl}" target="_blank" rel="noopener">${t}</a>`;
  });
  // 行内代码
  s = s.replace(/`([^`]+)`/g, "<code>$1</code>");
  // 粗体
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  // 斜体
  s = s.replace(/(^|[^*])\*([^*]+)\*/g, "$1<em>$2</em>");
  // 删除线
  s = s.replace(/~~([^~]+)~~/g, "<del>$1</del>");
  return s;
}

function renderTableRow(cells: string[], tag: "th" | "td"): string {
  return `<tr>${cells.map((c) => `<${tag}>${renderInline(c.trim())}</${tag}>`).join("")}</tr>`;
}

/** 按行渲染一个 Markdown 源 */
export function renderMarkdown(md: string): string {
  const lines = md.replace(/\r\n?/g, "\n").split("\n");
  const out: string[] = [];
  let i = 0;

  const isTableSeparator = (line: string) => /^\s*\|?[\s:|-]+\|?\s*$/.test(line) && line.includes("-");

  while (i < lines.length) {
    const line = lines[i];

    // 代码围栏
    if (/^\s*```/.test(line)) {
      const lang = line.replace(/^\s*```\s*/, "").trim();
      const buf: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```/.test(lines[i])) { buf.push(lines[i]); i++; }
      i++; // 跳过结束围栏
      out.push(`<pre><code class="lang-${escapeHtml(lang || "text")}">${escapeHtml(buf.join("\n"))}</code></pre>`);
      continue;
    }

    // 标题
    const h = /^(#{1,6})\s+(.*)$/.exec(line);
    if (h) {
      const level = h[1].length;
      out.push(`<h${level}>${renderInline(h[2])}</h${level}>`);
      i++; continue;
    }

    // 分隔线
    if (/^\s*([-*_])\s*(\1\s*){2,}$/.test(line)) {
      out.push("<hr/>"); i++; continue;
    }

    // 表格（当前行含 | 且下一行是分隔行）
    if (line.includes("|") && i + 1 < lines.length && isTableSeparator(lines[i + 1])) {
      const headerCells = line.split("|").map((c) => c.trim());
      // 首尾可能因 "|a|b|" 格式产生空串
      if (headerCells.length && headerCells[0] === "") headerCells.shift();
      if (headerCells.length && headerCells[headerCells.length - 1] === "") headerCells.pop();
      const rows: string[] = [];
      i += 2; // 跳过表头和分隔行
      while (i < lines.length && lines[i].includes("|") && lines[i].trim() !== "") {
        let cells = lines[i].split("|").map((c) => c.trim());
        if (cells[0] === "") cells.shift();
        if (cells.length && cells[cells.length - 1] === "") cells.pop();
        rows.push(renderTableRow(cells, "td"));
        i++;
      }
      out.push(`<table><thead>${renderTableRow(headerCells, "th")}</thead><tbody>${rows.join("")}</tbody></table>`);
      continue;
    }

    // 引用块（收集连续 > 行）
    if (/^\s*>\s?/.test(line)) {
      const buf: string[] = [];
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        buf.push(lines[i].replace(/^\s*>\s?/, "")); i++;
      }
      out.push(`<blockquote>${renderMarkdown(buf.join("\n"))}</blockquote>`);
      continue;
    }

    // 任务列表/列表
    if (/^\s*[-*+]\s+/.test(line) || /^\s*\d+[.)]\s+/.test(line)) {
      const items: string[] = [];
      let ordered = false;
      if (/^\s*\d+[.)]\s+/.test(line)) ordered = true;
      while (i < lines.length) {
        const m = /^\s*([-*+]|\d+[.)])\s+(.*)$/.exec(lines[i]);
        if (!m) break;
        const text = m[2];
        const task = /^\[([ xX])\]\s+(.*)$/.exec(text);
        if (task) {
          const checked = task[1] === "x" || task[1] === "X";
          items.push(`<li class="task-item"><input type="checkbox" disabled ${checked ? "checked" : ""}/>${renderInline(task[2])}</li>`);
        } else {
          items.push(`<li>${renderInline(text)}</li>`);
        }
        i++;
      }
      out.push(ordered ? `<ol>${items.join("")}</ol>` : `<ul>${items.join("")}</ul>`);
      continue;
    }

    // 空行
    if (line.trim() === "") { i++; continue; }

    // 普通段落（合并连续行）
    const buf: string[] = [line];
    i++;
    while (
      i < lines.length &&
      lines[i].trim() !== "" &&
      !/^(#{1,6})\s+/.test(lines[i]) &&
      !/^\s*```/.test(lines[i]) &&
      !/^\s*>\s?/.test(lines[i]) &&
      !/^\s*([-*+]|\d+[.)])\s+/.test(lines[i]) &&
      !/^\s*([-*_])\s*(\1\s*){2,}$/.test(lines[i])
    ) {
      buf.push(lines[i]); i++;
    }
    out.push(`<p>${renderInline(buf.join(" "))}</p>`);
  }

  return out.join("\n");
}

/** 供 v-html 使用：渲染并包装 */
export function markdownToHtml(md: string): string {
  return renderMarkdown(md || "");
}
