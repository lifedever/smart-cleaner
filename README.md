# Smart Cleaner (智能清理工具)

一款采用 **Tauri v2 + Vue 3 + Rust** 开发的现代化、轻量级 Mac 目录清理工具。致力于帮助用户快速扫描任意文件夹，根据多种自定义属性过滤并清理庞大的垃圾文件。

> 注：为了确保数据安全，此应用会将扫描出的文件移入 Mac 的“废纸篓”，你可以随时在废纸篓中将其恢复。

## 🌟 核心特性

1. **体积小巧**：依赖原生系统 Webview 和底层 Rust，独立包体积仅十几MB。
2. **丰富的过滤规则**：
   - 过滤大文件（如：大于 `100` MB）。
   - 按时间清理（如：创建/修改时间早于 `30` 天）。
   - 按文件格式精确打击（如：`.log`, `.tmp`, `.dmg`）。
   - 支持空文件夹检测与一并清理。
3. **安全可视化确认**：所有的删除动作发生前，均会在列表展示出命中的文件，并自动计算出能够释放的存储空间。
4. **现代化 UI 体验**：采用极简原生的毛玻璃与平滑动效风格，操作直观。

---

## 📥 安装与首运行 (macOS)

由于此应用目前为个人开发且**未经过苹果官方签名**，下载安装后初次打开可能会提示“应用已损坏”或“无法验证开发者”。

请在终端（Terminal）执行以下命令来解除安全限制：

```bash
sudo xattr -rd com.apple.quarantine "/Applications/Smart Cleaner.app"
```

执行后输入开机密码，即可正常开启。

---

## 💻 快速开始

### 运行环境要求

- Node.js (推荐 v18+)
- pnpm 包管理器
- Rust (Cargo)

### 开发环境调试

```bash
# 1. 安装前端所有依赖
pnpm install

# 2. 启动 Tauri 本地开发模式
pnpm tauri dev
```

### 构建打包发行版 (.app / .dmg)

```bash
# 这会在 src-tauri/target/release/bundle 生成 macOS 的原生文件
pnpm tauri build
```

---

## 🛠 技术栈

- **UI 页面**：[Vue 3](https://vuejs.org/) (Composition API) + Vanilla CSS
- **前端构建**：[Vite 6](https://vitejs.dev/)
- **桌面框架**：[Tauri v2](https://v2.tauri.app/)
- **后端内核**：[Rust](https://www.rust-lang.org/) (使用 `walkdir` 进行快速目录遍历，使用 `trash` 进行安全移除)

---

## 🗂 项目结构

```text
📦 smart-cleaner
 ┣ 📂 src               // Vue 3 前端代码源码
 ┃ ┣ 📂 assets          // 静态资源与全局样式 (styles.css)
 ┃ ┣ 📜 App.vue         // 主布局、所有的UI交互和状态管理
 ┃ ┗ 📜 main.ts         // Vue 实例挂载入口
 ┣ 📂 src-tauri         // Rust 后端与 Tauri 配置
 ┃ ┣ 📂 src
 ┃ ┃ ┗ 📜 lib.rs        // 核心命令：目录扫描 (scan_directory)、移动至废纸篓 (move_to_trash)
 ┃ ┣ 📜 Cargo.toml      // Rust 依赖清单
 ┃ ┗ 📜 tauri.conf.json // Tauri 权限与打包配置
 ┣ 📜 vite.config.ts    // Vite 前端构建配置
 ┗ 📜 package.json      // Node 依赖管理
```

---

## ✨ 支持与赞助 (Sponsor)

如果您觉得 Smart Cleaner 对您有帮助，为您的电脑清理了大量冗余空间，欢迎请作者喝杯奶茶！您的赞助将是我前进的动力。

| 微信支付 | 支付宝 |
| :---: | :---: |
| ![微信支付](./public/wechatpay.JPG) | ![支付宝](./public/alipay.PNG) |

---

<p align="center">
  开源不易，非常感谢您的支持与陪伴！❤️
</p>
<p align="center">
  项目官网与源码: <a href="https://github.com/lifedever/smart-cleaner">https://github.com/lifedever/smart-cleaner</a>
</p>

[回到顶部](#smart-cleaner)

---

## 📝 License

本项目基于 MIT License 协议开源。
