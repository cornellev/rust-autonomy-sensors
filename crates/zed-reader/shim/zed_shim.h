// Thin C-linkage wrapper around the ZED SDK's C++ API (sl::Camera).
//
// This SDK install has no C API (libsl_zed_c / sl/c_api/zed_interface.h are
// absent -- only the C++ libsl_zed.so is present), so we hand-wrap exactly
// the calls zed-reader needs rather than pulling in a full bindgen/cxx
// dependency for a handful of functions.
#ifndef ZED_SHIM_H
#define ZED_SHIM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void* zed_shim_handle;

// Fixed to VGA (672x376) for now -- see crate README for why.
zed_shim_handle zed_shim_create(void);

// Returns 0 on success, non-zero sl::ERROR_CODE value otherwise.
int zed_shim_open(zed_shim_handle cam, int fps);

int zed_shim_grab(zed_shim_handle cam);

int zed_shim_width(zed_shim_handle cam);
int zed_shim_height(zed_shim_handle cam);

// Copies the left BGRA image into dst (caller-owned, dst_len bytes).
// Returns 0 on success, non-zero if dst_len is too small or grab has never
// succeeded.
int zed_shim_get_image_bgra(zed_shim_handle cam, uint8_t* dst, size_t dst_len);

uint64_t zed_shim_timestamp_ns(zed_shim_handle cam);

void zed_shim_close(zed_shim_handle cam);
void zed_shim_destroy(zed_shim_handle cam);

#ifdef __cplusplus
}
#endif

#endif
