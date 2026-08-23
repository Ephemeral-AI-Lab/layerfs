"use strict";

(function layerFSLabBootstrap() {
  const MiB = 1024 ** 2;
  const pages = Object.freeze([
    { slug: "index", href: "index.html", label: "Overview", description: "Authority spine and the 60-second model", keywords: "architecture mental model authority" },
    { slug: "identities", href: "identities.html", label: "Identities", description: "Seven equality domains and forbidden substitutions", keywords: "object id root digest inode commit namespace" },
    { slug: "cas", href: "cas.html", label: "CAS", description: "Canonical bytes, immutable admission, verification", keywords: "content address hash dedup incumbent object" },
    { slug: "cdc", href: "cdc.html", label: "CDC", description: "FastCDC boundaries and local resynchronization", keywords: "chunk rolling hash 8 16 32 kib" },
    { slug: "data-structures", href: "data-structures.html", label: "Data structures", description: "Object graph, namespace, carrier and index", keywords: "tree sqlite segment storage" },
    { slug: "k64-f64", href: "k64-f64.html", label: "K64/F64", description: "Current positional mapping profile", keywords: "current leaf branch cumulative suffix" },
    { slug: "cd32-64", href: "cd32-64.html", label: "CD32–64", description: "G6 content-defined grouping research arm", keywords: "g6 marker rejoin fallback" },
    { slug: "bplus-rope", href: "bplus-rope.html", label: "B+ rope", description: "Projected byte-measured operational rope", keywords: "extent slice split concat logarithmic" },
    { slug: "operations", href: "operations.html", label: "Operations", description: "Read, edit, write, commit and history traces", keywords: "algorithm insert delete append truncate snapshot" },
    { slug: "performance", href: "performance.html", label: "Performance", description: "Big-O, byte work and quantified models", keywords: "complexity benchmark rss mapping" },
    { slug: "vfs-recovery", href: "vfs-recovery.html", label: "VFS & recovery", description: "Projection, exact/latest, durability and crash states", keywords: "vfs materialize mailbox commit recovery" }
  ]);

  const fileSizes = Object.freeze({
    oneMiB: Object.freeze({ bytes: MiB, extents: 53, label: "1 MiB" }),
    tenMiB: Object.freeze({ bytes: 10 * MiB, extents: 531, label: "10 MiB" }),
    hundredMiB: Object.freeze({ bytes: 100 * MiB, extents: 5284, label: "100 MiB" }),
    fiveHundredMiB: Object.freeze({ bytes: 500 * MiB, extents: 26533, label: "500 MiB" })
  });

  const constants = Object.freeze({
    MiB,
    CDC: Object.freeze({ minimum: 8192, target: 16384, maximum: 32768, normalizationShift: 2, seed: 0 }),
    CURRENT: Object.freeze({
      anchorBytes: 100 * MiB,
      anchorExtents: 5284,
      mappingObjects: 86,
      mappingBytes: 196055,
      earlyCountChangeBytes: 196091,
      middleCountChangeBytes: 100479,
      sameCountBytes: 5050,
      nonFileBytes: 284
    }),
    G6: Object.freeze({
      leafMaxBytes: 2332,
      internalMaxBytes: 3101,
      rootMaxBytes: 3121,
      heightOnePathBytes: 5453,
      heightTwoPathBytes: 8554,
      heightOneSplitBytes: 7785,
      heightTwoSplitBytes: 13987
    }),
    BPLUS: Object.freeze({
      nodeBytes: 8192,
      headerBytes: 64,
      entryBytes: 48,
      occupancy: 0.7,
      maxEntries: 169,
      nominalEntries: 118,
      nominalLeafBytes: 5728,
      nominalSplitHeightTwoBytes: 13680,
      conservativeSplitHeightTwoBytes: 18576
    })
  });

  function clamp(value, minimum, maximum) {
    return Math.min(maximum, Math.max(minimum, value));
  }

  function fmtNumber(value, digits = 0) {
    return new Intl.NumberFormat(undefined, { maximumFractionDigits: digits, minimumFractionDigits: digits }).format(value);
  }

  function fmtBytes(value, digits = 2) {
    if (!Number.isFinite(value)) return "—";
    const absolute = Math.abs(value);
    if (absolute < 1024) return `${fmtNumber(value)} B`;
    if (absolute < MiB) return `${fmtNumber(value / 1024, digits)} KiB`;
    if (absolute < 1024 * MiB) return `${fmtNumber(value / MiB, digits)} MiB`;
    return `${fmtNumber(value / (1024 * MiB), digits)} GiB`;
  }

  function fmtDurationNs(value) {
    if (!Number.isFinite(value)) return "—";
    if (value < 1e6) return `${fmtNumber(value / 1e3, 2)} µs`;
    if (value < 1e9) return `${fmtNumber(value / 1e6, 3)} ms`;
    return `${fmtNumber(value / 1e9, 3)} s`;
  }

  function filePopulation(fileBytes) {
    const exact = Object.values(fileSizes).find((entry) => entry.bytes === Number(fileBytes));
    if (exact) return { ...exact, evidence: "Observed" };
    const extents = Math.max(1, Math.round((Number(fileBytes) / MiB) * 52.84));
    return { bytes: Number(fileBytes), extents, label: fmtBytes(Number(fileBytes), 0), evidence: "Projected" };
  }

  function extentsForBytes(fileBytes) {
    return filePopulation(fileBytes).extents;
  }

  function currentMappingEarly(fileBytes) {
    const extents = extentsForBytes(fileBytes);
    return Math.round(constants.CURRENT.earlyCountChangeBytes * extents / constants.CURRENT.anchorExtents);
  }

  function currentMappingMiddle(fileBytes) {
    const extents = extentsForBytes(fileBytes);
    const suffixExtents = Math.ceil(extents / 2);
    return Math.round(constants.CURRENT.middleCountChangeBytes * suffixExtents / 2642);
  }

  function currentMappingModel(fileBytes, position = "middle") {
    if (position === "early") return currentMappingEarly(fileBytes);
    if (position === "middle") return currentMappingMiddle(fileBytes);
    if (position === "same-count") {
      if (Number(fileBytes) === constants.CURRENT.anchorBytes) return constants.CURRENT.sameCountBytes;
      return null;
    }
    const normalized = clamp(Number(position), 0, 1);
    const extents = extentsForBytes(fileBytes);
    const suffixExtents = Math.max(1, Math.ceil(extents * (1 - normalized)));
    const rate = normalized <= 0.5
      ? (constants.CURRENT.earlyCountChangeBytes / 5284) +
        (constants.CURRENT.middleCountChangeBytes / 2642 - constants.CURRENT.earlyCountChangeBytes / 5284) * (normalized * 2)
      : constants.CURRENT.middleCountChangeBytes / 2642;
    return Math.round(suffixExtents * rate);
  }

  function g6MappingPath(fileBytes, split = false) {
    const extents = extentsForBytes(fileBytes);
    const leaves = Math.ceil(extents / 64);
    const height = leaves <= 64 ? 1 : 2;
    if (height === 1) return split ? constants.G6.heightOneSplitBytes : constants.G6.heightOnePathBytes;
    return split ? constants.G6.heightTwoSplitBytes : constants.G6.heightTwoPathBytes;
  }

  function bplusTopology(fileBytes) {
    const extents = extentsForBytes(fileBytes);
    const leaves = Math.ceil(extents / constants.BPLUS.nominalEntries);
    const internal = leaves > constants.BPLUS.maxEntries
      ? Math.ceil(leaves / constants.BPLUS.nominalEntries)
      : 0;
    const rootChildren = internal || leaves;
    const leafBytes = constants.BPLUS.entryBytes * extents + constants.BPLUS.headerBytes * leaves;
    const internalBytes = internal
      ? constants.BPLUS.entryBytes * leaves + constants.BPLUS.headerBytes * internal
      : 0;
    const rootBytes = constants.BPLUS.headerBytes + constants.BPLUS.entryBytes * rootChildren;
    return {
      extents,
      leaves,
      internal,
      rootChildren,
      objects: leaves + internal + 1,
      leafBytes,
      internalBytes,
      rootBytes,
      mappingBytes: leafBytes + internalBytes + rootBytes,
      evidence: "Projected"
    };
  }

  function bplusMappingPath(fileBytes, split = "normal") {
    const topology = bplusTopology(fileBytes);
    const leafPath = topology.extents < constants.BPLUS.nominalEntries
      ? constants.BPLUS.headerBytes + constants.BPLUS.entryBytes * topology.extents
      : constants.BPLUS.nominalLeafBytes;
    const branchPath = topology.internal ? constants.BPLUS.nominalLeafBytes : 0;
    const normal = leafPath + branchPath + topology.rootBytes;
    if (split === "normal" || split === false) return normal;
    if (split === "conservative" && topology.internal) return constants.BPLUS.conservativeSplitHeightTwoBytes;
    if (split === "nominal" && topology.internal) return constants.BPLUS.nominalSplitHeightTwoBytes;
    return (2 * leafPath) + branchPath + topology.rootBytes;
  }

  function ratio(control, candidate) {
    if (![control, candidate].every((value) => Number.isFinite(value)) || candidate === 0) {
      return { factor: null, reductionPercent: null, regressionPercent: null };
    }
    const factor = control / candidate;
    return {
      factor,
      reductionPercent: (1 - candidate / control) * 100,
      regressionPercent: (candidate / control - 1) * 100
    };
  }

  function visibilityModel(fileBytes, editBytes, position = 0.5) {
    const file = Number(fileBytes);
    const edit = Number(editBytes);
    const suffix = file * (1 - clamp(Number(position), 0, 1));
    const nativeShift = 2 * suffix + edit;
    const nativeWithSeed = nativeShift + file;
    const virtual = edit + bplusMappingPath(file, "normal");
    return {
      suffix,
      nativeShift,
      nativeWithSeed,
      virtual,
      shiftRatio: nativeShift / virtual,
      seededRatio: nativeWithSeed / virtual,
      evidence: "Derived / Projected"
    };
  }

  function directoryModel(entries, entryBytes = 64, pathNodes = 3) {
    const count = Math.max(0, Math.floor(Number(entries)));
    const currentBytes = count * entryBytes;
    const targetBytes = pathNodes * constants.BPLUS.nodeBytes;
    return {
      entries: count,
      currentBytes,
      targetBytes,
      factor: currentBytes / targetBytes,
      evidence: "Projected"
    };
  }

  function sqlBatchModel(objects = 5284, capacity = 128) {
    const count = Math.max(0, Math.floor(Number(objects)));
    const cap = Math.max(1, Math.floor(Number(capacity)));
    const batches = Math.ceil(count / cap);
    return {
      objects: count,
      capacity: cap,
      batches,
      factorVsPerObject: count / batches,
      factorVsObservedLeafBatches: 83 / batches,
      evidence: "Derived"
    };
  }

  function historyMappingModel(fileBytes, revisions, design = "bplus", split = false) {
    const count = Math.max(0, Math.floor(Number(revisions)));
    let perRevision;
    if (design === "current-early") perRevision = currentMappingEarly(fileBytes);
    else if (design === "current-middle") perRevision = currentMappingMiddle(fileBytes);
    else if (design === "g6") perRevision = g6MappingPath(fileBytes, split);
    else perRevision = bplusMappingPath(fileBytes, split ? "nominal" : "normal");
    return { perRevision, revisions: count, total: perRevision * count };
  }

  function activateOnKeyboard(element, callback) {
    element.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        callback(event);
      }
    });
  }

  function focusableWithin(container) {
    return [...container.querySelectorAll(
      'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )].filter((element) => !element.hidden && element.getAttribute("aria-hidden") !== "true");
  }

  function trapFocus(container, event) {
    if (event.key !== "Tab") return;
    const focusable = focusableWithin(container);
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function setupActivePage() {
    const active = document.body.dataset.page || "index";
    document.querySelectorAll("a[data-page]").forEach((link) => {
      if (link.dataset.page === active) link.setAttribute("aria-current", "page");
      else link.removeAttribute("aria-current");
    });
  }

  function setupMobileNav() {
    const trigger = document.querySelector("[data-nav-toggle]");
    let panel = document.querySelector("[data-mobile-panel]");
    if (!panel && trigger) {
      panel = trigger.parentElement?.querySelector(":scope > div");
      if (panel) {
        panel.dataset.mobilePanel = "";
        panel.classList.add("mobile-nav-drawer");
        panel.hidden = true;
      }
    }
    if (!trigger || !panel) return;
    const close = () => {
      panel.hidden = true;
      trigger.setAttribute("aria-expanded", "false");
      trigger.setAttribute("aria-label", "Open curriculum");
    };
    const open = () => {
      panel.hidden = false;
      trigger.setAttribute("aria-expanded", "true");
      trigger.setAttribute("aria-label", "Close curriculum");
      panel.querySelector("a")?.focus();
    };
    trigger.addEventListener("click", () => panel.hidden ? open() : close());
    panel.addEventListener("click", (event) => {
      if (event.target.closest("a")) close();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !panel.hidden) {
        close();
        trigger.focus();
      }
    });
  }

  function setupEvidenceDetails() {
    document.querySelectorAll("[data-evidence-detail]").forEach((badge, index) => {
      const detailText = badge.dataset.evidenceDetail;
      if (!detailText) return;
      if (badge.tagName !== "BUTTON") {
        badge.setAttribute("role", "button");
        badge.tabIndex = 0;
      }
      const detail = document.createElement("span");
      detail.className = "evidence-detail";
      detail.id = `evidence-detail-${index + 1}`;
      detail.hidden = true;
      detail.textContent = detailText;
      badge.setAttribute("aria-controls", detail.id);
      badge.setAttribute("aria-expanded", "false");
      badge.title = detailText;
      badge.insertAdjacentElement("afterend", detail);
      const toggle = () => {
        const opening = detail.hidden;
        detail.hidden = !opening;
        badge.setAttribute("aria-expanded", String(opening));
      };
      badge.addEventListener("click", toggle);
      if (badge.tagName !== "BUTTON") activateOnKeyboard(badge, toggle);
    });
  }

  function fallbackCopy(text) {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.className = "visually-hidden";
    document.body.append(area);
    area.select();
    const copied = document.execCommand("copy");
    area.remove();
    return copied;
  }

  function setupCopyCode() {
    document.querySelectorAll("pre[data-copy]").forEach((pre) => {
      const parent = pre.parentElement;
      if (!parent || parent.querySelector(":scope > .copy-code")) return;
      const button = document.createElement("button");
      button.type = "button";
      button.className = "copy-code";
      button.textContent = pre.dataset.copyLabel || "Copy";
      button.setAttribute("aria-label", "Copy code to clipboard");
      parent.insertBefore(button, pre);
      button.addEventListener("click", async () => {
        const text = pre.innerText;
        let copied = false;
        try {
          if (navigator.clipboard?.writeText) {
            await navigator.clipboard.writeText(text);
            copied = true;
          } else {
            copied = fallbackCopy(text);
          }
        } catch {
          copied = fallbackCopy(text);
        }
        button.textContent = copied ? "Copied" : "Select text";
        button.classList.toggle("is-success", copied);
        window.setTimeout(() => {
          button.textContent = pre.dataset.copyLabel || "Copy";
          button.classList.remove("is-success");
        }, 1600);
      });
    });
  }

  function setupCommandPalette() {
    const dialog = document.querySelector("#command-dialog");
    if (!(dialog instanceof HTMLDialogElement)) return;
    const input = dialog.querySelector("[data-command-input]");
    const results = dialog.querySelector("[data-command-results]");
    const closeButton = dialog.querySelector("[data-command-close]");
    if (!input || !results) return;
    let selected = 0;
    let filtered = [...pages];
    let returnFocus = null;

    const render = () => {
      results.replaceChildren();
      if (!filtered.length) {
        const empty = document.createElement("li");
        empty.className = "command-empty";
        empty.textContent = "No lesson matches that query.";
        results.append(empty);
        return;
      }
      selected = clamp(selected, 0, filtered.length - 1);
      filtered.forEach((page, index) => {
        const item = document.createElement("li");
        const link = document.createElement("a");
        const label = document.createElement("span");
        const hint = document.createElement("small");
        link.className = "command-result";
        link.href = page.href;
        link.dataset.commandIndex = String(index);
        link.setAttribute("aria-selected", String(index === selected));
        label.textContent = page.label;
        hint.textContent = page.description;
        link.append(label, hint);
        item.append(link);
        results.append(item);
      });
    };

    const filter = () => {
      const query = input.value.trim().toLowerCase();
      filtered = pages.filter((page) => `${page.label} ${page.description} ${page.keywords}`.toLowerCase().includes(query));
      selected = 0;
      render();
    };

    const open = (trigger = document.activeElement) => {
      returnFocus = trigger instanceof HTMLElement ? trigger : null;
      input.value = "";
      filtered = [...pages];
      selected = Math.max(0, pages.findIndex((page) => page.slug === document.body.dataset.page));
      render();
      dialog.showModal();
      window.requestAnimationFrame(() => input.focus());
    };

    const close = () => dialog.close();

    document.querySelectorAll("[data-command-open]").forEach((trigger) => {
      trigger.addEventListener("click", () => open(trigger));
    });
    closeButton?.addEventListener("click", close);
    input.addEventListener("input", filter);
    input.addEventListener("keydown", (event) => {
      if (!filtered.length) return;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        selected = (selected + 1) % filtered.length;
        render();
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        selected = (selected - 1 + filtered.length) % filtered.length;
        render();
      } else if (event.key === "Enter") {
        event.preventDefault();
        window.location.href = filtered[selected].href;
      }
    });
    results.addEventListener("pointermove", (event) => {
      const link = event.target.closest("[data-command-index]");
      if (!link) return;
      selected = Number(link.dataset.commandIndex);
      results.querySelectorAll("[aria-selected]").forEach((item, index) => {
        item.setAttribute("aria-selected", String(index === selected));
      });
    });
    dialog.addEventListener("click", (event) => {
      if (event.target === dialog) close();
    });
    dialog.addEventListener("keydown", (event) => trapFocus(dialog, event));
    dialog.addEventListener("close", () => returnFocus?.focus());
    document.addEventListener("keydown", (event) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (dialog.open) close();
        else open();
      }
    });
  }

  function setupStepper(container, onChange) {
    const buttons = [...container.querySelectorAll("[data-step]")];
    if (!buttons.length) return { setStep() {} };
    let index = Math.max(0, buttons.findIndex((button) => button.getAttribute("aria-current") === "step"));
    const setStep = (next) => {
      index = clamp(Number(next), 0, buttons.length - 1);
      buttons.forEach((button, buttonIndex) => {
        if (buttonIndex === index) button.setAttribute("aria-current", "step");
        else button.removeAttribute("aria-current");
      });
      onChange(index, buttons[index].dataset.step, buttons[index]);
    };
    buttons.forEach((button, buttonIndex) => button.addEventListener("click", () => setStep(buttonIndex)));
    container.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      const delta = event.key === "ArrowRight" ? 1 : -1;
      setStep((index + delta + buttons.length) % buttons.length);
      buttons[index].focus();
    });
    setStep(index);
    return { setStep, get index() { return index; } };
  }

  function selfCheck() {
    const oneHundred = fileSizes.hundredMiB.bytes;
    const topology = bplusTopology(oneHundred);
    const visibility = visibilityModel(oneHundred, MiB, 0.5);
    const checks = [
      extentsForBytes(oneHundred) === 5284,
      currentMappingEarly(oneHundred) === 196091,
      currentMappingMiddle(oneHundred) === 100479,
      g6MappingPath(oneHundred, false) === 8554,
      topology.objects === 46,
      topology.mappingBytes === 258736,
      bplusMappingPath(oneHundred, "normal") === 7952,
      Math.round(visibility.nativeShift) === 105906176,
      Math.round(visibility.virtual) === 1056528,
      sqlBatchModel(5284, 128).batches === 42
    ];
    console.assert(checks.every(Boolean), "LayerFSLab calculator self-check failed", checks);
    return checks.every(Boolean);
  }

  window.LayerFSLab = Object.freeze({
    pages,
    fileSizes,
    constants,
    clamp,
    fmtNumber,
    fmtBytes,
    fmtDurationNs,
    filePopulation,
    extentsForBytes,
    currentMappingEarly,
    currentMappingMiddle,
    currentMappingModel,
    g6MappingPath,
    bplusTopology,
    bplusMappingPath,
    ratio,
    visibilityModel,
    directoryModel,
    sqlBatchModel,
    historyMappingModel,
    activateOnKeyboard,
    focusableWithin,
    trapFocus,
    setupStepper,
    selfCheck
  });

  function init() {
    setupActivePage();
    setupMobileNav();
    setupEvidenceDetails();
    setupCopyCode();
    setupCommandPalette();
    selfCheck();
    document.dispatchEvent(new CustomEvent("layerfs:ready", { detail: window.LayerFSLab }));
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init, { once: true });
  else init();
})();
