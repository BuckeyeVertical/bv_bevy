mod debug_camera;
mod drone_camera;
mod frame_stream;

pub use debug_camera::{DebugCamera, DebugCameraPlugin};
pub use drone_camera::{
    CameraFrame, CameraFrameReady, DroneCameraConfig, DroneCameraPlugin, OnboardCamera,
    PinholeIntrinsics,
};
pub use frame_stream::CameraFrameServerPlugin;

/// Render layer of the drone model: the debug camera sees it, the onboard camera does not.
pub const VEHICLE_RENDER_LAYER: usize = 1;
/// Render layer of world overlays (boundaries, routes) that only the debug camera shows.
pub const WORLD_DEBUG_RENDER_LAYER: usize = 2;
