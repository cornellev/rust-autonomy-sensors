#include "zed_shim.h"
#include <sl/Camera.hpp>
#include <cstring>

namespace {
struct ShimCamera {
    sl::Camera zed;
    sl::Mat image;
    bool opened = false;
};
}

zed_shim_handle zed_shim_create(void) {
    return new ShimCamera();
}

int zed_shim_open(zed_shim_handle handle, int fps) {
    auto* cam = static_cast<ShimCamera*>(handle);
    sl::InitParameters init_params;
    init_params.camera_resolution = sl::RESOLUTION::VGA;
    init_params.camera_fps = fps;
    init_params.depth_mode = sl::DEPTH_MODE::NONE;
    init_params.sdk_verbose = 1;

    auto err = cam->zed.open(init_params);
    cam->opened = (err == sl::ERROR_CODE::SUCCESS);
    return static_cast<int>(err);
}

int zed_shim_grab(zed_shim_handle handle) {
    auto* cam = static_cast<ShimCamera*>(handle);
    if (!cam->opened) return -1;
    sl::RuntimeParameters rt;
    auto err = cam->zed.grab(rt);
    if (err == sl::ERROR_CODE::SUCCESS) {
        cam->zed.retrieveImage(cam->image, sl::VIEW::LEFT);
    }
    return static_cast<int>(err);
}

int zed_shim_width(zed_shim_handle handle) {
    auto* cam = static_cast<ShimCamera*>(handle);
    return static_cast<int>(cam->image.getWidth());
}

int zed_shim_height(zed_shim_handle handle) {
    auto* cam = static_cast<ShimCamera*>(handle);
    return static_cast<int>(cam->image.getHeight());
}

int zed_shim_get_image_bgra(zed_shim_handle handle, uint8_t* dst, size_t dst_len) {
    auto* cam = static_cast<ShimCamera*>(handle);
    size_t needed = static_cast<size_t>(cam->image.getWidth()) *
                    static_cast<size_t>(cam->image.getHeight()) * 4;
    if (needed == 0 || needed > dst_len) return -1;
    std::memcpy(dst, cam->image.getPtr<sl::uchar1>(sl::MEM::CPU), needed);
    return 0;
}

uint64_t zed_shim_timestamp_ns(zed_shim_handle handle) {
    auto* cam = static_cast<ShimCamera*>(handle);
    return cam->zed.getTimestamp(sl::TIME_REFERENCE::IMAGE).getNanoseconds();
}

void zed_shim_close(zed_shim_handle handle) {
    auto* cam = static_cast<ShimCamera*>(handle);
    cam->zed.close();
    cam->opened = false;
}

void zed_shim_destroy(zed_shim_handle handle) {
    delete static_cast<ShimCamera*>(handle);
}
