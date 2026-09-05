#include "zed_camera.h"
#include <sl/Camera.hpp>
#include <cstring>

namespace {
struct Camera {
    sl::Camera zed;
    sl::Mat image;
    sl::Mat depth;
    bool opened = false;
    bool depth_enabled = false;
};
}

zed_camera_handle zed_camera_create(void) {
    return new Camera();
}

int zed_camera_open(zed_camera_handle handle, int fps, int depth_mode) {
    auto* cam = static_cast<Camera*>(handle);
    auto mode = static_cast<sl::DEPTH_MODE>(depth_mode);

    sl::InitParameters init_params;
    init_params.camera_resolution = sl::RESOLUTION::VGA;
    init_params.camera_fps = fps;
    init_params.depth_mode = mode;
    init_params.coordinate_units = sl::UNIT::METER;
    init_params.sdk_verbose = 1;

    auto err = cam->zed.open(init_params);
    cam->opened = (err == sl::ERROR_CODE::SUCCESS);
    cam->depth_enabled = (mode != sl::DEPTH_MODE::NONE);
    return static_cast<int>(err);
}

int zed_camera_grab(zed_camera_handle handle) {
    auto* cam = static_cast<Camera*>(handle);
    if (!cam->opened) return -1;
    sl::RuntimeParameters rt;
    auto err = cam->zed.grab(rt);
    if (err == sl::ERROR_CODE::SUCCESS) {
        cam->zed.retrieveImage(cam->image, sl::VIEW::LEFT);
        // Skip the depth pipeline entirely when disabled: it is a GPU
        // inference pass for NEURAL/NEURAL_PLUS, not a free extra output.
        if (cam->depth_enabled) {
            cam->zed.retrieveMeasure(cam->depth, sl::MEASURE::DEPTH);
        }
    }
    return static_cast<int>(err);
}

int zed_camera_width(zed_camera_handle handle) {
    auto* cam = static_cast<Camera*>(handle);
    return static_cast<int>(cam->image.getWidth());
}

int zed_camera_height(zed_camera_handle handle) {
    auto* cam = static_cast<Camera*>(handle);
    return static_cast<int>(cam->image.getHeight());
}

int zed_camera_get_image_bgra(zed_camera_handle handle, uint8_t* dst, size_t dst_len) {
    auto* cam = static_cast<Camera*>(handle);
    size_t needed = static_cast<size_t>(cam->image.getWidth()) *
                    static_cast<size_t>(cam->image.getHeight()) * 4;
    if (needed == 0 || needed > dst_len) return -1;
    std::memcpy(dst, cam->image.getPtr<sl::uchar1>(sl::MEM::CPU), needed);
    return 0;
}

int zed_camera_get_depth_f32(zed_camera_handle handle, float* dst, size_t dst_len_floats) {
    auto* cam = static_cast<Camera*>(handle);
    if (!cam->depth_enabled) return -2;
    size_t needed = static_cast<size_t>(cam->depth.getWidth()) *
                    static_cast<size_t>(cam->depth.getHeight());
    if (needed == 0 || needed > dst_len_floats) return -1;
    std::memcpy(dst, cam->depth.getPtr<float>(sl::MEM::CPU), needed * sizeof(float));
    return 0;
}

uint64_t zed_camera_timestamp_ns(zed_camera_handle handle) {
    auto* cam = static_cast<Camera*>(handle);
    return cam->zed.getTimestamp(sl::TIME_REFERENCE::IMAGE).getNanoseconds();
}

void zed_camera_close(zed_camera_handle handle) {
    auto* cam = static_cast<Camera*>(handle);
    cam->zed.close();
    cam->opened = false;
}

void zed_camera_destroy(zed_camera_handle handle) {
    delete static_cast<Camera*>(handle);
}
