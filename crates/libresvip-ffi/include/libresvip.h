/*
 * libresvip.h - C ABI of the LibreSVIP conversion engine.
 *
 * Link against libresvip.dll / liblibresvip.so / liblibresvip.dylib, or load it
 * at runtime (LoadLibrary / dlopen / ctypes).
 *
 * Memory rules:
 *   - every buffer returned through (out_ptr, out_len) is owned by the caller
 *     and must be released with libresvip_free(ptr, len);
 *   - strings are NUL-terminated UTF-8, buffers are raw bytes;
 *   - a return value of 0 means success; anything else is an error and the same
 *     output buffer holds the UTF-8 message instead of the payload.
 *
 * Example (C):
 *
 *     unsigned char *out = NULL; size_t len = 0;
 *     int status = libresvip_convert(data, data_len, "ustx", "svp",
 *                                    NULL, NULL, &out, &len);
 *     if (status == 0) fwrite(out, 1, len, stdout);
 *     else fprintf(stderr, "%.*s\n", (int)len, out);
 *     libresvip_free(out, len);
 */

#ifndef LIBRESVIP_H
#define LIBRESVIP_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#  define LIBRESVIP_API __declspec(dllimport)
#else
#  define LIBRESVIP_API
#endif

/* Engine version, e.g. "1.2.0". Static storage: do not free. */
LIBRESVIP_API const char *libresvip_version(void);

/* Releases a buffer returned by this library. */
LIBRESVIP_API void libresvip_free(uint8_t *pointer, size_t len);

/* Writes [{"id","name","description","suffixes","features","max_tracks_per_file"}]
 * for every registered format. Returns 0. */
LIBRESVIP_API int32_t libresvip_formats_json(uint8_t **out_ptr, size_t *out_len);

/* Writes {"input":[...],"output":[...]} describing the option schema of one
 * format pair. Returns 0, or non-zero with the message in the buffer. */
LIBRESVIP_API int32_t libresvip_options_json(const char *input_format,
                                             const char *output_format,
                                             uint8_t **out_ptr, size_t *out_len);

/* Converts one project file.
 *
 * `input_options` / `output_options` are optional JSON objects overriding the
 * plugin's default options ({"export_singer": true}); pass NULL for defaults.
 * Returns 0 and the converted file, or non-zero and an error message. */
LIBRESVIP_API int32_t libresvip_convert(const uint8_t *input, size_t input_len,
                                        const char *input_format,
                                        const char *output_format,
                                        const char *input_options,
                                        const char *output_options,
                                        uint8_t **out_ptr, size_t *out_len);

/* Writes [{"kind","detail"}] for the last conversion on the calling thread. */
LIBRESVIP_API int32_t libresvip_warnings_json(uint8_t **out_ptr, size_t *out_len);

#ifdef __cplusplus
} /* extern "C" */

#include <stdexcept>
#include <string>
#include <vector>

namespace libresvip {

class Error : public std::runtime_error {
public:
    explicit Error(const std::string &message) : std::runtime_error(message) {}
};

namespace detail {
inline std::string take(uint8_t *pointer, size_t len) {
    std::string text(reinterpret_cast<const char *>(pointer), len);
    libresvip_free(pointer, len);
    return text;
}
} // namespace detail

inline std::string version() { return libresvip_version(); }

inline std::string formats_json() {
    uint8_t *pointer = nullptr;
    size_t len = 0;
    libresvip_formats_json(&pointer, &len);
    return detail::take(pointer, len);
}

inline std::vector<uint8_t> convert(const std::vector<uint8_t> &input,
                                    const std::string &input_format,
                                    const std::string &output_format,
                                    const std::string &input_options = {},
                                    const std::string &output_options = {}) {
    uint8_t *pointer = nullptr;
    size_t len = 0;
    const int status = libresvip_convert(
        input.data(), input.size(), input_format.c_str(), output_format.c_str(),
        input_options.empty() ? nullptr : input_options.c_str(),
        output_options.empty() ? nullptr : output_options.c_str(), &pointer, &len);
    if (status != 0) throw Error(detail::take(pointer, len));
    std::vector<uint8_t> output(pointer, pointer + len);
    libresvip_free(pointer, len);
    return output;
}

} // namespace libresvip
#endif

#endif /* LIBRESVIP_H */
