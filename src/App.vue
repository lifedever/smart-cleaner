<script setup lang="ts">
import { ref, computed, nextTick, onMounted, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, message, ask } from "@tauri-apps/plugin-dialog";
import { load } from "@tauri-apps/plugin-store";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getVersion } from "@tauri-apps/api/app";
import "./assets/styles.css";

// Store
let store: any = null;
const whitelist = ref<string[]>([]);
const showWhitelistModal = ref(false);
const showAboutModal = ref(false);
const activeTab = ref("about");
const appVersion = ref("");

// Custom Modal State
const confirmModal = ref({
  show: false,
  title: "",
  message: "",
  onConfirm: () => {},
});

const showConfirm = (title: string, message: string) => {
  return new Promise<boolean>((resolve) => {
    confirmModal.value = {
      show: true,
      title,
      message,
      onConfirm: () => {
        confirmModal.value.show = false;
        resolve(true);
      },
    };
  });
};

const hasUpdate = ref(false);
const updateInfo = ref<any>(null);
const updateLoading = ref(false);
const updateLoadingText = ref("");

const checkUpdate = async (silent = false) => {
  try {
    updateLoading.value = true;
    updateLoadingText.value = "正在检查更新...";
    const update = await check();
    updateLoading.value = false;
    if (update) {
      hasUpdate.value = true;
      updateInfo.value = update;
      if (!silent) {
        const yes = await ask(
          `发现新版本 ${update.version}，是否更新？\n${update.body || ""}`,
          { title: "发现新版本", kind: "info" },
        );
        if (yes) {
          updateLoading.value = true;
          updateLoadingText.value = "正在下载并安装更新...";
          await update.downloadAndInstall();
          updateLoadingText.value = "更新安装完毕，即将重启...";
          await new Promise((r) => setTimeout(r, 800));
          await relaunch();
        }
      }
    } else {
      hasUpdate.value = false;
      if (!silent) {
        await message("当前已经是最新版本", { title: "检查更新" });
      }
    }
  } catch (e: any) {
    updateLoading.value = false;
    if (!silent) {
      const errorMsg =
        e?.message || (typeof e === "string" ? e : JSON.stringify(e));
      await message(`检查更新失败: ${errorMsg}`, {
        title: "错误",
        kind: "error",
      });
    }
  }
};

const expandAll = () => {
  collapsedDirs.value = new Set();
};

const getAllDirectoryIds = (node: any): string[] => {
  const ids: string[] = [];
  if (node.isDir) {
    if (node.id !== "root") ids.push(node.id);
    node.children.forEach((child: any) => {
      ids.push(...getAllDirectoryIds(child));
    });
  }
  return ids;
};

const collapseAll = () => {
  const tree = buildTree(scanResult.value);
  const allDirIds = getAllDirectoryIds(tree);
  collapsedDirs.value = new Set(allDirIds);
};

const openGithub = async () => {
  await openUrl("https://github.com/lifedever/smart-cleaner");
};

// Form State
const targetDir = ref("");
const minSizeMB = ref<number | "">("");
const createdBeforeDays = ref<number | "">("");
const modifiedBeforeDays = ref<number | "">("");
const extensions = ref("");
const includeEmptyDirs = ref(true);

const collapsedDirs = ref<Set<string>>(new Set());
const treeRoot = ref<any>(null);
const directoryTotalCount = new Map<string, number>();
const directorySelectedCount = ref<Map<string, number>>(new Map());

async function initStore() {
  try {
    store = await load("settings.json", { autoSave: true, defaults: {} });
    const savedWhitelist = await store.get("whitelist");
    if (savedWhitelist && Array.isArray(savedWhitelist)) {
      whitelist.value = savedWhitelist as string[];
    } else {
      await store.set("whitelist", []);
      await store.save();
    }

    const savedState: any = await store.get("form_state");
    if (savedState) {
      if (savedState.targetDir !== undefined)
        targetDir.value = savedState.targetDir;
      if (savedState.minSizeMB !== undefined)
        minSizeMB.value = savedState.minSizeMB;
      if (savedState.createdBeforeDays !== undefined)
        createdBeforeDays.value = savedState.createdBeforeDays;
      if (savedState.modifiedBeforeDays !== undefined)
        modifiedBeforeDays.value = savedState.modifiedBeforeDays;
      if (savedState.extensions !== undefined)
        extensions.value = savedState.extensions;
      if (savedState.includeEmptyDirs !== undefined)
        includeEmptyDirs.value = savedState.includeEmptyDirs;
    }
  } catch (e) {
    console.error("Failed to load store:", e);
  }
}

onMounted(async () => {
  try {
    appVersion.value = await getVersion();
  } catch (e) {
    console.error("Failed to get version:", e);
  }
  await initStore();

  watch(
    [
      targetDir,
      minSizeMB,
      createdBeforeDays,
      modifiedBeforeDays,
      extensions,
      includeEmptyDirs,
    ],
    async () => {
      if (store) {
        await store.set("form_state", {
          targetDir: targetDir.value,
          minSizeMB: minSizeMB.value,
          createdBeforeDays: createdBeforeDays.value,
          modifiedBeforeDays: modifiedBeforeDays.value,
          extensions: extensions.value,
          includeEmptyDirs: includeEmptyDirs.value,
        });
        await store.save();
      }
    },
    { deep: true },
  );

  // Auto check update once a day
  if (store) {
    const lastCheck = (await store.get("last_update_check")) as number;
    const now = Date.now();
    const oneDay = 24 * 60 * 60 * 1000;

    if (!lastCheck || now - lastCheck > oneDay) {
      await checkUpdate(true);
      await store.set("last_update_check", now);
      await store.save();
    }
  }

  window.addEventListener("click", () => {
    if (contextMenu.value.show) {
      contextMenu.value.show = false;
    }
  });

  _unlistenClean.value = await listen("clean-progress", (event: any) => {
    cleanProgress.value = {
      total: event.payload.total,
      current: event.payload.current,
      path: event.payload.current_path,
    };
  });
});

const isCleaning = ref(false);
const cleanProgress = ref({ total: 0, current: 0, path: "" });
const _unlistenClean = ref<any>(null);

const contextMenu = ref({
  show: false,
  x: 0,
  y: 0,
  item: null as any,
});

const showContextMenu = (e: MouseEvent, item: any) => {
  contextMenu.value = {
    show: true,
    x: e.clientX,
    y: e.clientY,
    item,
  };
};

const handleContextMenuAddWhitelist = async () => {
  const fileItem = contextMenu.value.item;
  contextMenu.value.show = false;
  if (!fileItem) return;

  // 1. Instantly remove from view
  const removedIds = new Set<string>();
  scanResult.value = scanResult.value.filter((f) => {
    // Exact match or sub-path match (with slash to prevent /a/b matching /a/bcd)
    const shouldRemove =
      f.path === fileItem.path ||
      f.path.startsWith(fileItem.path + "/") ||
      f.path.startsWith(fileItem.path + "\\");
    if (shouldRemove) {
      removedIds.add(f.id);
    }
    return !shouldRemove;
  });

  const nextSet = new Set(selectedIds.value);
  removedIds.forEach((id) => nextSet.delete(id));
  selectedIds.value = nextSet;
  treeRoot.value = buildTree(scanResult.value);

  const newSelCountMap = new Map<string, number>();
  const countSelected = (node: any): number => {
    let count = 0;
    if (!node.isDir) {
      if (selectedIds.value.has(node.id)) count = 1;
    } else {
      node.children.forEach((child: any) => {
        count += countSelected(child);
      });
      newSelCountMap.set(node.id, count);
    }
    return count;
  };
  countSelected(treeRoot.value);
  directorySelectedCount.value = newSelCountMap;

  // 2. Persist to whitelist
  if (!whitelist.value.includes(fileItem.path)) {
    whitelist.value.push(fileItem.path);
    await store.set("whitelist", Array.from(whitelist.value));
    await store.save();

    await message(
      `「${fileItem.name}」已加入白名单，该记录已从本次扫描列表中移除。`,
      { title: "操作成功", kind: "info" },
    );
  } else {
    await message(`「${fileItem.name}」已在白名单中，该记录已从列表中移除。`, {
      title: "提示",
      kind: "info",
    });
  }
};

const removeFromWhitelist = async (path: string) => {
  whitelist.value = whitelist.value.filter((p) => p !== path);
  await store.set("whitelist", Array.from(whitelist.value));
  await store.save();
};

const clearWhitelist = async () => {
  const confirm = await showConfirm("清空确认", "确定要清空所有白名单记录吗？");
  if (confirm) {
    whitelist.value = [];
    await store.set("whitelist", []);
    await store.save();
  }
};

const buildTree = (files: any[]) => {
  const root: any = {
    id: "root",
    name: "Root",
    path: targetDir.value,
    children: new Map(),
    isDir: true,
    size: 0,
    fileIds: [], // Direct files
  };

  directoryTotalCount.clear();

  files.forEach((file) => {
    const relativePath = file.path
      .replace(targetDir.value, "")
      .replace(/^[/\\]/, "");
    const parts = relativePath.split(/[/\\]/).filter((p: string) => p);

    let current = root;
    parts.forEach((part: string, index: number) => {
      const isLast = index === parts.length - 1;
      const fullPath =
        targetDir.value + "/" + parts.slice(0, index + 1).join("/");

      if (!current.children.has(part)) {
        if (isLast) {
          const node = {
            ...file,
            isDir: file.is_dir,
            children: new Map(),
          };
          current.children.set(part, node);
          if (!file.is_dir) {
            current.fileIds.push(file.id);
          }
        } else {
          current.children.set(part, {
            id: `dir-${fullPath}`,
            name: part,
            path: fullPath,
            isDir: true,
            size: 0,
            children: new Map(),
            fileIds: [],
          });
        }
      }
      current = current.children.get(part);
    });
  });

  // Calculate sizes and recursive counts
  const finalizeStats = (node: any) => {
    let sizeTotal = 0;
    let countTotal = 0;

    if (!node.isDir) {
      return { size: node.size, count: 1 };
    }

    node.children.forEach((child: any) => {
      const stats = finalizeStats(child);
      sizeTotal += stats.size;
      countTotal += stats.count;
    });

    node.size = sizeTotal;
    directoryTotalCount.set(node.id, countTotal);
    return { size: sizeTotal, count: countTotal };
  };

  finalizeStats(root);
  return root;
};

const flattenTree = (node: any, depth = -1): any[] => {
  const list: any[] = [];
  if (depth >= 0) {
    list.push({ ...node, depth });
  }

  if (node.isDir && (depth === -1 || !collapsedDirs.value.has(node.id))) {
    // Sort siblings based on global sortOrder
    const children = Array.from(node.children.values()).sort(
      (a: any, b: any) => {
        if (sortOrder.value === "none") {
          // Default: dirs first, then name
          if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
          return a.name.localeCompare(b.name);
        }

        const modifier = sortOrder.value === "asc" ? 1 : -1;
        // Keep dirs grouped at top or just follow size? Typically tree views group dirs.
        // Let's group dirs but sort both groups by size.
        if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
        return (a.size - b.size) * modifier;
      },
    );

    children.forEach((child: any) => {
      list.push(...flattenTree(child, depth + 1));
    });
  }
  return list;
};

const toggleDir = (id: string, e?: Event) => {
  if (e) e.stopPropagation();
  const nextSet = new Set(collapsedDirs.value);
  if (nextSet.has(id)) {
    nextSet.delete(id);
  } else {
    nextSet.add(id);
  }
  collapsedDirs.value = nextSet;
};

// Results State
const isScanning = ref(false);
const scanProgressPath = ref("");
const _unlistenProgress = ref<any>(null);

const scanResult = ref<any[]>([]);
const totalSize = ref(0);
const selectedIds = ref<Set<string>>(new Set());

const selectDirectory = async () => {
  const result = await open({
    directory: true,
    multiple: false,
  });
  if (result) {
    targetDir.value = result as string;
  }
};

const formatSize = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
};

const scanFiles = async (resetSelection = true) => {
  if (!targetDir.value) {
    await message("请先选择要清理的目录", { title: "提示", kind: "warning" });
    return;
  }

  scanProgressPath.value = "正在初始化扫描...";

  // Setup listener for progress
  if (!_unlistenProgress.value) {
    _unlistenProgress.value = await listen("scan-progress", (event: any) => {
      scanProgressPath.value = event.payload.current_path;
    });
  }

  isScanning.value = true;
  scanResult.value = [];
  selectedIds.value.clear();
  totalSize.value = 0;

  await nextTick();

  try {
    const now = Date.now();
    const createdBeforeMs = createdBeforeDays.value
      ? now - (createdBeforeDays.value as number) * 24 * 60 * 60 * 1000
      : null;
    const modifiedBeforeMs = modifiedBeforeDays.value
      ? now - (modifiedBeforeDays.value as number) * 24 * 60 * 60 * 1000
      : null;

    const exts = extensions.value
      .trim()
      .split(",")
      .map((e) => e.trim().replace(/^\./, ""))
      .filter((e) => e);

    const result: any = await invoke("scan_directory", {
      options: {
        target_dir: targetDir.value,
        min_size_mb: minSizeMB.value ? Number(minSizeMB.value) : null,
        created_before_ms: createdBeforeMs,
        modified_before_ms: modifiedBeforeMs,
        extensions: exts.length > 0 ? exts : null,
        include_empty_dirs: includeEmptyDirs.value,
        whitelist: whitelist.value,
      },
    });

    scanResult.value = result.files;
    totalSize.value = result.total_size;

    if (resetSelection) {
      collapsedDirs.value = new Set();
    }
    treeRoot.value = buildTree(scanResult.value);

    // Only auto-select all if not a refresh
    if (resetSelection) {
      const initialSelection = new Set<string>();
      const selCountMap = new Map<string, number>();
      scanResult.value.forEach((f) => {
        initialSelection.add(f.id);
      });

      // Helper to fill selection map at start
      directoryTotalCount.forEach((count, dirId) => {
        selCountMap.set(dirId, count);
      });
      directorySelectedCount.value = selCountMap;
      selectedIds.value = initialSelection;
    } else {
      // Refresh mode: clear selections that no longer exist
      const nextSet = new Set<string>();
      selectedIds.value.forEach((id) => {
        if (scanResult.value.some((f) => f.id === id)) {
          nextSet.add(id);
        }
      });
      selectedIds.value = nextSet;

      // Re-calculate counts for current selection
      const newSelCountMap = new Map<string, number>();
      const countSelected = (node: any): number => {
        let count = 0;
        if (!node.isDir) {
          if (selectedIds.value.has(node.id)) count = 1;
        } else {
          node.children.forEach((child: any) => {
            count += countSelected(child);
          });
          newSelCountMap.set(node.id, count);
        }
        return count;
      };
      countSelected(treeRoot.value);
      directorySelectedCount.value = newSelCountMap;
    }
  } catch (err: any) {
    await message(`扫描失败: ${err}`, { title: "错误", kind: "error" });
  } finally {
    isScanning.value = false;
    scanProgressPath.value = "";
    if (_unlistenProgress.value) {
      _unlistenProgress.value();
      _unlistenProgress.value = null;
    }
  }
};

// Removed unused helpers

const toggleSelection = (item: any) => {
  const alreadySelected = isAllSelected(item);
  const nextSet = new Set(selectedIds.value);
  const selCountMap = new Map(directorySelectedCount.value);

  const affectedFiles: string[] = [];
  const affectedDirs: string[] = [];

  const collect = (node: any) => {
    if (!node.isDir) {
      affectedFiles.push(node.id);
    } else {
      affectedDirs.push(node.id);
      node.children.forEach(collect);
    }
  };

  // Optimization: Find by path segments for O(depth) instead of O(N)
  const findNodeByPath = (targetPath: string) => {
    const parts = targetPath
      .replace(targetDir.value, "")
      .replace(/^[/\\]/, "")
      .split(/[/\\]/)
      .filter((p) => p);
    let current = treeRoot.value;
    if (targetPath === targetDir.value) return current;
    for (const part of parts) {
      if (!current || !current.children.has(part)) return null;
      current = current.children.get(part);
    }
    return current;
  };

  const node = findNodeByPath(item.path);
  if (!node) return;

  collect(node);

  const isAdding = !alreadySelected;

  affectedFiles.forEach((id) => {
    if (isAdding) nextSet.add(id);
    else nextSet.delete(id);
  });

  // Update selection counts for all affected directories AND their parents
  affectedDirs.forEach((dirId) => {
    if (isAdding) selCountMap.set(dirId, directoryTotalCount.get(dirId) || 0);
    else selCountMap.set(dirId, 0);
  });

  // Update parents
  const updateParents = (path: string) => {
    const parentPath = path.substring(0, path.lastIndexOf("/"));
    if (parentPath.startsWith(targetDir.value)) {
      const parentNode = findNodeByPath(parentPath);
      if (parentNode) {
        let count = 0;
        parentNode.children.forEach((child: any) => {
          if (!child.isDir) {
            if (nextSet.has(child.id)) count++;
          } else {
            count += selCountMap.get(child.id) || 0;
          }
        });
        selCountMap.set(parentNode.id, count);
        updateParents(parentPath);
      }
    } else if (path !== targetDir.value) {
      // Root case
      let count = 0;
      treeRoot.value.children.forEach((child: any) => {
        if (!child.isDir) {
          if (nextSet.has(child.id)) count++;
        } else {
          count += selCountMap.get(child.id) || 0;
        }
      });
      selCountMap.set("root", count);
    }
  };

  updateParents(item.path);

  directorySelectedCount.value = selCountMap;
  selectedIds.value = nextSet;
};

// Removed unused toggleSelectAll

const sortOrder = ref<"none" | "asc" | "desc">("desc");

// Removed unused toggleSort

const getFileIcon = (fileName: string, isDir: boolean) => {
  if (isDir) return "📁";

  const ext = fileName.split(".").pop()?.toLowerCase() || "";

  const iconMap: Record<string, string> = {
    // Images
    png: "🖼️",
    jpg: "🖼️",
    jpeg: "🖼️",
    gif: "🖼️",
    svg: "🖼️",
    webp: "🖼️",
    bmp: "🖼️",
    // Documents
    pdf: "📕",
    doc: "📘",
    docx: "📘",
    xls: "📗",
    xlsx: "📗",
    csv: "📗",
    ppt: "📙",
    pptx: "📙",
    txt: "📄",
    md: "📝",
    rtf: "📄",
    // Code
    html: "🌐",
    css: "🎨",
    js: "📜",
    ts: "📜",
    vue: "🟩",
    jsx: "⚛️",
    tsx: "⚛️",
    json: "📋",
    xml: "📋",
    yaml: "📋",
    yml: "📋",
    py: "🐍",
    java: "☕",
    c: "🇨",
    cpp: "🇨",
    cs: "#️⃣",
    go: "🐹",
    rs: "🦀",
    rb: "💎",
    php: "🐘",
    sh: "🐚",
    bash: "🐚",
    // Archives & Executables
    zip: "📦",
    rar: "📦",
    "7z": "📦",
    tar: "📦",
    gz: "📦",
    dmg: "💿",
    iso: "💿",
    exe: "🪟",
    app: "📱",
    apk: "📱",
    // Media
    mp4: "🎬",
    mkv: "🎬",
    avi: "🎬",
    mov: "🎬",
    wmv: "🎬",
    flv: "🎬",
    webm: "🎬",
    mp3: "🎵",
    wav: "🎵",
    ogg: "🎵",
    flac: "🎵",
    m4a: "🎵",
    // Misc
    sqlite: "🗄️",
    db: "🗄️",
    sql: "🗄️",
    log: "📋",
  };

  return iconMap[ext] || "📄"; // default fallback
};

const treeData = computed(() => {
  if (!treeRoot.value) return [];
  // Ensure we rebuild when sortOrder or collapsedDirs changes
  void sortOrder.value;
  void collapsedDirs.value;
  return flattenTree(treeRoot.value);
});

const isPartiallySelected = (item: any) => {
  if (!item.isDir) return false;
  const selected = directorySelectedCount.value.get(item.id) || 0;
  const total = directoryTotalCount.get(item.id) || 0;
  return selected > 0 && selected < total;
};

const isAllSelected = (item: any) => {
  if (!item.isDir) return selectedIds.value.has(item.id);
  const selected = directorySelectedCount.value.get(item.id) || 0;
  const total = directoryTotalCount.get(item.id) || 1; // avoid /0
  return selected === total;
};

const selectedSize = computed(() => {
  return scanResult.value
    .filter((f) => selectedIds.value.has(f.id))
    .reduce((acc, curr) => acc + curr.size, 0);
});

const handleBatchAddWhitelist = async () => {
  const pathsToAdd: string[] = [];

  const scanNode = (node: any) => {
    if (isAllSelected(node)) {
      pathsToAdd.push(node.path);
      return;
    }
    if (node.isDir && isPartiallySelected(node)) {
      node.children.forEach((child: any) => scanNode(child));
    }
  };

  if (treeRoot.value) {
    scanNode(treeRoot.value);
  }

  if (pathsToAdd.length === 0) {
    await message("请先勾选需要加入白名单的文件或文件夹", { kind: "warning" });
    return;
  }

  const confirm = await showConfirm(
    "批量加入白名单",
    `确定要将选定的 ${pathsToAdd.length} 个项目(会自动包含其子项目)加入白名单并隐藏吗？`,
  );
  if (!confirm) return;

  const removedIds = new Set<string>();
  scanResult.value = scanResult.value.filter((f) => {
    const shouldRemove = pathsToAdd.some(
      (p) =>
        p === f.path ||
        f.path.startsWith(p + "/") ||
        f.path.startsWith(p + "\\"),
    );
    if (shouldRemove) {
      removedIds.add(f.id);
    }
    return !shouldRemove;
  });

  const nextSet = new Set(selectedIds.value);
  removedIds.forEach((id) => nextSet.delete(id));
  selectedIds.value = nextSet;
  treeRoot.value = buildTree(scanResult.value);

  const newSelCountMap = new Map<string, number>();
  const countSelected = (node: any): number => {
    let count = 0;
    if (!node.isDir) {
      if (selectedIds.value.has(node.id)) count = 1;
    } else {
      node.children.forEach((child: any) => {
        count += countSelected(child);
      });
      newSelCountMap.set(node.id, count);
    }
    return count;
  };
  countSelected(treeRoot.value);
  directorySelectedCount.value = newSelCountMap;

  let addedCount = 0;
  for (const p of pathsToAdd) {
    if (!whitelist.value.includes(p)) {
      whitelist.value.push(p);
      addedCount++;
    }
  }

  if (addedCount > 0) {
    if (store) {
      await store.set("whitelist", Array.from(whitelist.value));
      await store.save();
    }
  }

  await message(`成功将 ${pathsToAdd.length} 项加入白名单并隐藏。`, {
    title: "操作成功",
    kind: "info",
  });
};

const executeClean = async () => {
  if (selectedIds.value.size === 0) return;

  const confirm = await showConfirm(
    "二次确认",
    `确认将选中的 ${selectedIds.value.size} 个文件（共计释放 ${formatSize(selectedSize.value)}）移入回收站吗？\n可在回收站进行恢复。`,
  );

  if (confirm) {
    const pathsToDelete = scanResult.value
      .filter((f) => selectedIds.value.has(f.id))
      .map((f) => f.path);

    isCleaning.value = true;
    cleanProgress.value = { total: pathsToDelete.length, current: 0, path: "" };

    try {
      await invoke("move_to_trash", {
        paths: pathsToDelete,
        targetDir: targetDir.value,
      });

      // Delay slightly for UX
      await new Promise((resolve) => setTimeout(resolve, 800));

      // 成功后重新触发一遍扫描以刷新列表
      scanFiles(false);
    } catch (err: any) {
      await message(`${err}`, {
        title: "部分操作失败",
        kind: "warning",
      });
      // 即便有部分失败，也可以刷新一下列表看看剩下哪些
      scanFiles(false);
    } finally {
      isCleaning.value = false;
    }
  }
};
</script>

<template>
  <div class="app-wrapper">
    <div class="layout">
      <!-- Sidebar Panel -->
      <aside class="sidebar">
        <div class="header">
          <div class="logo-box">
            <div class="icon-clean">✨</div>
            <h2>Smart Cleaner</h2>
          </div>
          <p class="subtitle">智能释放您的 Mac 空间</p>
        </div>

        <div class="card form-section">
          <h3>清理目标</h3>
          <div class="dir-selector">
            <input
              type="text"
              v-model="targetDir"
              placeholder="点击选择或输入绝对路径"
              readonly
              @click="selectDirectory"
            />
            <button @click="selectDirectory" class="btn-icon">📁</button>
          </div>
        </div>

        <div class="card form-section">
          <h3>筛选属性</h3>

          <div class="form-group">
            <label>文件体积筛选</label>
            <div class="input-with-unit">
              <span>大于</span>
              <input type="number" v-model="minSizeMB" placeholder="未设置" />
              <span>MB</span>
            </div>
          </div>

          <div class="form-group">
            <label>创建时间筛选</label>
            <div class="input-with-unit">
              <span>早于</span>
              <input
                type="number"
                v-model="createdBeforeDays"
                placeholder="未设置"
              />
              <span>天</span>
            </div>
          </div>

          <div class="form-group">
            <label>修改时间筛选</label>
            <div class="input-with-unit">
              <span>早于</span>
              <input
                type="number"
                v-model="modifiedBeforeDays"
                placeholder="未设置"
              />
              <span>天</span>
            </div>
          </div>

          <div class="form-group">
            <label>指定文件格式 (逗号分隔)</label>
            <input
              type="text"
              v-model="extensions"
              placeholder="例如: .dmg,.zip,.log"
            />
          </div>

          <div
            class="form-group"
            style="
              margin-top: 12px;
              display: flex;
              justify-content: space-between;
              align-items: center;
            "
          >
            <label class="checkbox-ctrl">
              <input type="checkbox" v-model="includeEmptyDirs" />
              包含并清理空目录
            </label>
          </div>
        </div>

        <div class="action-footer">
          <button
            class="primary btn-block scan-btn"
            :disabled="isScanning || !targetDir"
            @click="() => scanFiles(true)"
          >
            {{ isScanning ? "正在深度扫描..." : "开始扫描" }}
          </button>
        </div>
      </aside>

      <!-- Main Content Panel -->
      <main class="main-content">
        <header class="main-header">
          <h3>文件预览列表</h3>
          <div class="stats" v-if="scanResult.length > 0">
            <span class="badge"
              >已选 {{ selectedIds.size }} / 共 {{ scanResult.length }}</span
            >
            <span class="badge highlight"
              >可释放 {{ formatSize(selectedSize) }}</span
            >
          </div>
        </header>

        <div class="list-container">
          <div
            v-if="targetDir && !isScanning && scanResult.length === 0"
            class="empty-state"
          >
            <div class="empty-icon">🍃</div>
            <p>太棒了，目前该目录下没有符合清理条件的垃圾文件！</p>
          </div>
          <div v-else-if="!targetDir" class="empty-state">
            <div class="empty-icon">👈</div>
            <p>请先在左侧选择要扫描的目录并配置规则</p>
          </div>
          <div v-else-if="isScanning" class="empty-state">
            <div class="spinner"></div>
            <p>努力扫描中...</p>
            <div class="progress-path" v-if="scanProgressPath">
              {{ scanProgressPath }}
            </div>
          </div>

          <div v-else class="file-list">
            <div class="list-header">
              <div class="checkbox-ctrl">
                <input
                  type="checkbox"
                  :checked="
                    isAllSelected({ isDir: true, path: targetDir, id: 'root' })
                  "
                  :indeterminate="
                    isPartiallySelected({
                      isDir: true,
                      path: targetDir,
                      id: 'root',
                    })
                  "
                  @change="
                    toggleSelection({
                      isDir: true,
                      path: targetDir,
                      id: 'root',
                    })
                  "
                />
                <span style="font-size: 11px; margin-left: 4px; opacity: 0.8"
                  >全选</span
                >
              </div>
              <div
                class="col-name"
                style="display: flex; align-items: center; gap: 12px"
              >
                <span>名称</span>
                <div class="tree-controls" v-if="treeData.length > 0">
                  <button class="mini-btn" @click="expandAll" title="全部展开">
                    展开
                  </button>
                  <button
                    class="mini-btn"
                    @click="collapseAll"
                    title="全部折叠"
                  >
                    折叠
                  </button>
                  <button
                    v-if="selectedIds.size > 0"
                    class="mini-btn"
                    style="color: var(--accent); border-color: var(--accent)"
                    @click="handleBatchAddWhitelist"
                    title="批量将勾选的文件/文件夹加入白名单"
                  >
                    批量加白
                  </button>
                </div>
              </div>
              <div
                class="col-size"
                @click="sortOrder = sortOrder === 'desc' ? 'asc' : 'desc'"
                style="cursor: pointer; user-select: none"
              >
                大小 {{ sortOrder === "desc" ? "↓" : "↑" }}
              </div>
            </div>

            <RecycleScroller
              class="list-body"
              :items="treeData"
              :item-size="34"
              :buffer="200"
              key-field="id"
              v-slot="{ item }"
            >
              <div
                class="list-item-wrapper"
                :style="{ paddingLeft: item.depth * 18 + 8 + 'px' }"
              >
                <!-- Vertical connection line -->
                <div
                  v-if="item.depth > 0"
                  class="tree-line"
                  :style="{ left: item.depth * 18 - 8 + 'px' }"
                ></div>

                <div
                  class="list-item tree-row"
                  :class="{
                    selected: isAllSelected(item),
                    'is-dir': item.isDir,
                  }"
                  @click="
                    item.isDir ? toggleDir(item.id) : toggleSelection(item)
                  "
                  @contextmenu.prevent="showContextMenu($event, item)"
                >
                  <div class="checkbox-ctrl compact" @click.stop>
                    <input
                      type="checkbox"
                      :checked="isAllSelected(item)"
                      :indeterminate="isPartiallySelected(item)"
                      @change="toggleSelection(item)"
                    />
                  </div>
                  <div class="item-icon-compact">
                    <span
                      v-if="item.isDir && item.children.size > 0"
                      class="expand-arrow"
                      @click.stop="toggleDir(item.id)"
                      :class="{ collapsed: collapsedDirs.has(item.id) }"
                    >
                      <svg
                        viewBox="0 0 24 24"
                        fill="none"
                        xmlns="http://www.w3.org/2000/svg"
                        style="
                          width: 12px;
                          height: 12px;
                          transition: transform 0.2s;
                        "
                      >
                        <path
                          d="M7 10L12 15L17 10"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        />
                      </svg>
                    </span>
                    <span v-else class="dir-spacer"></span>
                    <span class="main-icon">
                      <template v-if="item.isDir">
                        <svg
                          viewBox="0 0 24 24"
                          fill="none"
                          xmlns="http://www.w3.org/2000/svg"
                          style="width: 18px; height: 18px; color: #a0a0a0"
                        >
                          <path
                            d="M3 7C3 5.89543 3.89543 5 5 5H9.58579C9.851 5 10.1054 5.10536 10.2929 5.29289L12.4142 7.41421C12.6017 7.60175 12.8561 7.70711 13.1213 7.70711H19C20.1046 7.70711 21 8.60168 21 9.70625V17C21 18.1046 20.1046 19 19 19H5C3.89543 19 3 18.1046 3 17V7Z"
                            fill="currentColor"
                            stroke="currentColor"
                            stroke-width="1.2"
                            stroke-linejoin="round"
                          />
                        </svg>
                      </template>
                      <template v-else>{{
                        getFileIcon(item.name, false)
                      }}</template>
                    </span>
                  </div>
                  <div class="item-info">
                    <div class="item-name-compact" :title="item.path">
                      {{ item.name }}
                    </div>
                  </div>
                  <div class="item-size-compact">
                    {{
                      item.isDir && item.size === 0 ? "" : formatSize(item.size)
                    }}
                  </div>
                </div>
              </div>
            </RecycleScroller>
          </div>
        </div>

        <footer class="main-footer" v-if="scanResult.length > 0">
          <div
            v-if="!isCleaning"
            class="footer-info-wrapper"
            style="
              display: flex;
              justify-content: space-between;
              align-items: center;
              width: 100%;
            "
          >
            <div class="selection-info">
              已选择 <strong>{{ selectedIds.size }}</strong> 项，总计将释放
              <strong class="highlight-text">{{
                formatSize(selectedSize)
              }}</strong>
              空间
            </div>
            <button
              class="danger confirm-btn"
              :disabled="selectedIds.size === 0"
              @click="executeClean"
            >
              🗑️ 确认移入回收站
            </button>
          </div>

          <!-- Cleaning Progress Bar -->
          <div v-else class="cleaning-progress-wrapper" style="width: 100%">
            <div
              style="
                display: flex;
                justify-content: space-between;
                margin-bottom: 8px;
                font-size: 13px;
              "
            >
              <span
                >正在清理: {{ cleanProgress.path.split(/[/\\]/).pop() }}</span
              >
              <span
                >{{ cleanProgress.current }} / {{ cleanProgress.total }}</span
              >
            </div>
            <div
              class="progress-bar-bg"
              style="
                height: 10px;
                background: #eee;
                border-radius: 5px;
                overflow: hidden;
              "
            >
              <div
                class="progress-bar-fill"
                :style="{
                  width:
                    (cleanProgress.total > 0
                      ? (cleanProgress.current / cleanProgress.total) * 100
                      : 0) + '%',
                }"
                style="
                  height: 100%;
                  background: var(--danger);
                  transition: width 0.3s ease;
                "
              ></div>
            </div>
          </div>
        </footer>
      </main>
    </div>

    <!-- Whitelist Modal -->
    <div
      class="modal-overlay"
      v-if="showWhitelistModal"
      @click.self="showWhitelistModal = false"
    >
      <div class="modal-content sidebar-modal">
        <header class="modal-header">
          <h3>🛡️ 扫描白名单</h3>
          <button class="close-btn" @click="showWhitelistModal = false">
            ✕
          </button>
        </header>
        <div class="modal-body">
          <p class="modal-desc">以下目录或文件将在执行扫描时被永远跳过：</p>
          <div v-if="whitelist.length === 0" class="empty-state mini">
            暂无白名单记录
          </div>
          <ul class="whitelist-list" v-else>
            <li v-for="path in whitelist" :key="path">
              <div
                style="
                  flex: 1;
                  display: flex;
                  flex-direction: column;
                  min-width: 0;
                  margin-right: 12px;
                  text-align: left;
                "
              >
                <span class="item-name" :title="path">{{
                  path.split(/[/\\]/).pop() || path
                }}</span>
                <span
                  class="item-path"
                  style="opacity: 0.6; font-size: 11px; margin-top: 2px"
                  >{{ path }}</span
                >
              </div>
              <button
                class="btn-text danger-text"
                @click="removeFromWhitelist(path)"
              >
                移除
              </button>
            </li>
          </ul>
        </div>
        <footer class="modal-footer" v-if="whitelist.length > 0">
          <button class="btn-text danger-text" @click="clearWhitelist">
            清空全部
          </button>
        </footer>
      </div>
    </div>

    <!-- About Modal -->
    <div
      class="modal-overlay"
      v-if="showAboutModal"
      @click.self="showAboutModal = false"
    >
      <div
        class="modal-content"
        style="
          max-width: 440px;
          border-radius: var(--radius-lg);
          overflow: hidden;
          box-shadow: 0 20px 40px rgba(0, 0, 0, 0.15);
        "
      >
        <header
          class="modal-header"
          style="
            justify-content: center;
            padding: 0;
            background: var(--surface-secondary);
            border-bottom: 1px solid var(--border);
            position: relative;
          "
        >
          <div style="display: flex; width: 100%">
            <button
              class="tab-btn"
              :class="{ active: activeTab === 'about' }"
              @click="activeTab = 'about'"
            >
              关于与更新
            </button>
            <button
              class="tab-btn"
              :class="{ active: activeTab === 'sponsor' }"
              @click="activeTab = 'sponsor'"
            >
              赞助作者
            </button>
          </div>
          <button
            class="close-btn"
            @click="showAboutModal = false"
            style="
              position: absolute;
              right: 16px;
              top: 12px;
              font-size: 16px;
              color: var(--text-muted);
              background: transparent;
              padding: 4px;
            "
          >
            ✕
          </button>
        </header>

        <div
          class="modal-body"
          v-if="activeTab === 'about'"
          style="text-align: center; padding: 24px"
        >
          <img
            src="/app-icon.svg"
            alt="Logo"
            style="width: 80px; height: 80px; margin-bottom: 16px"
          />
          <h2
            style="margin: 0 0 8px 0; color: var(--text-main); font-size: 20px"
          >
            Smart Cleaner
          </h2>
          <p
            style="
              margin: 0 0 16px 0;
              color: var(--text-muted);
              font-size: 13px;
              font-family: monospace;
              opacity: 0.8;
            "
          >
            v{{ appVersion }}
          </p>
          <p
            style="
              margin: 0 0 32px 0;
              color: var(--text-muted);
              font-size: 14px;
            "
          >
            一款轻量、极简的磁盘清理工具
          </p>

          <div
            style="
              display: flex;
              flex-direction: column;
              gap: 12px;
              align-items: center;
              max-width: 280px;
              margin: 0 auto;
            "
          >
            <button
              class="primary btn-block"
              @click="() => checkUpdate(false)"
              :disabled="updateLoading"
              style="
                font-size: 14px;
                display: flex;
                align-items: center;
                justify-content: center;
                gap: 6px;
                border-radius: 8px;
              "
            >
              <span v-if="updateLoading" class="spinner"></span>
              {{ updateLoading ? updateLoadingText : "🔄 检查新版本" }}
            </button>

            <button
              class="btn-text"
              @click="openGithub"
              style="
                font-size: 14px;
                color: var(--text-main);
                background: var(--surface-secondary);
                width: 100%;
                border-radius: 8px;
                padding: 12px;
                display: flex;
                align-items: center;
                justify-content: center;
                gap: 6px;
              "
            >
              <svg
                style="width: 16px; height: 16px; fill: currentColor"
                viewBox="0 0 16 16"
              >
                <path
                  d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z"
                ></path>
              </svg>
              前往 GitHub 主页
            </button>
          </div>
        </div>

        <div
          class="modal-body"
          v-if="activeTab === 'sponsor'"
          style="text-align: center; padding: 24px"
        >
          <p style="margin-top: 0; color: var(--text-main); font-size: 14px">
            如果这个工具帮您省出了大量空间<br />欢迎随意赞赏作者一杯饮品 🍹
          </p>
          <div
            style="
              display: flex;
              justify-content: center;
              gap: 24px;
              align-items: stretch;
              margin-top: 24px;
            "
          >
            <div
              style="
                flex: 1;
                display: flex;
                flex-direction: column;
                align-items: center;
                background: #fff;
                border-radius: 8px;
                padding: 12px;
                border: 1px solid var(--border);
              "
            >
              <img
                src="/wechatpay.JPG"
                alt="WeChat Pay"
                style="
                  width: 100%;
                  max-width: 140px;
                  object-fit: contain;
                  border-radius: 4px;
                "
              />
            </div>
            <div
              style="
                flex: 1;
                display: flex;
                flex-direction: column;
                align-items: center;
                background: #fff;
                border-radius: 8px;
                padding: 12px;
                border: 1px solid var(--border);
              "
            >
              <img
                src="/alipay.PNG"
                alt="Alipay"
                style="
                  width: 100%;
                  max-width: 140px;
                  object-fit: contain;
                  border-radius: 4px;
                "
              />
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- App Footer -->
    <footer class="app-footer">
      <div
        style="
          font-weight: 500;
          opacity: 0.8;
          display: flex;
          gap: 8px;
          align-items: center;
        "
      >
        Smart Cleaner
        <span style="font-size: 11px; opacity: 0.5">v{{ appVersion }}</span>
      </div>
      <div style="display: flex; gap: 16px">
        <button
          class="btn-text"
          @click="showWhitelistModal = true"
          style="
            color: var(--text-muted);
            background: transparent;
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 4px;
            padding: 4px;
          "
        >
          🛡️ 白名单 ({{ whitelist.length }})
        </button>
        <button
          class="btn-text"
          @click="showAboutModal = true"
          style="
            color: var(--text-muted);
            background: transparent;
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 4px;
            padding: 4px;
            position: relative;
          "
        >
          ℹ️ 关于
          <span v-if="hasUpdate" class="update-dot" title="有新版本可用"></span>
        </button>
      </div>
    </footer>

    <!-- Custom Context Menu -->
    <div
      v-if="contextMenu.show"
      class="context-menu"
      :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
      @click.stop
    >
      <div class="menu-item highlight" @click="handleContextMenuAddWhitelist">
        <span style="margin-right: 6px">🚫</span> 加入白名单并忽略
      </div>
    </div>

    <!-- Custom Confirm Modal -->
    <div
      class="modal-overlay"
      v-if="confirmModal.show"
      @click.self="confirmModal.show = false"
      style="z-index: 2000"
    >
      <div class="modal-content" style="max-width: 380px; padding: 24px">
        <h3 style="margin: 0 0 12px 0; font-size: 18px">
          {{ confirmModal.title }}
        </h3>
        <p style="margin: 0 0 24px 0; font-size: 14px; line-height: 1.6">
          {{ confirmModal.message }}
        </p>
        <div style="display: flex; justify-content: flex-end; gap: 12px">
          <button class="btn-text" @click="confirmModal.show = false">
            取消
          </button>
          <button class="danger" @click="confirmModal.onConfirm">确定</button>
        </div>
      </div>
    </div>

    <!-- Global Update Loading Overlay -->
    <div
      class="modal-overlay"
      v-if="updateLoading"
      style="
        z-index: 9999;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 24px;
        color: white;
      "
    >
      <div
        class="spinner"
        style="width: 48px; height: 48px; border-width: 4px"
      ></div>
      <div
        style="
          font-size: 16px;
          font-weight: 500;
          text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5);
        "
      >
        {{ updateLoadingText }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.app-wrapper {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
}

.layout {
  display: flex;
  flex: 1;
  width: 100%;
  background: var(--surface-secondary);
  overflow: hidden;
}

.app-footer {
  height: 48px;
  background: var(--surface);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 24px;
  font-size: 13px;
  color: var(--text-muted);
  box-shadow: 0 -2px 10px rgba(0, 0, 0, 0.02);
  z-index: 20;
}

.sidebar {
  width: 320px;
  background: var(--surface);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  padding: 20px;
  box-shadow: 2px 0 10px rgba(0, 0, 0, 0.02);
  z-index: 10;
}

.main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: var(--bg-color);
  min-width: 0;
}

/* Sidebar Components */
.header {
  margin-bottom: 24px;
}
.logo-box {
  display: flex;
  align-items: center;
  gap: 12px;
}
.icon-clean {
  font-size: 28px;
  background: linear-gradient(135deg, var(--primary), #a8b1e4);
  background-clip: text;
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
}
.subtitle {
  color: var(--text-muted);
  font-size: 13px;
  margin-top: 4px;
}

.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: 16px;
  margin-bottom: 16px;
  box-shadow: var(--shadow-sm);
}
.card h3 {
  font-size: 13px;
  color: var(--text-muted);
  margin-bottom: 16px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.dir-selector {
  display: flex;
  gap: 8px;
}
.dir-selector input {
  flex: 1;
  cursor: pointer;
}
.btn-icon {
  padding: 8px;
  background: var(--surface-secondary);
}

.form-group {
  margin-bottom: 14px;
}
.form-group label {
  display: block;
  font-size: 13px;
  color: var(--text-main);
  margin-bottom: 6px;
  font-weight: 500;
}
.input-with-unit {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--surface-secondary);
  border-radius: var(--radius-sm);
  padding: 0 12px;
  border: 1px solid var(--border);
  transition: var(--transition);
}
.input-with-unit:focus-within {
  border-color: var(--primary);
}
.input-with-unit span {
  font-size: 13px;
  color: var(--text-muted);
}
.input-with-unit input {
  border: none;
  background: transparent;
  padding: 8px 0;
  text-align: right;
  flex: 1;
  box-shadow: none !important;
}

.action-footer {
  margin-top: auto;
  padding-top: 16px;
}
.btn-block {
  width: 100%;
  padding: 12px;
  font-size: 15px;
  font-weight: 600;
  border-radius: var(--radius-md);
}

/* Main Content Components */
.main-header {
  height: 60px;
  padding: 0 24px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--border);
  background: var(--surface);
}
.stats {
  display: flex;
  gap: 12px;
}
.badge {
  background: var(--surface-secondary);
  padding: 4px 10px;
  border-radius: 20px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
}
.badge.highlight {
  background: rgba(92, 106, 196, 0.1);
  color: var(--primary);
}

.tree-controls {
  display: flex;
  gap: 8px;
}
.mini-btn {
  padding: 2px 10px;
  font-size: 10px;
  background: var(--surface-secondary);
  border: 1px solid var(--border);
  color: var(--text-muted);
  height: 20px;
  border-radius: 4px;
  opacity: 0.8;
  transition: all 0.2s ease;
}
.mini-btn:hover {
  opacity: 1;
  background: var(--border);
  color: var(--text-main);
}

.update-dot {
  position: absolute;
  top: 2px;
  right: -2px;
  width: 8px;
  height: 8px;
  background: #ff4d4f;
  border-radius: 50%;
  border: 1.5px solid var(--surface);
}

.spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.3);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
  flex-shrink: 0;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.list-container {
  flex: 1;
  overflow: hidden;
  position: relative;
  display: flex;
  flex-direction: column;
}

.empty-state {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  text-align: center;
  color: var(--text-muted);
}
.empty-icon {
  font-size: 48px;
  margin-bottom: 16px;
  opacity: 0.8;
}

.progress-path {
  margin-top: 12px;
  font-size: 12px;
  color: var(--text-muted);
  max-width: 400px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  opacity: 0.7;
}

.file-list {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.list-header {
  display: flex;
  align-items: center;
  padding: 12px 24px;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  font-size: 13px;
  font-weight: 600;
  color: var(--text-muted);
}
.list-header .checkbox-ctrl {
  width: 80px;
}
.col-name {
  flex: 1;
  padding-left: 36px;
}
.col-size {
  width: 100px;
  text-align: right;
}

.list-body {
  flex: 1;
  overflow-y: auto;
  padding: 8px 12px;
  /* virtual scroller internally needs block context */
  display: block;
}
.list-item-wrapper {
  position: relative;
  padding-bottom: 2px;
}
.tree-line {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--border);
  opacity: 0.5;
  z-index: 1;
}
.list-item.tree-row {
  display: flex;
  align-items: center;
  height: 32px;
  padding: 0 4px;
  font-size: 13px;
  border-radius: 4px;
  cursor: pointer;
  transition: background 0.1s ease;
  width: 100%;
}
.list-item.tree-row:hover {
  background: var(--surface-secondary);
}
.checkbox-ctrl.compact {
  width: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 2px;
}
.item-icon-compact {
  display: flex;
  align-items: center;
  margin-right: 6px;
  width: 40px; /* fixed width for alignment */
  justify-content: flex-start;
}
.expand-arrow {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  color: var(--text-muted);
  cursor: pointer;
  border-radius: 3px;
}
.expand-arrow:hover {
  background: rgba(0, 0, 0, 0.08);
  color: var(--text-main);
}
.expand-arrow svg {
  transform: rotate(0deg);
}
.expand-arrow.collapsed svg {
  transform: rotate(-90deg);
}
.dir-spacer {
  width: 16px;
}
.main-icon {
  font-size: 16px;
  width: 18px;
  height: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-left: 2px;
  opacity: 0.85; /* slight transparency for file emojis */
}
.item-info {
  flex: 1;
  min-width: 0;
}
.item-name-compact {
  font-size: 12.5px;
  font-weight: 400;
  color: var(--text-main);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  letter-spacing: -0.01em;
}
.item-size-compact {
  width: 75px;
  text-align: right;
  font-size: 11px;
  color: var(--text-muted);
  opacity: 0.6;
}
.list-item.tree-row.is-dir {
  font-weight: 500;
}
.list-item.selected {
  background: rgba(0, 0, 0, 0.04) !important;
}
.list-item.selected .item-name-compact {
  font-weight: 600;
}

.summary-bar {
  display: flex;
}

.main-footer {
  padding: 16px 24px;
  background: var(--surface);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  box-shadow: 0 -4px 10px rgba(0, 0, 0, 0.03);
  z-index: 10;
}
.selection-info {
  font-size: 14px;
}
.highlight-text {
  color: var(--primary);
  font-size: 16px;
}
.confirm-btn {
  padding: 10px 24px;
  font-size: 15px;
  box-shadow: var(--shadow-sm);
}

/* Spinner */
.spinner {
  width: 40px;
  height: 40px;
  border: 3px solid rgba(92, 106, 196, 0.1);
  border-radius: 50%;
  border-top-color: var(--primary);
  animation: spin 1s ease-in-out infinite;
  margin: 0 auto 16px;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* Item Actions */
.item-actions {
  display: flex;
  opacity: 0;
  transition: opacity 0.2s;
  padding-right: 8px;
}
.list-item:hover .item-actions {
  opacity: 1;
}

/* Modals */
.modal-overlay {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(0, 0, 0, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(2px);
}
.modal-content {
  background: var(--surface);
  border-radius: var(--radius-lg);
  width: 90%;
  max-width: 500px;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}
.modal-header {
  padding: 16px 20px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--border);
}
.modal-header h3 {
  margin: 0;
  font-size: 16px;
}
.close-btn {
  background: none;
  border: none;
  font-size: 18px;
  cursor: pointer;
  color: var(--text-muted);
}
.modal-body {
  padding: 20px;
  overflow-y: auto;
  flex: 1;
}
.modal-desc {
  font-size: 13px;
  color: var(--text-muted);
  margin-bottom: 16px;
}
.whitelist-list {
  list-style: none;
  padding: 0;
  margin: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  max-height: 300px;
  overflow-y: auto;
}
.whitelist-list li {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
  font-size: 13px;
}
.whitelist-list li:last-child {
  border-bottom: none;
}
.path-text {
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-right: 12px;
  color: var(--text-main);
  direction: rtl;
  text-align: left;
}
.danger-text {
  color: #e53935 !important;
}
.modal-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border);
  text-align: right;
  background: var(--surface-secondary);
}

.tab-btn {
  flex: 1;
  background: transparent;
  color: var(--text-muted);
  padding: 14px 0;
  font-size: 14px;
  font-weight: 500;
  border-bottom: 2px solid transparent;
  border-radius: 0;
}
.tab-btn:hover {
  background: rgba(0, 0, 0, 0.02);
}
.tab-btn.active {
  color: var(--primary);
  border-bottom-color: var(--primary);
  background: transparent;
}

/* Context Menu */
.context-menu {
  position: fixed;
  background: var(--surface);
  border: 1px solid var(--border);
  box-shadow: 0 8px 16px rgba(0, 0, 0, 0.12);
  border-radius: var(--radius-sm);
  padding: 6px;
  z-index: 1000;
  min-width: 180px;
}
.context-menu .menu-item {
  padding: 10px 14px;
  font-size: 13px;
  cursor: pointer;
  border-radius: 4px;
  display: flex;
  align-items: center;
  transition: var(--transition);
}
.context-menu .menu-item:hover {
  background: var(--surface-secondary);
}
.context-menu .menu-item.highlight {
  color: var(--text-main);
  font-weight: 500;
}
</style>
