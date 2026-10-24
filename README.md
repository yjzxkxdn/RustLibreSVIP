# RustLibreSVIP

**LibreSVIP 的 Rust 重写版**：歌声合成工程转换器，原生 CLI、桌面 GUI、WebAssembly 网页版共用同一套转换核心。

[中文](#中文) · [English](#english)

---

## 中文

### 这是什么

基于 **LibreSVIP 2.9.2**（Python）的完整移植：43 种工程格式（`.ustx` `.svp` `.acep` `.ust` `.vsqx` `.mid` `.musicxml` `.ppsf` `.tlp` `.nn` …）的读写、11 个中间件（6 个移植自上游，5 个本移植新增）、命令行、桌面 GUI 和网页版。

转换结果逐字节对齐 Python 参考实现（golden 测试 40/40），并在此基础上修复了上游若干缺陷（见下方「与 Python 版的差异」），因此本项目使用独立版本号：**1.1.0**。

### 速度

同一台机器、同一个 4 分钟 OpenUtau 工程（579 音符、21 个片段）：

| 转换 | Python 2.9.2 | Rust 1.1 | 提升 |
| --- | --- | --- | --- |
| `ustx` → `acep` | 24.7 s | **0.15 s** | ~160× |
| `ustx` → `svp` | 15.0 s | **0.06 s** | ~250× |
| `json` → `mid`（1.6 MiB 合成基准） | 17.0 s | **0.10 s** | ~178× |

原生单文件程序，无外部依赖：CLI 11.0 MB，GUI 12.5 MB。

### WebAssembly

`crates/libresvip-web` 把同一套核心编译成 `.wasm`：**零 import**，不需要 WASI，也不需要 wasm-bindgen，`WebAssembly.instantiate(bytes, {})` 即可运行。转换完全在浏览器本地完成，服务器只发静态文件。

| 产物 | 大小 |
| --- | --- |
| `libresvip_web.wasm` | 6.01 MB（6,011,630 字节） |
| gzip / brotli（部署时传输） | 2.28 MB / **1.81 MB** |
| `libresvip-single.html`（wasm 内联，双击即用） | 3.07 MB |

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
* wasm 构建需要 `wasm32-unknown-unknown` target；`[profile.wasm]` 使用 `opt-level="z"`（体积优先，转换约慢 7%）。部署形态按 **brotli -11 压缩后**的体积算：当前 1,894,900 字节（原始 6,011,630）；相对移植前的 1,791,914（原始 5,614,286）增加约 5.8%，来源是新增的 gj / msq / vsp / ppsf-legacy 四个格式与语义层。
* 测试：`cargo test --workspace`（158 个测试 + 逐格式 golden 对比 + 43 格式能力表一致性检查）。
* 网页版支持多文件批量转换和每个格式的选项面板（输入/输出选项由 `options_json` 动态渲染）；`split` / `merge` 目前在 CLI 和 GUI 中。

### 与 Python 版的差异

* **解析器不再把歌词抄成“发音”**（`ustx` / `mid` / `vspx` / `svip3`）。否则拼音歌词会被写进音素层，ACE Studio 认不出（`*[wo]`）。`.acep` 和 `.svp` 另有“写入音素层”开关，可整层留空或按目标引擎记法写入。
* **新增 5 个上游没有的中间件**：日文歌词转假名（`japanese_lyrics`）、音素映射（`phoneme_mapping`）、颤音烘焙（`vibrato`）、音高量化（`pitch_quantization`）、节奏量化（`rhythm_quantization`）。它们默认关闭，也**不进入交互提问**——`proj convert` 仍然只问 Python 那 6 个中间件，提问顺序与 Python 逐字一致；新增的 5 个用 `proj convert --middleware <id>` 显式启用（可重复，按各中间件的默认选项运行），GUI 里则是默认未勾选的条目。
* `.acep` 的 pitch 曲线做了两处等价优化（结果逐字节一致），4 分钟工程从 6.7 s 降到 0.15 s。
* 其余差异（`--help` 排版、错误信息、音频探测只支持 WAV 等）记录在提交历史与 `PORTING.md`。

### 已知差异（刻意保留的上游行为与明确的报错化）

以下四处是 Python 上游的既定行为，golden 测试逐字节钉住，本移植**原样保留**；修改它们属于契约变更，需要同步改 golden 基准并在此声明：

* `aisp` 解析器的“自复制”怪癖（`aisp/parser.rs` 有注释）；
* `vspx` 断点坐标的混用记法（`vspx/parser.rs` 有注释）；
* `key_number = 0` 被当作“无基音”（`ust/pitch.rs` 的真值判断，与 Python 相同）；
* `replace_lyric` 中间件沿用了上游的参数顺序 bug，任何非空规则集都会像 Python 一样报错。

以下行为从“静默产出垃圾文件”改成了**明确报错**（Python 在同样输入下也会崩溃，只是异常形态不同）：

* MusicXML 缺 `<divisions>` 或其值为 0（Python 是 `assert` / `ZeroDivisionError`）；
* MIDI 的 `ticks_per_beat == 0`（Python 是 `ZeroDivisionError`）；
* `.svp` 导出时参数曲线出现 0 值采样点（Python 是 `ZeroDivisionError`）；
* `svip3` 导出时 tick 超出 `int32` 范围（Python 是 `struct.error`）；
* UST 文本编码名未知时（Python 是 `LookupError`）；另外 `utf-32` 在本移植中不受支持（`encoding_rs` 无此编码）。

其他刻意选择：

* 粤拼（jyutping）多音字取**字典序第一个**读法（Python 的 `set.pop` 依赖进程内哈希序，输出不确定）；
* `.json` / `.ufdata` 读到比本构建支持更新的主版本时给出 `incompatible version` 警告而不是静默丢字段。

### 上游隔离区格式的处置（gj / msq / vsp / nep / ppsf legacy）

Python 仓库里有三批不在 `libresvip.plugins` 目录、因而程序本身**不加载**的转换代码：`experimental/`（vsp、msq、nep）、`deprecated/`（gj）以及 ppsf 的 legacy 二进制分支。本移植按“插件完整性”原则全部纳入：

* `gj`（Gejigeji v2.0，`deprecated/`）与 `msq`（Mikoto Studio，`experimental/`）、`vsp`（Vocalina Studio 早期二进制格式，`experimental/`）移植为正式插件，对拍测试（`experimental_plugins.rs`）用 Python 原代码生成的 fixture 钉住：hub 模型结构一致，导出字节一致（`gj` 的 JSON 剥离 null 后一致）。`gj` 保留了上游“每读一次整体右移一小节”的往返漂移怪癖。
* `ppsf` 的 legacy 二进制格式（`PPSF` chunk 容器，与新版 ZIP+JSON 并存）已完整移植：`use_legacy_format` 选项现在真正可用，导出与 Python 字节级一致。解析侧只深度解码 hub 模型用到的四个 chunk（`CLPS`/`EVTS`/`PLGS`/`TRKS`），其余 chunk 按声明长度跳过——即损坏的 `PROJ`/`EDTS` 等块在 Python（construct 全量校验）会报错，这里跳过，属于错误行为差异，正常文件不受影响。
* `nep`（`experimental/nep/`）上游只有一份 2 KB 的 pydantic model 草稿，没有 parser / generator / converter，**无物可移**，未注册为插件。

### 格式能力表

`libresvip-cli plugin capabilities --json` 输出每个格式携带的信息（五条参数曲线、音素层、颤音、伴奏轨、单轨上限），下表由同一数据源生成（`ALL_CAPABILITIES`，`crates/libresvip-plugins/src/capabilities.rs`）：

<!-- FORMAT-CAPABILITIES-START -->
| 格式 | 音高 | 音量 | 气声 | 性别 | 强度 | 音素 | 颤音 | 伴奏轨 | 单轨 |
| --- | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: | :-: |
| `.ace` | ✓ |  |  |  | ✓ | ✓ |  |  |  |
| `.acep` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |
| `.aisp` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.ass` |  |  |  |  |  |  |  |  | ✓ |
| `.ccs` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.ds` |  |  |  | ✓ |  | ✓ |  |  | ✓ |
| `.dspx` | ✓ |  |  |  |  | ✓ | ✓ |  |  |
| `.dv` | ✓ |  |  |  |  | ✓ | ✓ |  |  |
| `.gj` | ✓ | ✓ |  |  |  | ✓ |  |  |  |
| `.json` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |
| `.lrc` |  |  |  |  |  |  |  |  | ✓ |
| `.mid` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.msq` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |  |
| `.mtp` | ✓ | ✓ |  |  |  | ✓ |  |  |  |
| `.musicxml` |  | ✓ |  |  |  |  |  |  |  |
| `.nn` |  |  |  |  |  |  | ✓ |  | ✓ |
| `.ppsf` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.ps_project` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.s5p` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.srt` |  |  |  |  |  |  |  |  | ✓ |
| `.svg` |  |  |  |  |  |  |  |  | ✓ |
| `.svip` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |
| `.svip3` | ✓ |  |  |  |  | ✓ | ✓ |  |  |
| `.svp` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |
| `.tlp` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.tlpx` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.tsmsln` |  |  |  |  |  |  | ✓ |  | ✓ |
| `.tssln` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.ufdata` | ✓ |  |  |  |  |  |  |  |  |
| `.ust` |  |  |  |  |  |  |  |  | ✓ |
| `.ustx` | ✓ |  |  |  |  | ✓ | ✓ |  |  |
| `.vfp` | ✓ |  |  |  |  | ✓ | ✓ |  |  |
| `.vog` |  |  |  |  |  | ✓ |  |  |  |
| `.vpr` | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |  |  |
| `.vshp` | ✓ | ✓ | ✓ | ✓ |  |  |  |  |  |
| `.vspx` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.vsq` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |  |
| `.vsqx` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |  |
| `.vsp` | ✓ |  |  |  |  |  |  |  |  |
| `.vvproj` | ✓ |  |  |  |  |  |  |  |  |
| `.vxf` | ✓ |  |  |  |  | ✓ |  |  |  |
| `.xvsq` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |  |  |  |
| `.y77` |  |  |  |  |  | ✓ |  |  | ✓ |
音高/音量/气声/性别/强度 = 五条参数曲线（导入或导出至少一侧支持）；单轨 = 一个输出文件只容纳一条轨道。
<!-- FORMAT-CAPABILITIES-END -->


### 许可

MIT，见 [LICENSE](LICENSE)。上游 LibreSVIP © SoulMelody 与贡献者。

---

## English

**RustLibreSVIP** is a Rust rewrite of [LibreSVIP](https://github.com/SoulMelody/LibreSVIP), a converter for 43 singing-voice-synthesis project formats.

* **Based on** LibreSVIP 2.9.2 — byte-identical output for the golden fixtures (40/40).
* **Fast**: `ustx → acep` 24.7 s → **0.15 s**; `ustx → svp` 15.0 s → **0.06 s** (same 4-minute project).
* **WebAssembly ready**: the same core builds to a zero-import `.wasm` (6.01 MB, 1.81 MB brotli); conversions run in the browser and the server only serves static files.
* **Targets**: native CLI, native GUI (egui) and the web build share one engine. Release artifacts: `libresvip-cli.exe`, `libresvip-gui.exe`, `libresvip-web.zip`.

```powershell
cargo build --release -p libresvip-cli -p libresvip-gui
rustup target add wasm32-unknown-unknown
cargo build --profile wasm -p libresvip-web --target wasm32-unknown-unknown
node web/build-web.mjs
cargo test --workspace
```

Notes: do not run the binaries from a low-integrity (sandboxed) directory — Windows then treats them as untrusted and blocks file writes; `install-gui.ps1` copies the GUI to `%LOCALAPPDATA%\Programs` and resets the label. The wasm build needs the `wasm32-unknown-unknown` target.

Differences from the Python build: the parsers never store the lyric as a pronunciation (`ustx` / `mid` / `vspx` / `svip3`), so romanised lyrics no longer end up in a phoneme layer; `.acep` and `.svp` expose "write the phoneme layer" switches; the `.acep` pitch-curve path is algorithmically equivalent but ~45× faster. Five extra middlewares (Japanese lyrics, phoneme mapping, vibrato baking, pitch and rhythm quantisation) are opt-in: the interactive prompts stay identical to Python's six, and `proj convert --middleware <id>` runs an extra one with its default options.

MIT licensed — see [LICENSE](LICENSE). Upstream LibreSVIP © SoulMelody and contributors.