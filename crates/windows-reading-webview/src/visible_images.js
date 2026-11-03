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
      return (url.protocol === "https:" || url.protocol === "http:") && url.href.length <= MAX_URL ? url.href : null;
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
    // A visible message may have eligible images below the viewport; only actual ancestor
    // clipping, not the viewport edge, removes a candidate.
    let left = rect.left, top = rect.top, right = rect.right, bottom = rect.bottom;
    let node = element, depth = 0;
    while (node && depth++ < MAX_ANCESTORS) {
      const style = getComputedStyle(node);
      if (node.hidden || style.display === "none" || style.visibility !== "visible" || Number(style.opacity) === 0 || style.contentVisibility === "hidden") return null;
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
  function add(value, dimensions, kind) {
    const url = absoluteHttp(value);
    if (!url || seen.has(url)) return;
    if (images.length >= MAX_IMAGES) { truncated = true; return; }
    seen.add(url);
    images.push({ url, width: dimensions.width, height: dimensions.height, kind });
  }
  // Computed CSS is serialized by the browser. Walk URL tokens rather than splitting on commas:
  // commas may appear inside quoted URLs, while strings outside url() are not image requests.
  function backgrounds(css, dimensions) {
    if (css.length > MAX_CSS) { truncated = true; return; }
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
      add(value, dimensions, "background");
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
        if (dimensions) {
          if (isImage) add(element.currentSrc || element.src, dimensions, "image");
          backgrounds(getComputedStyle(element).backgroundImage, dimensions);
        }
      }
      element = walker.nextNode();
    }
    if (element) truncated = true;
  }
  return { version: 1, images, scanned, truncated };
})()
