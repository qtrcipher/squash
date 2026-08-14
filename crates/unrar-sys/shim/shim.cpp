/* Squash's shim over the vendored UnRAR dll.hpp API — see shim.h.
 *
 * New code (not part of the vendored unrar tree); the same license posture
 * applies: extraction only, never RAR creation.
 */
#include "shim.h"

#include <string>

/* dll.hpp defines HANDLE/PASCAL/etc. itself only when _UNIX or _WIN_ALL is
 * set; those normally come from raros.hpp (via rar.hpp) inside the unrar
 * build. The shim includes dll.hpp standalone, so set the platform marker. */
#ifdef _WIN32
#include <windows.h>
#else
#define _UNIX
#endif
#include "dll.hpp"

struct squash_rar {
  HANDLE handle;
  std::string name; /* UTF-8 name of the current header */
  squash_rar_write_cb cb;
  void *user;
  bool aborted;
  bool need_password;
};

/* --- UTF-8 <-> wchar_t (UTF-16 on Windows, UTF-32 on unix) ---------------- */

/* UnRAR names are bounded by NM (1024) in practice; scan at most this many
 * code units so a non-terminated FileNameW can never run the reader past the
 * header struct into adjacent heap. 8x headroom costs nothing. */
#define SQUASH_MAX_NAME_UNITS 8192

static void append_utf8(std::string &out, uint32_t cp) {
  if (cp < 0x80) {
    out += (char)cp;
  } else if (cp < 0x800) {
    out += (char)(0xC0 | (cp >> 6));
    out += (char)(0x80 | (cp & 0x3F));
  } else if (cp < 0x10000) {
    out += (char)(0xE0 | (cp >> 12));
    out += (char)(0x80 | ((cp >> 6) & 0x3F));
    out += (char)(0x80 | (cp & 0x3F));
  } else {
    out += (char)(0xF0 | (cp >> 18));
    out += (char)(0x80 | ((cp >> 12) & 0x3F));
    out += (char)(0x80 | ((cp >> 6) & 0x3F));
    out += (char)(0x80 | (cp & 0x3F));
  }
}

static std::string wide_to_utf8(const wchar_t *w) {
  std::string out;
  if (w == nullptr)
    return out;
  size_t units = 0;
  if (sizeof(wchar_t) == 2) {
    const uint16_t *p = (const uint16_t *)w;
    while (*p && units++ < SQUASH_MAX_NAME_UNITS) {
      uint32_t cp = *p++;
      if (cp >= 0xD800 && cp <= 0xDBFF && p[0] >= 0xDC00 && p[0] <= 0xDFFF)
        cp = 0x10000 + ((cp - 0xD800) << 10) + (*p++ - 0xDC00);
      append_utf8(out, cp);
    }
  } else {
    const uint32_t *p = (const uint32_t *)w;
    while (*p && units++ < SQUASH_MAX_NAME_UNITS)
      append_utf8(out, *p++);
  }
  return out;
}

#ifdef _WIN32
static std::wstring utf8_to_wide(const char *s) {
  int n = MultiByteToWideChar(CP_UTF8, 0, s, -1, nullptr, 0);
  if (n <= 0)
    return std::wstring();
  /* n counts the terminating NUL the API writes — the buffer must hold it. */
  std::wstring w((size_t)n, L'\0');
  MultiByteToWideChar(CP_UTF8, 0, s, -1, &w[0], n); // &w[0]: C++11-safe
  w.resize((size_t)n - 1);                          // drop the NUL from the body
  return w;
}
#endif

/* --- callback bridge ------------------------------------------------------ */

/* UCM_PROCESSDATA chunk-size sanity bound. UnRAR hands over slices of its
 * own decode buffer (well under 1 MiB in practice); a negative or absurd
 * p2 would become a huge size_t and an out-of-bounds slice on the Rust
 * side, so refuse instead of forwarding. */
#define SQUASH_MAX_PROCESS_CHUNK ((long long)64 * 1024 * 1024)

static int CALLBACK on_unrar_msg(UINT msg, LPARAM user, LPARAM p1, LPARAM p2) {
  squash_rar *arc = (squash_rar *)user;
  switch (msg) {
  case UCM_PROCESSDATA:
    if (p2 < 0 || (long long)p2 > SQUASH_MAX_PROCESS_CHUNK) {
      arc->aborted = true;
      return -1;
    }
    if (arc->cb != nullptr &&
        arc->cb((const unsigned char *)p1, (size_t)p2, arc->user) != 0) {
      arc->aborted = true;
      return -1;
    }
    return 1;
  case UCM_NEEDPASSWORD:
  case UCM_NEEDPASSWORDW:
    arc->need_password = true;
    return -1;
  default:
    /* UCM_CHANGEVOLUME*: multi-volume archives are unsupported (v1) — abort.
     * UCM_LARGEDICT: refuse rather than allocate a huge dictionary. */
    arc->aborted = true;
    return -1;
  }
}

/* --- API ------------------------------------------------------------------ */

int squash_rar_open(const char *path, int list_only, squash_rar **out) {
  *out = nullptr;
  RAROpenArchiveDataEx data{};
#ifdef _WIN32
  /* ArcName is ANSI on Windows; use the wide field for full Unicode paths.
   * (On unix ArcName is UTF-8, which is what we pass.) */
  std::wstring wide_path = utf8_to_wide(path);
  data.ArcNameW = const_cast<wchar_t *>(wide_path.c_str());
#endif
  data.ArcName = const_cast<char *>(path);
  data.OpenMode = list_only ? RAR_OM_LIST : RAR_OM_EXTRACT;

  HANDLE handle = RAROpenArchiveEx(&data);
  if (handle == nullptr)
    return data.OpenResult != 0 ? (int)data.OpenResult : ERAR_EOPEN;
  if (data.OpenResult != 0) {
    RARCloseArchive(handle);
    return (int)data.OpenResult;
  }
  squash_rar *arc = new squash_rar();
  arc->handle = handle;
  arc->cb = nullptr;
  arc->user = nullptr;
  arc->aborted = false;
  arc->need_password = false;
  *out = arc;
  return 0;
}

int squash_rar_next(squash_rar *arc, const char **name, uint64_t *size,
                    int *is_dir, int *is_encrypted) {
  RARHeaderDataEx header{};
  int rc = RARReadHeaderEx(arc->handle, &header);
  if (rc != 0)
    return rc;
  arc->name = wide_to_utf8(header.FileNameW);
  if (arc->name.empty())
    arc->name = header.FileName; /* last resort: raw bytes as stored */
  *name = arc->name.c_str();
  *size = ((uint64_t)header.UnpSizeHigh << 32) | header.UnpSize;
  *is_dir = (header.Flags & RHDF_DIRECTORY) != 0;
  *is_encrypted = (header.Flags & RHDF_ENCRYPTED) != 0;
  return 0;
}

int squash_rar_extract(squash_rar *arc, squash_rar_write_cb cb, void *user) {
  arc->cb = cb;
  arc->user = user;
  arc->aborted = false;
  arc->need_password = false;
  RARSetCallback(arc->handle, on_unrar_msg, (LPARAM)arc);
  /* RAR_TEST decodes the entry (driving UCM_PROCESSDATA) without unrar ever
   * creating files — Squash owns every byte written to disk. */
  int rc = RARProcessFile(arc->handle, RAR_TEST, nullptr, nullptr);
  arc->cb = nullptr;
  RARSetCallback(arc->handle, nullptr, 0);
  if (arc->need_password)
    return ERAR_MISSING_PASSWORD;
  if (arc->aborted)
    return SQUASH_RAR_ABORTED;
  return rc;
}

int squash_rar_skip(squash_rar *arc) {
  return RARProcessFile(arc->handle, RAR_SKIP, nullptr, nullptr);
}

void squash_rar_close(squash_rar *arc) {
  if (arc == nullptr)
    return;
  RARCloseArchive(arc->handle);
  delete arc;
}
