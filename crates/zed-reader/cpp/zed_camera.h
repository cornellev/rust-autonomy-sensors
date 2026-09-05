// Thin C-linkage wrapper around the ZED SDK's C++ API (sl::Camera).
//
// This SDK install has no C API (libsl_zed_c / sl/c_api/zed_interface.h are
// absent -- only the C++ libsl_zed.so is present), so we hand-wrap exactly
// the calls zed-reader needs rather than pulling in a full bindgen/cxx
// dependency for a handful of functions.
#ifndef ZED_CAMERA_H
#define ZED_CAMERA_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void* zed_camera_handle;

// Fixed to VGA (672x376) for now -- see crate README for why.
zed_camera_handle zed_camera_create(void);

// depth_mode: an sl::DEPTH_MODE value (NONE=0, NEURAL_LIGHT=4, NEURAL=5,
// NEURAL_PLUS=6; PERFORMANCE=1/QUALITY=2/ULTRA=3 exist but are deprecated in
// the SDK). NONE disables depth: zed_camera_grab will not run the depth
// pipeline, and zed_camera_get_depth_f32 will fail.
// Returns 0 on success, non-zero sl::ERROR_CODE value otherwise.
int zed_camera_open(zed_camera_handle cam, int fps, int depth_mode);

int zed_camera_grab(zed_camera_handle cam);

int zed_camera_width(zed_camera_handle cam);
int zed_camera_height(zed_camera_handle cam);

// Copies the left BGRA image into dst (caller-owned, dst_len bytes).
// Returns 0 on success, non-zero if dst_len is too small or grab has never
// succeeded.
int zed_camera_get_image_bgra(zed_camera_handle cam, uint8_t* dst, size_t dst_len);

// Copies the depth map (meters, one float per pixel) into dst
// (dst_len_floats elements). Returns -2 if depth_mode was NONE at open.
int zed_camera_get_depth_f32(zed_camera_handle cam, float* dst, size_t dst_len_floats);

uint64_t zed_camera_timestamp_ns(zed_camera_handle cam);

void zed_camera_close(zed_camera_handle cam);
void zed_camera_destroy(zed_camera_handle cam);

#ifdef __cplusplus
}
#endif

#endif
