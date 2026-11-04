// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Fixed host-authored ExecuteScript expression. Read-only classification only: the host must
// revalidate results, enforce permission and message generation, and perform every fetch itself.
(() => {
  "use strict";
  const MAX_ELEMENTS = 10000, MAX_IMAGES = 60, MAX_URL = 8192, MAX_CSS = 65536, MAX_ANCESTORS = 128;
  const images = [], seen = new Set();
  let scanned = 0, truncated = false;

  function absoluteHttp(value) {
    if (typeof value !== "string" || value.length === 0 || value.length > MAX_URL) return null;
    try {
      const url = new URL(value, document.baseURI);
      return (url.protocol === "https:" || url.protocol === "http:") && url.hostname !== "mailcraft.invalid" && url.href.length <= MAX_URL ? url.href : null;
    } catch (_) { return null; }
  }
  function tinyAttribute(element, name) {
    const raw = element.getAttribute(name);
    if (raw === null || !/^\s*\d+(?:\.\d+)?(?:px)?\s*$/i.test(raw)) return false;
    const number = parseFloat(raw);
    return Number.isFinite(number) && number <= 2;
  }
  function visible(element) {
    const rect = element.getBoundingClientRect();
    if (!Number.isFinite(rect.width) || !Number.isFinite(rect.height) || rect.width <= 2 || rect.height <= 2) return null;
    if (rect.right + window.scrollX <= 0 || rect.bottom + window.scrollY <= 0) return null;
    // A visible message may have eligible images below the viewport; only actual ancestor
    // clipping, not the viewport edge, removes a candidate.
    let left = rect.left, top = rect.top, right = rect.right, bottom = rect.bottom;
    let node = element, depth = 0;
    while (node && depth++ < MAX_ANCESTORS) {
      const style = getComputedStyle(node);
      if (node.hidden || style.display === "none" || style.visibility !== "visible" || Number(style.opacity) === 0 || style.contentVisibility === "hidden" || style.clipPath !== "none" || style.clip !== "auto") return null;
      if (node === element) {
        const w = parseFloat(style.width), h = parseFloat(style.height);
        if ((Number.isFinite(w) && w <= 2) || (Number.isFinite(h) && h <= 2)) return null;
      } else {
        const clipX = /^(hidden|clip|scroll|auto)$/.test(style.overflowX);
        const clipY = /^(hidden|clip|scroll|auto)$/.test(style.overflowY);
        if (clipX || clipY) {
          const clip = node.getBoundingClientRect();
          if (clipX) { left = Math.max(left, clip.left); right = Math.min(right, clip.right); }
          if (clipY) { top = Math.max(top, clip.top); bottom = Math.min(bottom, clip.bottom); }
        }
      }
      node = node.parentElement;
    }
    if (node || right - left <= 0 || bottom - top <= 0) return null;
    return { width: rect.width, height: rect.height };
  }
  // A blocked image with no intrinsic dimensions can collapse to 0×0. Treat it as uncertain,
  // never as an automatically eligible image. Deliberate consent is required by the host.
  let checkedRules = 0;
  function unknownImage(element) {
    const rect = element.getBoundingClientRect();
    if (!Number.isFinite(rect.width) || !Number.isFinite(rect.height) || (rect.width > 0 && rect.width <= 2) || (rect.height > 0 && rect.height <= 2)) return false;
    if (rect.right + window.scrollX < 0 || rect.bottom + window.scrollY < 0) return false;
    if (tinyAttribute(element, "width") || tinyAttribute(element, "height")) return false;
    const tiny = value => {
      value = (value || "").trim().toLowerCase();
      if (!value || /^(auto|none|initial|unset)$/.test(value)) return false;
      const number = /^([+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?)(px|%)?$/i.exec(value);
      return !number || Number(number[1]) <= 2;
    };
    const properties = ["width", "height", "min-width", "min-height", "max-width", "max-height"];
    if (properties.some(name => tiny(element.style.getPropertyValue(name)))) return false;
    function safeRules(rules) {
      for (const rule of rules) {
        if (++checkedRules > 2000) return false;
        if (rule.cssRules && !safeRules(rule.cssRules)) return false;
        if (rule.selectorText && rule.style) {
          try { if (element.matches(rule.selectorText) && properties.some(name => tiny(rule.style.getPropertyValue(name)))) return false; }
          catch (_) { return false; }
        }
      }
      return true;
    }
    for (const sheet of document.styleSheets) { try { if (!safeRules(sheet.cssRules)) return false; } catch (_) { return false; } }
    let node = element, depth = 0;
    while (node && depth++ < MAX_ANCESTORS) {
      const style = getComputedStyle(node);
      if (node.hidden || style.display === "none" || (node === element && style.display === "contents") || style.visibility !== "visible" || Number(style.opacity) === 0 || style.contentVisibility === "hidden" || style.clipPath !== "none" || style.clip !== "auto" || style.transform !== "none" || style.filter !== "none" || style.maskImage !== "none") return false;
      if (node !== element && (/^(hidden|clip|scroll|auto)$/.test(style.overflowX) || /^(hidden|clip|scroll|auto)$/.test(style.overflowY))) {
        const clip = node.getBoundingClientRect();
        const outsideX = rect.width === 0 ? rect.left < clip.left || rect.left >= clip.right : rect.right <= clip.left || rect.left >= clip.right;
        const outsideY = rect.height === 0 ? rect.top < clip.top || rect.top >= clip.bottom : rect.bottom <= clip.top || rect.top >= clip.bottom;
        if ((/^(hidden|clip|scroll|auto)$/.test(style.overflowX) && (clip.width <= 2 || outsideX)) || (/^(hidden|clip|scroll|auto)$/.test(style.overflowY) && (clip.height <= 2 || outsideY))) return false;
      }
      node = node.parentElement;
    }
    return !node;
  }
  function needsUnknownSize(element) {
    if (element.naturalWidth > 0 && element.naturalHeight > 0) return false;
    // Unlike getComputedStyle's resolved pixels, Typed OM preserves `auto` for broken-image
    // placeholders, which can otherwise look like a harmless 16×16 image before it loads.
    try {
      const style = element.computedStyleMap();
      return String(style.get("width")) === "auto" || String(style.get("height")) === "auto";
    } catch (_) { return true; }
  }
  function safeUnknownBackground(element) {
    let node = element, depth = 0;
    while (node && depth++ < MAX_ANCESTORS) {
      const style = getComputedStyle(node);
      if (style.filter !== "none" || style.maskImage !== "none" || style.transform !== "none") return false;
      node = node.parentElement;
    }
    return !node;
  }
  function add(value, dimensions, kind) {
    const url = absoluteHttp(value);
    if (!url || seen.has(url)) return;
    if (images.length >= MAX_IMAGES) { truncated = true; return; }
    seen.add(url);
    images.push({ url, width: dimensions.width, height: dimensions.height, kind });
  }
  // Split CSS layers only at top-level commas, never inside a URL or function.
  function layers(css) {
    const result = []; let start = 0, depth = 0, quote = null;
    for (let i = 0; i < css.length; i++) {
      const c = css[i];
      if (c === "\\") { i++; continue; }
      if (quote) { if (c === quote) quote = null; continue; }
      if (c === '"' || c === "'") { quote = c; continue; }
      if (c === "(") depth++;
      else if (c === ")") { if (depth === 0) return []; depth--; }
      else if (c === "," && depth === 0) { result.push(css.slice(start, i).trim()); start = i + 1; }
    }
    if (quote || depth !== 0) return [];
    result.push(css.slice(start).trim());
    return result;
  }
  // Computed lengths are normally pixels; percentages use the background positioning area.
  // Unknown expressions are skipped rather than treating a possibly tiny layer as eligible.
  function tinyOrUnknown(size, width, height) {
    const parts = size.trim().toLowerCase().split(/\s+/);
    if (parts.length === 1 && (parts[0] === "cover" || parts[0] === "contain")) return false;
    if (parts.length < 1 || parts.length > 2) return true;
    return parts.some((part, index) => {
      if (part === "auto") return false;
      const match = /^(\d+(?:\.\d+)?|\.\d+)(px|%)?$/.exec(part);
      if (!match) return true;
      const value = Number(match[1]);
      if (!match[2] && value !== 0) return true;
      const pixels = match[2] === "%" ? value * (index === 0 ? width : height) / 100 : value;
      return !Number.isFinite(pixels) || pixels <= 2;
    });
  }
  // Computed CSS is serialized by the browser. Strings outside url() are not image requests.
  function backgrounds(css, sizes, dimensions, width, height, allowUnknown) {
    if (css.length > MAX_CSS || sizes.length > MAX_CSS) { truncated = true; return; }
    const imagesByLayer = layers(css), sizesByLayer = layers(sizes);
    if (!sizesByLayer.length) return;
    for (let layer = 0; layer < imagesByLayer.length; layer++) {
      const size = sizesByLayer[layer % sizesByLayer.length];
      if (tinyOrUnknown(size, width, height)) continue;
      const unknown = /\b(auto|contain)\b/i.test(size);
      if (unknown && !allowUnknown) continue;
      backgroundLayer(imagesByLayer[layer], unknown ? {width: 0, height: 0} : dimensions, unknown ? "unknown" : "background");
    }
  }
  function backgroundLayer(css, dimensions, kind) {
    let i = 0;
    while (i < css.length && images.length < MAX_IMAGES) {
      const c = css[i];
      if (c === '"' || c === "'") {
        const q = c; i++;
        while (i < css.length) { if (css[i] === "\\") i += 2; else if (css[i++] === q) break; }
        continue;
      }
      if (css.slice(i, i + 3).toLowerCase() !== "url" || (i && /[\w-]/.test(css[i - 1]))) { i++; continue; }
      let j = i + 3;
      while (/\s/.test(css[j] || "") && j < css.length) j++;
      if (css[j++] !== "(") { i++; continue; }
      while (j < css.length && /\s/.test(css[j])) j++;
      let quote = null;
      if (css[j] === '"' || css[j] === "'") quote = css[j++];
      const start = j;
      while (j < css.length && css[j] !== "\\" && (quote ? css[j] !== quote : css[j] !== ")" && !/\s/.test(css[j]))) j++;
      if (css[j] === "\\") { i = j + 1; continue; } // Ambiguous escape: fail closed.
      const value = css.slice(start, j);
      if (quote && css[j++] !== quote) { i = j; continue; }
      while (j < css.length && /\s/.test(css[j])) j++;
      if (css[j] !== ")") { i = j; continue; }
      add(value, dimensions, kind);
      i = j + 1;
    }
  }
  const root = document.documentElement;
  if (root) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
    let element = root;
    while (element && scanned < MAX_ELEMENTS && images.length < MAX_IMAGES) {
      scanned++;
      const isImage = element.localName === "img";
      if (!tinyAttribute(element, "width") && !tinyAttribute(element, "height")) {
        const dimensions = visible(element);
        if (isImage && needsUnknownSize(element)) {
          if (unknownImage(element)) add(element.currentSrc || element.src, {width: 0, height: 0}, "unknown");
        } else if (isImage && dimensions) add(element.currentSrc || element.src, dimensions, "image");
        if (dimensions) {
          const style = getComputedStyle(element);
          // The background positioning area may be the content box, which is smaller than
          // clientWidth/clientHeight. Use that lower bound for percentage sizes.
          const padding = name => parseFloat(style[name]) || 0;
          const width = Math.max(0, element.clientWidth - padding("paddingLeft") - padding("paddingRight") - 1);
          const height = Math.max(0, element.clientHeight - padding("paddingTop") - padding("paddingBottom") - 1);
          backgrounds(style.backgroundImage, style.backgroundSize, dimensions, width, height, safeUnknownBackground(element));
        }
      }
      element = walker.nextNode();
    }
    if (element) truncated = true;
  }
  return { version: 1, images, scanned, truncated };
})()
