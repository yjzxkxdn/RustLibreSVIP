# 用其它语言调用 RustLibreSVIP

转换引擎有三种暴露方式，选一种用就行：

| 语言 | 用哪个 | 怎么开始 |
| --- | --- | --- |
| Rust | `rust-libresvip` crate | `rust_libresvip::convert_bytes(&input, "ustx", "svp", None, None)` |
| C / C++ | `rustlibresvip.dll` + `include/rustlibresvip.h` | 见下方示例 |
| Python | `bindings/python/rustlibresvip.py` | `RustLibreSVIP().convert(data, "ustx", "svp")` |
| C# / Java / Node / Delphi … | 同一个 C ABI | 按 `rustlibresvip.h` 声明绑定 |
| 浏览器 | `libresvip_web.wasm` | 同一套 JSON 接口，零 import |

## 构建

```bash
cargo build --release -p rust-libresvip-ffi
```

产物：

| 平台 | 文件 |
| --- | --- |
| Windows | `target/release/rustlibresvip.dll` |
| Linux | `target/release/librustlibresvip.so` |
| macOS | `target/release/librustlibresvip.dylib` |

静态库同时也产出（`rustlibresvip.lib` / `librustlibresvip.a`），可以直接链进 C/C++ 程序。

## 可运行的例子

仓库里带三份**能直接编译运行**的示例，不是伪代码：

| 语言 | 文件 | 编译 / 运行 | 实测 |
| --- | --- | --- | --- |
| C | [`c/example.c`](c/example.c) | `gcc example.c -I ../../crates/rust-libresvip-ffi/include ../../target/release/rustlibresvip.dll -o example.exe` | `simple.json → ust`，317 字节 |
| C++ | [`cpp/example.cpp`](cpp/example.cpp) | `g++ example.cpp -I ../../crates/rust-libresvip-ffi/include ../../target/release/rustlibresvip.dll -o example.exe` | `simple.json → svp`，3570 字节 |
| Python | [`python/example.py`](python/example.py) | `python example.py song.json json svp song.svp` | `simple.json → vsqx`，3422 字节 |

`rustlibresvip.dll` 要和 `.exe` 放在同一目录（或者加到 PATH），Windows 才找得到。
MinGW 可以直接链 DLL（不必先生成 `.lib`）；MSVC 用 `rustlibresvip.dll.lib`：

```bat
cl example.c /I ..\..\crates\rust-libresvip-ffi\include rustlibresvip.dll.lib
```

## C

```c
#include <stdio.h>
#include "rustlibresvip.h"

int main(void) {
    FILE *in = fopen("song.ustx", "rb");
    unsigned char data[1 << 20];
    size_t data_len = fread(data, 1, sizeof data, in);
    fclose(in);

    unsigned char *out = NULL;
    size_t out_len = 0;
    int status = rustlibresvip_convert(data, data_len, "ustx", "svp",
                                   NULL, NULL, &out, &out_len);
    if (status != 0) {
        fprintf(stderr, "%.*s\n", (int)out_len, out);
        rustlibresvip_free(out, out_len);
        return 1;
    }
    FILE *file = fopen("song.svp", "wb");
    fwrite(out, 1, out_len, file);
    fclose(file);
    rustlibresvip_free(out, out_len);
    return 0;
}
```

编译（MinGW）：`gcc demo.c -I../include -L. -lresvip -o demo.exe`

## C++

头文件里带了一层包装，不用自己管缓冲区：

```cpp
#include "rustlibresvip.h"
#include <fstream>
#include <vector>

int main() {
    std::ifstream in("song.ustx", std::ios::binary);
    std::vector<uint8_t> data((std::istreambuf_iterator<char>(in)), {});
    auto out = rust_libresvip::convert(data, "ustx", "svp");
    std::ofstream("song.svp", std::ios::binary).write((const char *)out.data(), out.size());
}
```

## Python

```python
from rustlibresvip import RustLibreSVIP

engine = RustLibreSVIP()                       # 或 RustLibreSVIP("C:/path/rustlibresvip.dll")
print(engine.version, len(engine.formats()))

data = engine.convert(open("song.ustx", "rb").read(), "ustx", "svp")
open("song.svp", "wb").write(data)
for warning in engine.warnings():
    print(warning["kind"], warning["detail"])
```

找库的顺序：显式路径 → `RUSTLIBRESVIP_LIB` 环境变量 → 模块所在目录 → 仓库的 `target/{release,debug,fast}`。

## 约定

* 返回的缓冲区归调用方所有，必须用 `rustlibresvip_free(ptr, len)` 释放；
* 返回 `0` 成功，非 0 时缓冲区里是 UTF-8 错误信息（不是异常，不会 abort）；
* 转换线程安全；`rustlibresvip_warnings_json` 的报告按线程隔离；
* 选项是 JSON 对象，只写要改的字段，其余保持插件默认。
