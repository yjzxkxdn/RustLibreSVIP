# RustLibreSVIP

**歌声合成工程转换器的 Rust 实现**：桌面 GUI、命令行和 WebAssembly 网页版共用一套转换核心。**双上游**：**LibreSVIP 2.9.2** 提供格式插件、OpenSVIP 轴心与 CLI/GUI/RPC 交互，**UtaFormatix3** 提供能力矩阵、结构化警告、音高语义层、日文歌词与音素映射等设计与算法。

[中文](#中文) · [English](#english)

---

## 中文

### 这是什么

* **43 种工程格式**：LibreSVIP 2.9.2 的 40 个插件全部移植（`.ustx` `.svp` `.acep` `.ust` `.vsqx` `.mid` `.musicxml` `.ppsf` `.tlp` `.nn` …），另外补上上游隔离区里的 `gj`、`msq`、`vsp` 三个格式，并把 `ppsf` 的 legacy 二进制容器做完。
* **11 个中间件**：LibreSVIP 的 6 个（音高变调、工程缩放、歌词发音转换、移除数字与标点、移除短静音、歌词替换）+ 本实现新增的 5 个（日文歌词、音素映射、颤音展开、音高量化、节奏量化）。
* **一套核心三端共用**：`libresvip-cli`、`libresvip-gui`（egui）、`libresvip-web`（wasm32，零 import，无 wasm-bindgen、无 WASI）。

### 两个上游（共同基座）

本项目不是"LibreSVIP 的移植外加一点参考"：两边的产出都在主干上，缺一不可。

| 来源 | 拿来了什么 | 没有拿什么 |
| --- | --- | --- |
| **LibreSVIP 2.9.2**（MIT，SoulMelody／岁月静好_初心不忘） | 40 个格式的读写、OpenSVIP `.json` 轴心模型、6 个中间件、CLI/GUI 交互与 RPC 协议 | — |
| **UtaFormatix3**（Apache-2.0，SderColin，参考 `f3c8335`／2025-05） | 只借鉴设计与算法并自行实现：格式能力矩阵、结构化警告、音高语义层、颤音展开、日文歌词、音素映射、中间格式版本协商 | 没有引入它的格式集合；没有采用它的模板生成器方案（见 T9） |

> UtaFormatix3 是 Apache-2.0，本仓库是 MIT。因此**没有复制它的代码**，只吸收上面列出的设计与算法思路。

### UtaFormatix3 带来的能力（逐条，参考 `f3c8335`）

| 编号 | 能力 | 落在哪 |
| --- | --- | --- |
| T1 | 格式能力矩阵（只读元数据 + 由它生成的表格） | `core/capabilities.rs`、`plugins/capabilities.rs`、`plugin capabilities` |
| T2 | 结构化警告（kind + detail），贯穿 CLI / GUI / wasm / MCP | `core/warning_types.rs` |
| T3 | 扩展位与访问器（`Note.attrs`、`Params::get/set`） | `core/model/base.rs` |
| T4 | 语义层骨架（音高数学从插件上收） | `core/semantics/` |
| T5 | 音高语义：阶梯↔线性、绝对↔相对、音符边界补点、重叠检测 | `core/semantics/pitch/` |
| T6 | 日文歌词：类型检测（70%）+ 假名↔罗马字 + CV↔VCV | `middlewares/japanese_lyrics.rs`、`core/semantics/lyrics.rs` |
| T7 | 音素映射（用户表 + 默认恒等） | `middlewares/phoneme_mapping.rs` |
| T8 | 中间格式版本校验（`.json` 主版本、`.ufdata` 的 `formatVersion`） | `json.rs`、`ufdata.rs` |
| T9 | 模板生成器（vsqx / vpr / s5p / ccs） | **未做**：这四套输出已被 golden 字节级钉住，再引一条等价生成路径收益低、风险高 |

### 从 LibreSVIP 2.9.2 修掉的东西（因此输出与 Python 不同）

下面四条是上游的 bug。本实现按正常逻辑修了，并在 `testdata/golden/*/meta.json` 的 `deviations` 里逐条登记：

1. **`aisp` 导入**：上游每处理一个 item 就把音高数组重新绑定到当前 item，再 `extend(自己)` —— 结果只剩最后一个 item，且每个点重复两次。现在按 item 顺序累积。
2. **`vspx` 导入**：上游拿未偏移的 `param.time` 去减已经偏移过的 `prev_tick`，差值恒定多出一个 `first_bar_length`，真正的断点反而漏掉。现在两边在同一坐标系。
3. **`key_number == 0`**：上游 `if value:` 把 0 当"没有基音"，C-1 音符没有音高。现在 0 是合法音高（`ust`、`vpr`、`vsqx`、`y77`、`aisp` 里的同类判断一并修）。
4. **`replace_lyric` 中间件**：上游把 `replace_lyric(text, replacement)` 的参数传反，任何非空规则集都抛 `TypeError`。现在按规则顺序做正则替换。

其余是性能与健壮性修复，**不改变输出字节**：时间轴重映射改前缀和、tempo 表共享、曲线所有权改写、Vocaloid 系列音高缓存的 `Vec`→`HashMap`（`vsqx`/`vsq`/`xvsq`/`vpr` 从 12 s 降到 0.4 s）、`<divisions>`/`ticks_per_beat`/拍号分母为 0 时明确报错、`svip3` 的 int32 溢出报错、设置文件原子写入。

### 与 Python 版的其它差异（取舍，不是 bug）

* **`.svp` 默认不写音素层**（`write_phonemes=false`）：Python 会给每个音符写 `normalize_phoneme`。打开该选项即按 SynthV 记法写入；`.acep` 的 `write_syllable` 默认 `true`，与 Python 一致。
* **解析器不把歌词抄成"发音"**（`ustx` / `mid` / `vspx` / `svip3`），否则拼音歌词会被写进音素层。
* **支持 USTX 0.10（OpenUtau 1.571+）**：`masked_curves` 里的绝对音高曲线会真正导入 —— `pito`（用户覆盖）有值处覆盖音高，没有 `pitd` 时用 `rpit`（渲染音高）兜底；音符 `tuning` 计入音高。上游 2.9.2 只把 `MaskedCurve` 加进了类型联合，字段本身没有接收，数据被 pydantic 静默丢掉。同时 `voice_parts: null` 这类空列表也能读了。
* 新增三个 CLI 表面：`proj convert --middleware <id>`、`--strict-warnings`、`plugin capabilities [--json]`；**交互提问仍与 Python 完全一致**（6 个中间件，顺序相同），新增的 5 个中间件不参与提问。
* 新增的 3 个格式后缀（`gj` / `msq` / `vsp`）会出现在"支持的扩展名"报错列表里。
* `--help` 排版、错误信息文案与 Python 不同；音频时长探测只支持 RIFF/WAVE（上游用 MediaInfo/libsndfile）；`utf-32` 文本编码不受支持。

### 速度

同一台机器、同一个 4 分钟 OpenUtau 工程（579 音符、21 个片段）：

| 转换 | Python 2.9.2 | Rust 1.1 | 提升 |
| --- | --- | --- | --- |
| `ustx` → `acep` | 24.7 s | **0.15 s** | ~160× |
| `ustx` → `svp` | 15.0 s | **0.06 s** | ~250× |
| `json` → `mid`（1.6 MiB 合成基准） | 17.0 s | **0.10 s** | ~178× |

Vocaloid 系与音高密集的格式，按 937 音符的真实工程（同一台机器，修复前 = 1.0.0 分支的构建）：

| 转换 | 1.1.0 前 | 1.1.0 |
| --- | --- | --- |
| `ustx` → `vsqx` | 14.89 s | **0.24 s** |
| `ustx` → `vpr` | 9.39 s | **0.15 s** |
| `ustx` → `vsq` | 12.58 s | **0.35 s** |
| `ustx` → `xvsq` | 11.10 s | **0.35 s** |
| `ustx` → `vvproj` | 6.93 s | **0.16 s** |
| `ustx` → `y77` | 13.24 s | **0.14 s** |
| `ustx` → `aisp`（84k 点音高曲线） | 105.3 s | **1.01 s** |

原生单文件程序，无外部依赖：CLI 11.0 MB，GUI 12.5 MB。

本机真实工程语料（4,706 个文件里挑 111 个，522 次转换，逐个串行跑）的完整结果见
[docs/benchmark.md](docs/benchmark.md)：对 Python 参考实现中位提速 **140×**（n=23，61×–416×），
对 UtaFormatix3 中位 **13.8×**（n=118，按目标格式 10.6×–22.0×）；同一进程内连续转换（warm）中位 **10.3 ms**（冷启动 31.1 ms），wasm 稳态 **15.9 ms**；同一输入下与 Python 的音符**逐字节一致率 99.9%**、
音高曲线完全一致，与 UtaFormatix3 的音高曲线中位差 **0.11 半音**。复现脚本在
[scripts/bench_corpus](scripts/bench_corpus)。

### WebAssembly

`crates/libresvip-web` 把同一套核心编译成 `.wasm`：**零 import**，不需要 WASI，也不需要 wasm-bindgen，`WebAssembly.instantiate(bytes, {})` 即可运行。转换完全在浏览器本地完成，服务器只发静态文件。

| 产物 | 大小 |
| --- | --- |
| `libresvip_web.wasm` | 5.06 MB（5,061,056 字节，经 binaryen `-Oz`） |
| gzip / brotli（部署时传输） | 2.36 MB / **1.89 MB** |
| `libresvip-single.html`（wasm 内联，双击即用） | 3.20 MB |

### 作为库使用

转换引擎本身就是一个库：Rust 直接依赖，其他语言通过 C ABI。

| 层 | 产物 | 用法 |
| --- | --- | --- |
| Rust | `libresvip` crate | `libresvip::convert_bytes(&input, "ustx", "svp", None, None)?` |
| C / C++ | `libresvip.dll` + `libresvip.h` | `libresvip_convert(...)`；头文件里另有 C++ 的 `libresvip::convert` 包装 |
| Python | `bindings/python/libresvip.py` | `LibreSVIP().convert(data, "ustx", "svp")` |
| WebAssembly | `libresvip_web.wasm` | 同一套 JSON 接口 |

```rust
let output = libresvip::convert_bytes(
    &std::fs::read("song.ustx")?,
    "ustx",
    "svp",
    None,          // 输入选项，None = 插件默认
    None,          // 输出选项
)?;
```

```python
from libresvip import LibreSVIP

engine = LibreSVIP()            # 自动在 target/release、LIBRESVIP_LIB 里找
data = engine.convert(open("song.ustx", "rb").read(), "ustx", "svp")
open("song.svp", "wb").write(data)
print(engine.warnings())        # 转换过程中的结构化警告
```

C ABI（`libresvip.h`）：

| 函数 | 作用 |
| --- | --- |
| `libresvip_version()` | 版本字符串 |
| `libresvip_formats_json()` | 全部格式、后缀、能力、每文件轨道上限 |
| `libresvip_options_json(in, out)` | 一对格式的选项 schema |
| `libresvip_convert(...)` | 一次转换（可传 JSON 选项覆盖默认值） |
| `libresvip_warnings_json()` | 本线程上次转换的警告 |
| `libresvip_free(ptr, len)` | 释放返回的缓冲区 |

返回的缓冲区归调用方所有，必须用 `libresvip_free` 释放；返回 `0` 表示成功，非 0 时缓冲区里是 UTF-8 错误信息。转换是线程安全的，警告按线程隔离。

### 下载与发布

发布页提供三样东西：

| 文件 | 说明 |
| --- | --- |
| `libresvip-cli.exe` | 命令行版，单文件、免安装（Windows x64） |
| `libresvip-gui.exe` | 桌面版，免安装（Windows x64） |
| `libresvip.dll` | 转换引擎动态库（C ABI，Windows x64） |
| `libresvip.h` | C / C++ 头文件 |
| `libresvip-python.zip` | Python 绑定：`libresvip.py` + `libresvip.dll` + 示例 |
| `libresvip-web.zip` | 网页版：`index.html` + `libresvip_web.wasm` + 单文件版 + 说明 |

### 构建

前置：Rust 1.80+（Windows 使用 MSVC 工具链），Node.js 仅用于打包网页版。

```powershell
git clone https://github.com/yjzxkxdn/RustLibreSVIP
cd RustLibreSVIP

# 原生 CLI + GUI（发布用，fat LTO，约 10 分钟）
cargo build --release -p libresvip-cli -p libresvip-gui

# 日常迭代：同样的优化级别，不做 LTO、并行 codegen，改完链接几秒到一两分钟
cargo build --profile fast -p libresvip-gui

# 网页版
rustup target add wasm32-unknown-unknown
cargo build --profile wasm -p libresvip-web --target wasm32-unknown-unknown
node web/build-web.mjs        # 有 wasm-opt 在 PATH 上时会先跑 binaryen -Oz
```

网页版的回归检查（需要 Node 18+）：

```powershell
cd web
npm install                   # 只装 devDependency jsdom
npm run check-ui              # 在 jsdom 里真实启动单文件版并断言 UI
npm run check-abi             # 直接驱动 wasm 的 C ABI，跑一次 json→ustx
npm run check-page            # 页面静态 id 检查，不需要 wasm
```

`check-ui` 会验证：43 个格式都注册了、Ustx 的 5 个输入选项与 ACE 的 8 个输出选项都渲染出来、每个控件都带 `data-side`（这正是 1.1.1 修的那个崩溃）、翻转开关后角标变成"1 项已改"、控制台无异常。任何一条不满足就返回非零。

GUI 安装（重置完整性标签 + 开始菜单快捷方式）：

```powershell
powershell -ExecutionPolicy Bypass -File .\install-gui.ps1 -Build
```

### 注意事项

* **不要把仓库放在“低完整性”目录里运行编译产物**（某些 sandbox / agent 工作区会给目录打上 Low 标签）。在那里编出来的 exe 会被 Windows 当作不可信程序：双击弹“安全警告”，进程以低权限启动，连 `%TEMP%` 都写不了。用 `install-gui.ps1` 复制到 `%LOCALAPPDATA%\Programs` 并重置完整性级别即可。
* 程序未做代码签名，首次运行可能触发 SmartScreen，选“仍要运行”；从命令行启动不会弹。
* wasm 构建需要 `wasm32-unknown-unknown` target；`[profile.wasm]` 使用 `opt-level="z"`（体积优先，转换约慢 7%）。部署形态按 **brotli -11 压缩后**的体积算：当前 1,894,858 字节（原始 6,015,525）；相对移植前的 1,791,914（原始 5,614,286）增加约 5.8%，来源是新增的 gj / msq / vsp / ppsf-legacy 四个格式与语义层。
* 测试：`cargo test --workspace`（158 个测试 + 逐格式 golden 对比 + 43 格式能力表一致性检查）。
* 网页版支持多文件批量转换和每个格式的选项面板（输入/输出选项由 `options_json` 动态渲染）；`split` / `merge` 目前在 CLI 和 GUI 中。

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

**本项目作者**：依旧在星空下等你（[B 站](https://space.bilibili.com/299528104) · [GitHub](https://github.com/yjzxkxdn)）。

MIT，见 [LICENSE](LICENSE)。上游 LibreSVIP © SoulMelody（岁月静好_初心不忘）与贡献者；UtaFormatix3 © SderColin 与贡献者（Apache-2.0，仅借鉴设计与算法，未复制代码）。

---

## English

**RustLibreSVIP** is a Rust implementation built on two upstreams: [LibreSVIP](https://github.com/SoulMelody/LibreSVIP) 2.9.2 (format plugins, the OpenSVIP hub model, CLI/GUI/RPC) and [UtaFormatix3](https://github.com/sdercolin/utaformatix3) (`f3c8335`, design and algorithms only). It handles 43 singing-voice-synthesis project formats through one engine shared by a native CLI, a desktop GUI and a zero-import WebAssembly build.

* **From LibreSVIP 2.9.2**: all 40 format plugins, the hub model and the CLI/GUI/RPC surface — byte-identical output for the golden fixtures (40/40), plus four intentional fixes of upstream bugs (recorded per format in `testdata/golden/*/meta.json`).
* **From UtaFormatix3** (design only, no code copied): the format-capability matrix, structured warnings, the pitch-semantics layer (RDP / resampling / interpolation / absolute↔relative), vibrato expansion, Japanese lyric detection with CV↔VCV, phoneme mapping and intermediate-format version negotiation. Its template-generator scheme (T9) was not adopted.
* **Fast**: `ustx → acep` 24.7 s → **0.15 s**; `ustx → svp` 15.0 s → **0.06 s**; `ustx → vsqx` 14.89 s → **0.24 s** (a 4-minute, 937-note project). On a corpus of 111 real projects (352 native conversions, one process at a time) the median speed-up over the Python reference is **140×**, and a median **13.8×** over UtaFormatix3 (10.6×–22.0× per target format); converting continuously in one process costs **10.3 ms** per project (31.1 ms cold) and the wasm build settles at **15.9 ms** — see [docs/benchmark.md](docs/benchmark.md).
* **WebAssembly ready**: the same core builds to a zero-import `.wasm` (5.06 MB after binaryen `-Oz`, 1.89 MB brotli); conversions run in the browser and the server only serves static files.
* **Targets**: native CLI, native GUI (egui) and the web build share one engine. Release artifacts: `libresvip-cli.exe`, `libresvip-gui.exe`, `libresvip-web.zip`.

```powershell
cargo build --release -p libresvip-cli -p libresvip-gui

# day-to-day: same optimisation, no LTO, parallel codegen
cargo build --profile fast -p libresvip-gui
rustup target add wasm32-unknown-unknown
cargo build --profile wasm -p libresvip-web --target wasm32-unknown-unknown
node web/build-web.mjs
cd web; npm install; npm run check-ui; npm run check-abi
cargo test --workspace
```

Notes: do not run the binaries from a low-integrity (sandboxed) directory — Windows then treats them as untrusted and blocks file writes; `install-gui.ps1` copies the GUI to `%LOCALAPPDATA%\Programs` and resets the label. The wasm build needs the `wasm32-unknown-unknown` target.

Differences from the Python build: the parsers never store the lyric as a pronunciation (`ustx` / `mid` / `vspx` / `svip3`); `.svp` defaults to an empty phoneme layer while `.acep` keeps writing syllables; `aisp` / `vspx` pitch assembly, `key_number = 0` handling and the `replace_lyric` middleware follow the intended behaviour instead of the 2.9.2 bugs. Five extra middlewares (Japanese lyrics, phoneme mapping, vibrato baking, pitch and rhythm quantisation) are opt-in: the interactive prompts stay identical to Python's six, and `proj convert --middleware <id>` runs an extra one with its default options.

**Project author**: 依旧在星空下等你 ([Bilibili](https://space.bilibili.com/299528104) · [GitHub](https://github.com/yjzxkxdn)).

MIT licensed — see [LICENSE](LICENSE). Upstream LibreSVIP © SoulMelody (岁月静好_初心不忘) and contributors; UtaFormatix3 © SderColin and contributors (Apache-2.0, design and algorithms only).
