function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function normalizeImageTarget(raw: string): string {
  return raw
    .trim()
    .replace(/^<|>$/g, "")
    .split("#")[0]
    .split("?")[0]
    .replace(/\\/g, "/")
    .toLowerCase();
}

function imageBaseName(raw: string): string {
  const normalized = normalizeImageTarget(raw);
  const parts = normalized.split("/");
  return parts[parts.length - 1] ?? normalized;
}

function parseReferenceDefinitions(markdown: string): {
  body: string;
  refs: Map<string, string>;
} {
  const refs = new Map<string, string>();
  const kept: string[] = [];
  const lines = markdown.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  for (const line of lines) {
    const match = line.match(/^\s*\[([^\]]+)\]:\s*(\S+)(?:\s+["'(].*["')])?\s*$/);
    if (!match) {
      kept.push(line);
      continue;
    }
    const key = match[1].trim().toLowerCase();
    const target = String(match[2] ?? "").trim().replace(/^<|>$/g, "");
    if (key && target) {
      refs.set(key, target);
    }
  }
  return { body: kept.join("\n"), refs };
}

function renderInlineMarkdown(
  raw: string,
  refs: Map<string, string>,
  resolveImageTarget: (target: string) => string | null,
  resolveLinkTarget: (target: string) => string | null
): string {
  let html = escapeHtml(raw);
  html = html.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (_match, alt, target) => {
    const resolved = resolveImageTarget(String(target ?? ""));
    if (!resolved) {
      return `<span class="reader-md-missing-image">[image: ${escapeHtml(String(alt ?? "").trim() || "missing")}]</span>`;
    }
    return `<img src="${escapeHtml(resolved)}" alt="${escapeHtml(String(alt ?? "").trim())}" loading="lazy" />`;
  });
  html = html.replace(/!\[([^\]]*)\]\[([^\]]*)\]/g, (_match, alt, ref) => {
    const key = String(ref ?? "").trim().toLowerCase() || String(alt ?? "").trim().toLowerCase();
    const target = refs.get(key);
    const resolved = target ? resolveImageTarget(target) : null;
    if (!resolved) {
      return `<span class="reader-md-missing-image">[image: ${escapeHtml(String(alt ?? "").trim() || "missing")}]</span>`;
    }
    return `<img src="${escapeHtml(resolved)}" alt="${escapeHtml(String(alt ?? "").trim())}" loading="lazy" />`;
  });
  html = html.replace(/`([^`]+)`/g, "<code>$1</code>");
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html.replace(/\*([^*]+)\*/g, "<em>$1</em>");
  html = html.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_match, text, target) => {
    const resolved = resolveLinkTarget(String(target ?? ""));
    if (!resolved) {
      return escapeHtml(String(text ?? ""));
    }
    return `<a href="${escapeHtml(resolved)}" target="_blank" rel="noreferrer">${escapeHtml(
      String(text ?? "")
    )}</a>`;
  });
  html = html.replace(/\[([^\]]+)\]\[([^\]]*)\]/g, (_match, text, ref) => {
    const rawText = String(text ?? "");
    const key = String(ref ?? "").trim().toLowerCase() || rawText.trim().toLowerCase();
    const target = refs.get(key);
    const resolved = target ? resolveLinkTarget(target) : null;
    if (!resolved) {
      return escapeHtml(rawText);
    }
    return `<a href="${escapeHtml(resolved)}" target="_blank" rel="noreferrer">${escapeHtml(
      rawText
    )}</a>`;
  });
  return html;
}

export function renderMarkdownToHtml(
  markdown: string,
  imageCandidates: Array<{ rawPath: string; src: string }>
): string {
  const { body, refs } = parseReferenceDefinitions(markdown);
  const lines = body.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
  const out: string[] = [];
  let listBuffer: string[] = [];
  let anchorIndex = 0;
  const unusedImages = [...imageCandidates];

  const resolveImageTarget = (target: string): string | null => {
    const normalizedTarget = normalizeImageTarget(target);
    if (!normalizedTarget) {
      return null;
    }
    if (
      normalizedTarget.startsWith("http://") ||
      normalizedTarget.startsWith("https://") ||
      normalizedTarget.startsWith("data:") ||
      normalizedTarget.startsWith("asset:")
    ) {
      return target;
    }
    const targetBaseName = imageBaseName(normalizedTarget);
    const matched = unusedImages.find((candidate) => {
      const candidateNormalized = normalizeImageTarget(candidate.rawPath);
      return (
        candidateNormalized === normalizedTarget ||
        candidateNormalized.endsWith(`/${normalizedTarget}`) ||
        imageBaseName(candidateNormalized) === targetBaseName
      );
    });
    if (matched) {
      const idx = unusedImages.indexOf(matched);
      if (idx >= 0) {
        unusedImages.splice(idx, 1);
      }
      return matched.src;
    }
    if (unusedImages.length > 0) {
      const fallback = unusedImages.shift();
      return fallback?.src ?? null;
    }
    return null;
  };

  const resolveLinkTarget = (target: string): string | null => {
    const raw = String(target ?? "").trim().replace(/^<|>$/g, "");
    if (!raw) {
      return null;
    }
    if (
      raw.startsWith("http://") ||
      raw.startsWith("https://") ||
      raw.startsWith("data:") ||
      raw.startsWith("asset:") ||
      raw.startsWith("#")
    ) {
      return raw;
    }
    return resolveImageTarget(raw) ?? raw;
  };

  const nextAnchor = (): string => {
    const current = anchorIndex;
    anchorIndex += 1;
    return ` data-ll-md-anchor="${current}"`;
  };

  const flushList = (): void => {
    if (listBuffer.length === 0) {
      return;
    }
    out.push(`<ul>${listBuffer.join("")}</ul>`);
    listBuffer = [];
  };

  for (const rawLine of lines) {
    const line = rawLine.trimEnd();
    const trimmed = line.trim();
    if (!trimmed) {
      flushList();
      continue;
    }
    if (trimmed.startsWith("# ")) {
      flushList();
      out.push(
        `<h1${nextAnchor()}>${renderInlineMarkdown(
          trimmed.slice(2).trim(),
          refs,
          resolveImageTarget,
          resolveLinkTarget
        )}</h1>`
      );
      continue;
    }
    if (trimmed.startsWith("## ")) {
      flushList();
      out.push(
        `<h2${nextAnchor()}>${renderInlineMarkdown(
          trimmed.slice(3).trim(),
          refs,
          resolveImageTarget,
          resolveLinkTarget
        )}</h2>`
      );
      continue;
    }
    if (trimmed.startsWith("### ")) {
      flushList();
      out.push(
        `<h3${nextAnchor()}>${renderInlineMarkdown(
          trimmed.slice(4).trim(),
          refs,
          resolveImageTarget,
          resolveLinkTarget
        )}</h3>`
      );
      continue;
    }
    if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
      listBuffer.push(
        `<li${nextAnchor()}>${renderInlineMarkdown(
          trimmed.slice(2).trim(),
          refs,
          resolveImageTarget,
          resolveLinkTarget
        )}</li>`
      );
      continue;
    }
    flushList();
    out.push(
      `<p${nextAnchor()}>${renderInlineMarkdown(
        trimmed,
        refs,
        resolveImageTarget,
        resolveLinkTarget
      )}</p>`
    );
  }

  flushList();
  return out.join("");
}
