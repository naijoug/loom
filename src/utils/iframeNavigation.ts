export const PLAN_PREVIEW_SANDBOX = "allow-same-origin";

function decodeHash(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function findLocalHashAnchor(target: EventTarget | null) {
  if (!(target instanceof Element)) {
    return null;
  }

  const anchor = target.closest("a[href]");
  if (!(anchor instanceof HTMLAnchorElement)) {
    return null;
  }

  const href = anchor.getAttribute("href")?.trim();
  if (!href || href === "#" || !href.startsWith("#")) {
    return null;
  }

  return anchor;
}

export function wireIframeHashNavigation(iframe: HTMLIFrameElement) {
  const document = iframe.contentDocument;
  if (!document?.body || document.body.dataset.loomHashNavigation === "wired") {
    iframe.dataset.loomHashNavigation = document?.body ? "wired" : "unavailable";
    return;
  }

  document.body.dataset.loomHashNavigation = "wired";
  iframe.dataset.loomHashNavigation = "wired";
  document.addEventListener("click", (event) => {
    const anchor = findLocalHashAnchor(event.target);
    if (!anchor) {
      return;
    }

    const rawHash = anchor.getAttribute("href")?.trim().slice(1);
    if (!rawHash) {
      return;
    }

    const target = document.getElementById(decodeHash(rawHash));
    if (!target) {
      return;
    }

    event.preventDefault();
    target.scrollIntoView({ block: "start" });
  });
}
