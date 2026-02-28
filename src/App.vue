<script setup lang="ts">
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, message, ask } from "@tauri-apps/plugin-dialog";
import "./assets/styles.css";

// Form State
const targetDir = ref("");
const minSizeMB = ref<number | "">("");
const createdBeforeDays = ref<number | "">("");
const modifiedBeforeDays = ref<number | "">("");
const extensions = ref("");
const includeEmptyDirs = ref(false);

// Results State
const isScanning = ref(false);
const scanResult = ref<any[]>([]);
const totalSize = ref(0);
const selectedIds = ref<Set<String>>(new Set());

const selectDirectory = async () => {
  const result = await open({
    directory: true,
    multiple: false,
  });
  if (result) {
    targetDir.value = result as string;
    // Auto-scan when directory changes
    if (targetDir.value) {
      scanFiles();
    }
  }
};

const formatSize = (bytes: number) => {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
};

const scanFiles = async () => {
  if (!targetDir.value) {
    await message("请先选择要清理的目录", { title: "提示", kind: "warning" });
    return;
  }

  isScanning.value = true;
  scanResult.value = [];
  selectedIds.value.clear();
  totalSize.value = 0;

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
      },
    });

    scanResult.value = result.files;
    totalSize.value = result.total_size;

    // Select all by default
    scanResult.value.forEach((f) => selectedIds.value.add(f.id));
  } catch (err: any) {
    await message(`扫描失败: ${err}`, { title: "错误", kind: "error" });
  } finally {
    isScanning.value = false;
  }
};

const toggleSelection = (id: string) => {
  if (selectedIds.value.has(id)) {
    selectedIds.value.delete(id);
  } else {
    selectedIds.value.add(id);
  }
};

const toggleSelectAll = () => {
  if (selectedIds.value.size === scanResult.value.length) {
    selectedIds.value.clear();
  } else {
    scanResult.value.forEach((f) => selectedIds.value.add(f.id));
  }
};

const selectedSize = computed(() => {
  return scanResult.value
    .filter((f) => selectedIds.value.has(f.id))
    .reduce((acc, curr) => acc + curr.size, 0);
});

const executeClean = async () => {
  if (selectedIds.value.size === 0) return;

  const confirm = await ask(
    `确认将选中的 ${selectedIds.value.size} 个文件（共计释放 ${formatSize(selectedSize.value)}）移入回收站吗？\n可在回收站进行恢复。`,
    { title: "二次确认", kind: "warning" },
  );

  if (confirm) {
    const pathsToDelete = scanResult.value
      .filter((f) => selectedIds.value.has(f.id))
      .map((f) => f.path);

    try {
      await invoke("move_to_trash", { paths: pathsToDelete });
      await message("清理完成！已移入废纸篓。", {
        title: "成功",
        kind: "info",
      });
      // Rescan left files
      scanFiles();
    } catch (err: any) {
      await message(`清理过程中出现错误: ${err}`, {
        title: "错误",
        kind: "error",
      });
    }
  }
};
</script>

<template>
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

        <div class="form-group" style="margin-top: 12px">
          <label class="checkbox-ctrl">
            <input type="checkbox" v-model="includeEmptyDirs" />
            包含并清理空目录
          </label>
        </div>
      </div>

      <div class="action-footer">
        <button
          class="primary btn-block scan-btn"
          @click="scanFiles"
          :disabled="isScanning || !targetDir"
        >
          {{ isScanning ? "正在深度扫描..." : "开始扫描并预览" }}
        </button>
      </div>
    </aside>

    <!-- Main Content Panel -->
    <main class="main-content">
      <header class="main-header">
        <h3>文件预览列表</h3>
        <div class="stats" v-if="scanResult.length > 0">
          <span class="badge">共 {{ scanResult.length }} 项</span>
          <span class="badge highlight"
            >可释放 {{ formatSize(totalSize) }}</span
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
        </div>

        <div v-else class="file-list">
          <div class="list-header">
            <label class="checkbox-ctrl">
              <input
                type="checkbox"
                :checked="selectedIds.size === scanResult.length"
                @change="toggleSelectAll"
              />
              全选
            </label>
            <span class="col-name">名称</span>
            <span class="col-size">大小</span>
          </div>

          <div class="list-body">
            <div
              v-for="item in scanResult"
              :key="item.id"
              class="list-item"
              :class="{ selected: selectedIds.has(item.id) }"
              @click="toggleSelection(item.id)"
            >
              <div class="checkbox-ctrl">
                <input
                  type="checkbox"
                  :checked="selectedIds.has(item.id)"
                  @change="toggleSelection(item.id)"
                  @click.stop
                />
              </div>
              <div class="item-icon">{{ item.is_dir ? "📁" : "📄" }}</div>
              <div class="item-info">
                <div class="item-name" :title="item.path">{{ item.name }}</div>
                <div class="item-path">{{ item.path }}</div>
              </div>
              <div class="item-size">
                {{ item.is_dir ? "空目录" : formatSize(item.size) }}
              </div>
            </div>
          </div>
        </div>
      </div>

      <footer class="main-footer" v-if="scanResult.length > 0">
        <div class="selection-info">
          已选择 <strong>{{ selectedIds.size }}</strong> 项，总计将释放
          <strong class="highlight-text">{{ formatSize(selectedSize) }}</strong>
          空间
        </div>
        <button
          class="danger confirm-btn"
          :disabled="selectedIds.size === 0"
          @click="executeClean"
        >
          🗑️ 确认移入回收站
        </button>
      </footer>
    </main>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  height: 100vh;
  width: 100vw;
  background: var(--surface-secondary);
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
}
.list-item {
  display: flex;
  align-items: center;
  padding: 12px;
  border-radius: var(--radius-md);
  margin-bottom: 4px;
  cursor: pointer;
  background: var(--surface);
  border: 1px solid transparent;
  transition: var(--transition);
}
.list-item:hover {
  background: var(--surface-secondary);
}
.list-item.selected {
  background: rgba(92, 106, 196, 0.05);
  border-color: rgba(92, 106, 196, 0.2);
}
.list-item .checkbox-ctrl {
  width: 40px;
}
.item-icon {
  font-size: 20px;
  width: 36px;
  text-align: center;
}
.item-info {
  flex: 1;
  min-width: 0;
}
.item-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-main);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.item-path {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 4px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.item-size {
  width: 100px;
  text-align: right;
  font-size: 13px;
  font-variant-numeric: tabular-nums;
  color: var(--text-muted);
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
</style>
