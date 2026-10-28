/* Minimal C example: convert one project file with the C ABI.
 *
 *   gcc example.c -I ../../crates/rust-libresvip-ffi/include \
 *       ../../target/release/rustlibresvip.dll -o example.exe
 *   example.exe song.json json ust song.ust
 */

#include <stdio.h>
#include <stdlib.h>

#include "rustlibresvip.h"

static unsigned char *read_file(const char *path, size_t *size) {
    FILE *file = fopen(path, "rb");
    if (!file) {
        perror(path);
        return NULL;
    }
    fseek(file, 0, SEEK_END);
    long length = ftell(file);
    fseek(file, 0, SEEK_SET);
    unsigned char *data = malloc((size_t)length);
    if (!data || fread(data, 1, (size_t)length, file) != (size_t)length) {
        fprintf(stderr, "%s: read failed\n", path);
        free(data);
        fclose(file);
        return NULL;
    }
    fclose(file);
    *size = (size_t)length;
    return data;
}

int main(int argc, char **argv) {
    if (argc < 5) {
        fprintf(stderr, "usage: %s <input> <input-format> <output-format> <output>\n", argv[0]);
        return 2;
    }

    size_t input_len = 0;
    unsigned char *input = read_file(argv[1], &input_len);
    if (!input) return 1;

    printf("rustlibresvip %s\n", rustlibresvip_version());

    unsigned char *out = NULL;
    size_t out_len = 0;
    int status = rustlibresvip_convert(input, input_len, argv[2], argv[3],
                                       NULL, NULL, &out, &out_len);
    free(input);

    if (status != 0) {
        fprintf(stderr, "conversion failed: %.*s\n", (int)out_len, out);
        rustlibresvip_free(out, out_len);
        return 1;
    }

    FILE *file = fopen(argv[4], "wb");
    if (!file) {
        perror(argv[4]);
        rustlibresvip_free(out, out_len);
        return 1;
    }
    fwrite(out, 1, out_len, file);
    fclose(file);
    printf("wrote %s (%zu bytes)\n", argv[4], out_len);
    rustlibresvip_free(out, out_len);

    /* Warnings of the conversion, as a JSON array. */
    if (rustlibresvip_warnings_json(&out, &out_len) == 0 && out_len > 2) {
        printf("warnings: %.*s\n", (int)out_len, out);
        rustlibresvip_free(out, out_len);
    }
    return 0;
}
