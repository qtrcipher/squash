/* Squash's C ABI shim over the vendored UnRAR dll.hpp API (extraction only).
 *
 * The dll.hpp structs are `#pragma pack(1)` and use `wchar_t` (2 bytes on
 * Windows, 4 on unix) — both are painful to model in Rust FFI. This shim
 * exposes plain values and UTF-8 strings instead, so src/lib.rs declares a
 * minimal, naturally-aligned surface.
 *
 * Return codes are the dll.hpp ERAR_* values plus SQUASH_RAR_ABORTED.
 */
#ifndef SQUASH_UNRAR_SHIM_H
#define SQUASH_UNRAR_SHIM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct squash_rar squash_rar;

/* Not an ERAR_* code: the Rust data callback aborted the current entry. */
#define SQUASH_RAR_ABORTED 1000

/* `list_only` != 0 opens in RAR_OM_LIST mode (metadata pass). Returns an
 * ERAR_* code; on success *out holds the archive handle. */
int squash_rar_open(const char *path, int list_only, squash_rar **out);

/* Reads the next header. Returns 0 and fills the outputs, 10
 * (ERAR_END_ARCHIVE) at the end of the archive, or another ERAR_* code.
 * `name` is UTF-8 and valid until the next shim call on this handle. */
int squash_rar_next(squash_rar *arc, const char **name, uint64_t *size,
                    int *is_dir, int *is_encrypted);

/* Called with each decoded data chunk; return 0 to continue, non-zero to
 * abort (squash_rar_extract then returns SQUASH_RAR_ABORTED). */
typedef int (*squash_rar_write_cb)(const unsigned char *data, size_t size,
                                   void *user);

/* Decodes the current entry in RAR_TEST mode (unrar itself never touches the
 * filesystem; the callback receives the bytes). Returns an ERAR_* code or
 * SQUASH_RAR_ABORTED. */
int squash_rar_extract(squash_rar *arc, squash_rar_write_cb cb, void *user);

/* Skips the current entry (RAR_SKIP). */
int squash_rar_skip(squash_rar *arc);

void squash_rar_close(squash_rar *arc);

#ifdef __cplusplus
}
#endif

#endif
