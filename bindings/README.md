# 用其它语言调用 LibreSVIP

转换引擎有三种暴露方式，选一种用就行：

| 语言 | 用哪个 | 怎么开始 |
| --- | --- | --- |
| Rust | `libresvip` crate | `libresvip::convert_bytes(&input, "ustx", "svp", None, None)` |
| C / C++ | `libresvip.dll` + `include/libresvip.h` | 见下方示例 |
| Python | `bindings/python/libresvip.py` | `LibreSVIP().convert(data, "ustx", "svp")` |
| C# / Java / Node / Delphi … | 同一个 C ABI | 按 `libresvip.h` 声明绑定 |
| 浏览器 | `libresvip_web.wasm` | 同一套 JSON 接口，零 import |

## 构建

```bash
cargo build --release -p libresvip-ffi
```

产物：

| 平台 | 文件 |
| --- | --- |
| Windows | `target/release/libresvip.dll` |
| Linux | `target/release/liblibresvip.so` |
| macOS | `target/release/liblibresvip.dylib` |

静态库同时也产出（`libresvip.lib` / `liblibresvip.a`），可以直接链进 C/C++ 程序。

## C

```c
#include <stdio.h>
#include "libresvip.h"

int main(void) {
    FILE *in = fopen("song.ustx", "rb");
    unsigned char data[1 << 20];
    size_t data_len = fread(data, 1, sizeof data, in);
    fclose(in);

    unsigned char *out = NULL;
    size_t out_len = 0;
    int status = libresvip_convert(data, data_len, "ustx", "svp",
                                   NULL, NULL, &out, &out_len);
    if (status != 0) {
        fprintf(stderr, "%.*s\n", (int)out_len, out);
        libresvip_free(out, out_len);
        return 1;
    }
    FILE *file = fopen("song.svp", "wb");
    fwrite(out, 1, out_len, file);
    fclose(file);
    libresvip_free(out, out_len);
    return 0;
}
```

编译（MinGW）：`gcc demo.c -I../include -L. -lresvip -o demo.exe`

## C++

头文件里带了一层包装，不用自己管缓冲区：

```cpp
#include "libresvip.h"
#include <fstream>
#include <vector>

int main() {
    std::ifstream in("song.ustx", std::ios::binary);
    std::vector<uint8_t> data((std::istreambuf_iterator<char>(in)), {});
    auto out = libresvip::convert(data, "ustx", "svp");
    std::ofstream("song.svp", std::ios::binary).write((const char *)out.data(), out.size());
}
```

## Python

```python
from libresvip import LibreSVIP

engine = LibreSVIP()                       # 或 LibreSVIP("C:/path/libresvip.dll")
print(engine.version, len(engine.formats()))

data = engine.convert(open("song.ustx", "rb").read(), "ustx", "svp")
open("song.svp", "wb").write(data)
for warning in engine.warnings():
    print(warning["kind"], warning["detail"])
```

找库的顺序：显式路径 → `LIBRESVIP_LIB` 环境变量 → 模块所在目录 → 仓库的 `target/{release,debug,fast}`。

## 约定

* 返回的缓冲区归调用方所有，必须用 `libresvip_free(ptr, len)` 释放；
* 返回 `0` 成功，非 0 时缓冲区里是 UTF-8 错误信息（不是异常，不会 abort）；
* 转换线程安全；`libresvip_warnings_json` 的报告按线程隔离；
* 选项是 JSON 对象，只写要改的字段，其余保持插件默认。
