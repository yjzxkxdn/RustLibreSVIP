/* C++ example: the header ships a small wrapper, so no manual buffer handling.
 *
 *   g++ example.cpp -I ../../crates/rust-libresvip-ffi/include \
 *       ../../target/release/rustlibresvip.dll -o example.exe
 *   example.exe song.json json ust song.ust
 */

#include <fstream>
#include <iostream>
#include <iterator>
#include <vector>

#include "rustlibresvip.h"

int main(int argc, char **argv) {
    if (argc < 5) {
        std::cerr << "usage: " << argv[0]
                  << " <input> <input-format> <output-format> <output>\n";
        return 2;
    }

    std::ifstream source(argv[1], std::ios::binary);
    if (!source) {
        std::cerr << argv[1] << ": cannot open\n";
        return 1;
    }
    const std::vector<uint8_t> input((std::istreambuf_iterator<char>(source)),
                                     std::istreambuf_iterator<char>());

    std::cout << "rustlibresvip " << rustlibresvip::version() << "\n";

    try {
        const std::vector<uint8_t> output = rustlibresvip::convert(input, argv[2], argv[3]);
        std::ofstream(argv[4], std::ios::binary)
            .write(reinterpret_cast<const char *>(output.data()), (std::streamsize)output.size());
        std::cout << "wrote " << argv[4] << " (" << output.size() << " bytes)\n";
    } catch (const rustlibresvip::Error &error) {
        std::cerr << "conversion failed: " << error.what() << "\n";
        return 1;
    }
    return 0;
}
