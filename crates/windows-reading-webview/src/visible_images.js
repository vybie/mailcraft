// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
// Fixed host-authored ExecuteScript expression. Read-only classification only: the host must
// revalidate results, enforce permission and message generation, and perform every fetch itself.
(() => {
  "use strict";
  const MAX_ELEMENTS = 10000, MAX_IMAGES = 60, MAX_URL = 8192, MAX_CSS = 65536, MAX_ANCESTORS = 128;
  const images = [], seen = new Set(), collapsed = [], collapsedSeen = new Set();
  let scanned = 0, truncated = false;

  function absoluteHttp(value) {
    if (typeof value !== "string" || value.length === 0 || value.length > MAX_URL) return null;
    try {
      const url = new URL(value, document.baseURI);
      if ((url.protocol !== "https:" && url.protocol !== "http:") || url.hostname === "mailcraft.invalid") return null;
      // Fragments are not sent in HTTP requests, so the host sees the URL without them.
      url.hash = "";
      return url.href.length <= MAX_URL ? url.href : null;
    } catch (_) { return null; }
  }
  function tinyAttribute(element, name) {
    const raw = element.getAttribute(name);
    if (raw === null || !/^\s*\d+(?:\.\d+)?(?:px)?\s*$/i.test(raw)) return false;
    const number = parseFloat(raw);
    return Number.isFinite(number) && number <= 2;
  }
  function emptyClip(node, style) {
    if (style.position === "absolute" || style.position === "fixed") {
      const legacy = /^rect\(\s*(-?(?:\d+(?:\.\d+)?|\.\d+))px\s*,\s*(-?(?:\d+(?:\.\d+)?|\.\d+))px\s*,\s*(-?(?:\d+(?:\.\d+)?|\.\d+))px\s*,\s*(-?(?:\d+(?:\.\d+)?|\.\d+))px\s*\)$/i.exec(style.clip);
      if (legacy && (Number(legacy[2]) <= Number(legacy[4]) || Number(legacy[3]) <= Number(legacy[1]))) return true;
    }
    // Reject only an inset that provably clips the entire box, not ordinary rounded/cropped images.
    const match = /^inset\(([^)]*)\)$/i.exec(style.clipPath);
    if (!match) return false;
    const parts = match[1].split(/\s+round\s+/i)[0].trim().split(/\s+/);
    if (parts.length < 1 || parts.length > 4) return false;
    const values = parts.map(part => /^(-?(?:\d+(?:\.\d+)?|\.\d+))(px|%)$/.exec(part));
    if (values.some(value => !value)) return false;
    const [top, right = top, bottom = top, left = right] = values;
    const rect = node.getBoundingClientRect();
    const pixels = (value, extent) => Number(value[1]) * (value[2] === "%" ? extent / 100 : 1);
    return pixels(top, rect.height) + pixels(bottom, rect.height) >= rect.height
        || pixels(left, rect.width) + pixels(right, rect.width) >= rect.width;
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
      if (node.hidden || style.display === "none" || style.visibility !== "visible" || Number(style.opacity) === 0 || style.contentVisibility === "hidden" || emptyClip(node, style)) return null;
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
  // A blocked, unsized image can collapse to zero before it has intrinsic dimensions.
  // Keep the normal visible-image path unchanged; this only adds collapsed candidates.
  function collapsedImage(element) {
    const rect = element.getBoundingClientRect();
    if (!Number.isFinite(rect.width) || !Number.isFinite(rect.height) || (rect.width !== 0 && rect.height !== 0)
        || (rect.width > 0 && rect.width <= 2) || (rect.height > 0 && rect.height <= 2)
        || rect.right + window.scrollX < 0 || rect.bottom + window.scrollY < 0) return false;
    // Computed Typed OM retains auto instead of turning it into a zero-pixel layout value.
    // Refuse unrecognised expressions: an explicit tiny value must not become an image.
    try {
      const style = element.computedStyleMap();
      for (const prop of ["width", "height", "max-width", "max-height"]) {
        const value = style.get(prop);
        if (!value) return false;
        const text = String(value).trim().toLowerCase();
        if (text === "auto" || text === "none") continue;
        const number = /^(\d+(?:\.\d+)?|\.\d+)(px|%)$/.exec(text);
        if (!number || Number(number[1]) === 0 || (number[2] === "px" && Number(number[1]) <= 2)) return false;
      }
    } catch (_) { return false; }
    let node = element, depth = 0;
    while (node && depth++ < MAX_ANCESTORS) {
      const style = getComputedStyle(node);
      if (node.hidden || style.display === "none" || (node === element && style.display === "contents") || style.visibility !== "visible" || Number(style.opacity) === 0
          || style.contentVisibility === "hidden" || style.clipPath !== "none" || style.clip !== "auto"
          || style.transform !== "none" || style.filter !== "none" || style.maskImage !== "none") return false;
      if (node !== element) {
        const clipX = /^(hidden|clip|scroll|auto)$/.test(style.overflowX);
        const clipY = /^(hidden|clip|scroll|auto)$/.test(style.overflowY);
        if (clipX || clipY) {
          const clip = node.getBoundingClientRect();
          const outsideX = rect.width === 0 ? rect.left < clip.left || rect.left >= clip.right : rect.right <= clip.left || rect.left >= clip.right;
          const outsideY = rect.height === 0 ? rect.top < clip.top || rect.top >= clip.bottom : rect.bottom <= clip.top || rect.top >= clip.bottom;
          if ((clipX && (clip.width <= 2 || outsideX)) || (clipY && (clip.height <= 2 || outsideY))) return false;
        }
      }
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
  function backgrounds(css, sizes, dimensions, width, height) {
    if (css.length > MAX_CSS || sizes.length > MAX_CSS) { truncated = true; return; }
    const imagesByLayer = layers(css), sizesByLayer = layers(sizes);
    if (!sizesByLayer.length) return;
    for (let layer = 0; layer < imagesByLayer.length; layer++) {
      if (tinyOrUnknown(sizesByLayer[layer % sizesByLayer.length], width, height)) continue;
      backgroundLayer(imagesByLayer[layer], dimensions);
    }
  }
  function backgroundLayer(css, dimensions) {
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
          const style = getComputedStyle(element);
          // The background positioning area may be the content box, which is smaller than
          // clientWidth/clientHeight. Use that lower bound for percentage sizes.
          const padding = name => parseFloat(style[name]) || 0;
          const width = Math.max(0, element.clientWidth - padding("paddingLeft") - padding("paddingRight") - 1);
          const height = Math.max(0, element.clientHeight - padding("paddingTop") - padding("paddingBottom") - 1);
          backgrounds(style.backgroundImage, style.backgroundSize, dimensions, width, height);
        } else if (isImage && collapsed.length < MAX_IMAGES && collapsedImage(element)) {
          const url = absoluteHttp(element.currentSrc || element.src);
          if (url && !seen.has(url) && !collapsedSeen.has(url)) { collapsedSeen.add(url); collapsed.push(url); }
        }
      }
      element = walker.nextNode();
    }
    if (element) truncated = true;
    // Images that were already eligible retain priority over newly discovered collapsed ones.
    for (const url of collapsed) add(url, {width: 0, height: 0}, "collapsed-image");
  }
  return { version: 1, images, scanned, truncated };
})()
