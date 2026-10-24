# RustLibreSVIP

**LibreSVIP 的 Rust 重写版**：歌声合成工程转换器，原生 CLI、桌面 GUI、WebAssembly 网页版共用同一套转换核心。

[中文](#中文) · [English](#english)

---

## 中文

### 这是什么

基于 **LibreSVIP 2.9.2**（Python）的完整移植：40 种工程格式（`.ustx` `.svp` `.acep` `.ust` `.vsqx` `.mid` `.musicxml` `.ppsf` `.tlp` `.nn` …）的读写、6 个中间件、命令行、桌面 GUI 和网页版。

转换结果逐字节对齐 Python 参考实现（golden 测试 40/40），并在此基础上修复了上游若干缺陷（见下方「与 Python 版的差异」），因此本项目使用独立版本号：**1.0.0**。

### 速度

同一台机器、同一个 4 分钟 OpenUtau 工程（579 音符、21 个片段）：

| 转换 | Python 2.9.2 | Rust 1.0 | 提升 |
| --- | --- | --- | --- |
| `ustx` → `acep` | 24.7 s | **0.15 s** | ~160× |
| `ustx` → `svp` | 15.0 s | **0.06 s** | ~250× |
| `json` → `mid`（1.6 MiB 合成基准） | 17.0 s | **0.10 s** | ~178× |

原生单文件程序，无外部依赖：CLI 11.5 MB，GUI 13.5 MB。

### WebAssembly

`crates/libresvip-web` 把同一套核心编译成 `.wasm`：**零 import**，不需要 WASI，也不需要 wasm-bindgen，`WebAssembly.instantiate(bytes, {})` 即可运行。转换完全在浏览器本地完成，服务器只发静态文件。

| 产物 | 大小 |
| --- | --- |
| `libresvip_web.wasm` | 6.32 MB |
| gzip / brotli（部署时传输） | 2.17 MB / **1.60 MB** |
| `libresvip-single.html`（wasm 内联，双击即用） | 2.9 MB |

### 下载与发布

发布页提供三样东西：

| 文件 | 说明 |
| --- | --- |
| `libresvip-cli.exe` | 命令行版，单文件、免安装（Windows x64） |
| `libresvip-gui.exe` | 桌面版，免安装（Windows x64） |
| `libresvip-web.zip` | 网页版：`index.html` + `libresvip_web.wasm` + 单文件版 + 说明 |

### 构建

前置：Rust 1.80+（Windows 使用 MSVC 工具链），Node.js 仅用于打包网页版。

```powershell
git clone https://github.com/yjzxkxdn/RustLibreSVIP
cd RustLibreSVIP

# 原生 CLI + GUI
cargo build --release -p libresvip-cli -p libresvip-gui

# 网页版
rustup target add wasm32-unknown-unknown
cargo build --profile wasm -p libresvip-web --target wasm32-unknown-unknown
node web/build-web.mjs
```

GUI 安装（重置完整性标签 + 开始菜单快捷方式）：

```powershell
powershell -ExecutionPolicy Bypass -File .\install-gui.ps1 -Build
```

### 注意事项

* **不要把仓库放在“低完整性”目录里运行编译产物**（某些 sandbox / agent 工作区会给目录打上 Low 标签）。在那里编出来的 exe 会被 Windows 当作不可信程序：双击弹“安全警告”，进程以低权限启动，连 `%TEMP%` 都写不了。用 `install-gui.ps1` 复制到 `%LOCALAPPDATA%\Programs` 并重置完整性级别即可。
* 程序未做代码签名，首次运行可能触发 SmartScreen，选“仍要运行”；从命令行启动不会弹。
* wasm 构建需要 `wasm32-unknown-unknown` target；`[profile.wasm]` 使用 `opt-level="z"`（体积优先，转换约慢 7%）。
* 测试：`cargo test --workspace`（68 个单元测试 + 逐格式 golden 对比）。
* 网页版一次转换一个文件、使用默认选项；`split` / `merge` / 自定义选项目前在 CLI 和 GUI 中。

### 与 Python 版的差异

* **解析器不再把歌词抄成“发音”**（`ustx` / `mid` / `vspx` / `svip3`）。否则拼音歌词会被写进音素层，ACE Studio 认不出（`*[wo]`）。`.acep` 和 `.svp` 另有“写入音素层”开关，可整层留空或按目标引擎记法写入。
* `.acep` 的 pitch 曲线做了两处等价优化（结果逐字节一致），4 分钟工程从 6.7 s 降到 0.15 s。
* 其余差异（`--help` 排版、错误信息、音频探测只支持 WAV 等）记录在提交历史与 `PORTING.md`。

### 许可

MIT，见 [LICENSE](LICENSE)。上游 LibreSVIP © SoulMelody 与贡献者。

---

## English

**RustLibreSVIP** is a Rust rewrite of [LibreSVIP](https://github.com/SoulMelody/LibreSVIP), a converter for 40 singing-voice-synthesis project formats.

* **Based on** LibreSVIP 2.9.2 — byte-identical output for the golden fixtures (40/40).
* **Fast**: `ustx → acep` 24.7 s → **0.15 s**; `ustx → svp` 15.0 s → **0.06 s** (same 4-minute project).
* **WebAssembly ready**: the same core builds to a zero-import `.wasm` (6.32 MB, 1.60 MB brotli); conversions run in the browser and the server only serves static files.
* **Targets**: native CLI, native GUI (egui) and the web build share one engine. Release artifacts: `libresvip-cli.exe`, `libresvip-gui.exe`, `libresvip-web.zip`.

```powershell
cargo build --release -p libresvip-cli -p libresvip-gui
rustup target add wasm32-unknown-unknown
cargo build --profile wasm -p libresvip-web --target wasm32-unknown-unknown
node web/build-web.mjs
cargo test --workspace
```

Notes: do not run the binaries from a low-integrity (sandboxed) directory — Windows then treats them as untrusted and blocks file writes; `install-gui.ps1` copies the GUI to `%LOCALAPPDATA%\Programs` and resets the label. The wasm build needs the `wasm32-unknown-unknown` target.

Differences from the Python build: the parsers never store the lyric as a pronunciation (`ustx` / `mid` / `vspx` / `svip3`), so romanised lyrics no longer end up in a phoneme layer; `.acep` and `.svp` expose "write the phoneme layer" switches; the `.acep` pitch-curve path is algorithmically equivalent but ~45× faster.

MIT licensed — see [LICENSE](LICENSE). Upstream LibreSVIP © SoulMelody and contributors.